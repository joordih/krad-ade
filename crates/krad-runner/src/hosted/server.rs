use krad_domain::{
    DeviceName, InstallId, PairingCode, PairingPermissions, PairingToken, Timestamp,
};
use krad_protocol::RequestId;
use krad_runtime::RuntimeHandle;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use super::connection::serve_connection;
use super::listener::{Listener, Stream};
use super::pairing_codes::{PairingCodes, Redeemed};
use crate::pairings::{authorize, pair, unpair};

const CONNECTION_THREAD: &str = "krad-runner-connection";
const REQUEST_SPACE_BITS: u32 = 32;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct ConnectionKey(u64);

impl ConnectionKey {
    pub(crate) const fn first_request(self) -> RequestId {
        RequestId(self.0 << REQUEST_SPACE_BITS)
    }

    pub(crate) const fn owns(self, request: RequestId) -> bool {
        request.0 >> REQUEST_SPACE_BITS == self.0
    }
}

type Closer = Box<dyn Fn() + Send>;

#[derive(Debug, thiserror::Error)]
pub enum RedeemRefusal {
    #[error("that pairing code is unknown, used or expired")]
    UnknownCode,
    #[error("too many pairing codes were tried, wait a minute")]
    TooManyTries,
    #[error("the runtime could not keep the pairing")]
    NotKept(#[source] io::Error),
}

pub struct RuntimeServer {
    pub handle: RuntimeHandle,
    pub(crate) root: PathBuf,
    pub(crate) codes: PairingCodes,
    connections: AtomicU64,
    live: Mutex<HashMap<InstallId, HashMap<ConnectionKey, Closer>>>,
}

impl RuntimeServer {
    #[must_use]
    pub fn new(handle: RuntimeHandle, root: &Path) -> Arc<Self> {
        Arc::new(Self {
            handle,
            root: root.to_owned(),
            codes: PairingCodes::default(),
            connections: AtomicU64::new(1),
            live: Mutex::new(HashMap::new()),
        })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn serve<L: Listener>(self: &Arc<Self>, listener: &L) {
        loop {
            match listener.accept() {
                Ok(stream) => self.serve_stream(stream),
                Err(error) => tracing::debug!(%error, "a runtime connection was not accepted"),
            }
        }
    }

    pub fn serve_stream<S: Stream>(self: &Arc<Self>, stream: S) {
        let server = Arc::clone(self);
        let spawned = thread::Builder::new()
            .name(CONNECTION_THREAD.to_owned())
            .spawn(move || serve_connection(&server, stream));
        if let Err(error) = spawned {
            tracing::warn!(%error, "could not serve a runtime connection");
        }
    }

    #[must_use]
    pub fn offer_pairing_code(
        &self,
        permissions: PairingPermissions,
    ) -> Option<(PairingCode, Timestamp)> {
        self.codes.offer(permissions)
    }

    pub(crate) fn next_key(&self) -> ConnectionKey {
        ConnectionKey(self.connections.fetch_add(1, Ordering::Relaxed))
    }

    fn live(&self) -> MutexGuard<'_, HashMap<InstallId, HashMap<ConnectionKey, Closer>>> {
        self.live.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn admit(&self, install: &InstallId, key: ConnectionKey, stream: impl Stream) {
        self.live()
            .entry(install.clone())
            .or_default()
            .insert(key, Box::new(move || stream.close()));
    }

    #[must_use]
    pub fn authorize(&self, pairing: &krad_domain::Pairing) -> Option<PairingPermissions> {
        authorize(&self.root, pairing)
    }

    /// # Errors
    ///
    /// Refuses an unknown, used or expired code, any code while too many were tried, and a
    /// pairing that could not be written.
    pub fn redeem(
        &self,
        code: &PairingCode,
        install: &InstallId,
        device: DeviceName,
    ) -> Result<(PairingToken, PairingPermissions), RedeemRefusal> {
        let permissions = match self.codes.redeem(code) {
            Redeemed::Granted(permissions) => permissions,
            Redeemed::Unknown => return Err(RedeemRefusal::UnknownCode),
            Redeemed::TooManyTries => return Err(RedeemRefusal::TooManyTries),
        };
        let token = pair(&self.root, install, device, permissions.clone())
            .map_err(RedeemRefusal::NotKept)?;
        tracing::info!(install = install.as_str(), "paired a device with a code");
        Ok((token, permissions))
    }

    pub(crate) fn leave(&self, install: &InstallId, key: ConnectionKey) {
        let mut live = self.live();
        let Some(streams) = live.get_mut(install) else {
            return;
        };
        streams.remove(&key);
        if streams.is_empty() {
            live.remove(install);
        }
    }

    pub(crate) fn revoke(&self, install: &InstallId) -> io::Result<bool> {
        let removed = unpair(&self.root, install)?;
        let streams = self.live().remove(install).unwrap_or_default();
        for close in streams.values() {
            close();
        }
        tracing::info!(install = install.as_str(), "revoked a pairing");
        Ok(removed)
    }
}
