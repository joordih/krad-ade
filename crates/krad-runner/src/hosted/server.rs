use krad_domain::InstallId;
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
use super::pairing_codes::PairingCodes;
use crate::pairings::unpair;

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

pub struct RuntimeServer<S> {
    pub(crate) handle: RuntimeHandle,
    pub(crate) root: PathBuf,
    pub(crate) codes: PairingCodes,
    connections: AtomicU64,
    live: Mutex<HashMap<InstallId, HashMap<ConnectionKey, S>>>,
}

impl<S: Stream> RuntimeServer<S> {
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

    pub fn serve(self: &Arc<Self>, listener: &impl Listener<Stream = S>) {
        loop {
            let stream = match listener.accept() {
                Ok(stream) => stream,
                Err(error) => {
                    tracing::debug!(%error, "a runtime connection was not accepted");
                    continue;
                }
            };
            let server = Arc::clone(self);
            let spawned = thread::Builder::new()
                .name(CONNECTION_THREAD.to_owned())
                .spawn(move || serve_connection(&server, stream));
            if let Err(error) = spawned {
                tracing::warn!(%error, "could not serve a runtime connection");
            }
        }
    }

    pub(crate) fn next_key(&self) -> ConnectionKey {
        ConnectionKey(self.connections.fetch_add(1, Ordering::Relaxed))
    }

    fn live(&self) -> MutexGuard<'_, HashMap<InstallId, HashMap<ConnectionKey, S>>> {
        self.live.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn admit(&self, install: &InstallId, key: ConnectionKey, stream: S) {
        self.live()
            .entry(install.clone())
            .or_default()
            .insert(key, stream);
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
        streams.values().for_each(Stream::close);
        tracing::info!(install = install.as_str(), "revoked a pairing");
        Ok(removed)
    }
}
