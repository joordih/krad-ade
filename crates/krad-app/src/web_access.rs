use krad_domain::{PairingCode, PairingPermissions};
use krad_host::runner::ensure_private_dir;
use krad_runner::RuntimeServer;
use krad_runtime::RuntimeHandle;
use krad_ui::WebAccessHost;
use krad_web_listener::{
    read_web_access, serve_web_access, turn_off_web_access, ServedWebAccess, WebAccess,
};
use std::io::{self, Write as _};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::awake::{keep_awake_for_agents, KeptAwake};
use crate::tailnet::tailnet_web_access;

pub(crate) const WEB_ACCESS_VERB: &str = "web-access";
const WEB_ACCESS_FOLDER: &str = "runtime";
const IS_OFF: &str = "web access is off";
const NO_CODE: &str = "no pairing code could be made";

pub(crate) fn web_access_folder(config: &Path) -> PathBuf {
    config.join(WEB_ACCESS_FOLDER)
}

fn web_access_said(rest: &[String]) -> io::Result<String> {
    let root = web_access_folder(&crate::desktop_runtime::config_folder()?);
    ensure_private_dir(&root)?;
    krad_web_listener::web_access_command(rest, &root)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))
}

pub(crate) fn run_web_access_verb(rest: &[String]) -> i32 {
    let said = web_access_said(rest).and_then(|said| {
        let mut out = io::stdout().lock();
        writeln!(out, "{said}")?;
        out.flush()
    });
    said.map_or_else(
        |error| krad_host::runner::report_verb_failure(&error.to_string()),
        |()| krad_host::runner::EXIT_OK,
    )
}

struct Serving {
    served: ServedWebAccess,
    address: String,
    _awake: KeptAwake,
}

pub(crate) struct AppWebAccess {
    root: PathBuf,
    server: Arc<RuntimeServer>,
    serving: Mutex<Option<Serving>>,
}

fn address_of(access: &WebAccess, bound: SocketAddr) -> String {
    match access.names.first().filter(|_| access.secure()) {
        Some(name) => format!("https://{name}:{}", bound.port()),
        None => format!("http://{bound}"),
    }
}

impl AppWebAccess {
    /// # Errors
    ///
    /// Fails when the web access folder cannot be made private.
    pub(crate) fn new(runtime: RuntimeHandle, root: PathBuf) -> io::Result<Self> {
        ensure_private_dir(&root)?;
        Ok(Self {
            server: RuntimeServer::new(runtime, &root),
            root,
            serving: Mutex::new(None),
        })
    }

    fn serving(&self) -> MutexGuard<'_, Option<Serving>> {
        self.serving.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn restore(&self) {
        match read_web_access(&self.root) {
            Ok(Some(access)) => {
                if let Err(error) = self.serve(&access) {
                    tracing::warn!(%error, "web access did not start");
                }
            }
            Ok(None) => tracing::debug!("web access is off"),
            Err(error) => tracing::warn!(%error, "the web access settings could not be read"),
        }
    }

    fn stop(&self) {
        if let Some(serving) = self.serving().take() {
            serving.served.stop();
        }
    }

    fn serve(&self, access: &WebAccess) -> io::Result<()> {
        self.stop();
        let served = serve_web_access(access, &self.server).map_err(io::Error::other)?;
        let awake = match keep_awake_for_agents(self.server.handle.clone()) {
            Ok(awake) => awake,
            Err(error) => {
                served.stop();
                return Err(error);
            }
        };
        *self.serving() = Some(Serving {
            address: address_of(access, served.address),
            served,
            _awake: awake,
        });
        Ok(())
    }
}

impl WebAccessHost for AppWebAccess {
    fn address(&self) -> Option<String> {
        self.serving()
            .as_ref()
            .map(|serving| serving.address.clone())
    }

    fn turn_on(&self) -> io::Result<()> {
        let access = tailnet_web_access(&self.root)?;
        self.serve(&access)
    }

    fn turn_off(&self) -> io::Result<()> {
        self.stop();
        turn_off_web_access(&self.root).map(drop)
    }

    fn pairing_code(&self, permissions: PairingPermissions) -> io::Result<PairingCode> {
        if self.serving().is_none() {
            return Err(io::Error::new(io::ErrorKind::NotConnected, IS_OFF));
        }
        self.server
            .offer_pairing_code(permissions)
            .map(|(code, _)| code)
            .ok_or_else(|| io::Error::other(NO_CODE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::PairingPermission;
    use krad_runtime::{Runtime, RuntimeSetup};
    use krad_web_listener::{web_access_file, write_web_access};
    use std::net::TcpStream;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn loopback() -> Result<WebAccess, std::net::AddrParseError> {
        Ok(WebAccess {
            bind: "127.0.0.1:0".parse()?,
            tls: None,
            names: Vec::new(),
        })
    }

    #[test]
    fn saved_web_access_is_served_at_start_and_turned_off_with_its_file() -> TestResult {
        let root = std::env::temp_dir().join(format!("krad-app-web-{}", std::process::id()));
        let web = AppWebAccess::new(Runtime::start(RuntimeSetup::default())?, root.clone())?;
        assert_eq!(web.address(), None);
        assert!(web
            .pairing_code(PairingPermissions::from_iter([PairingPermission::Read]))
            .is_err());
        write_web_access(&root, &loopback()?)?;
        web.restore();
        let address = web.address().ok_or("web access was not restored")?;
        let bound = address.trim_start_matches("http://");
        TcpStream::connect(bound)?;
        web.pairing_code(PairingPermissions::from_iter([PairingPermission::Read]))?;
        web.turn_off()?;
        assert_eq!(web.address(), None);
        assert!(!web_access_file(&root).exists());
        std::fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn a_secure_access_is_addressed_by_its_tailnet_name() -> TestResult {
        let access = WebAccess {
            bind: "100.101.102.103:7421".parse()?,
            tls: Some(krad_web_listener::TlsFiles {
                certificate: PathBuf::from("web.crt"),
                key: PathBuf::from("web.key"),
            }),
            names: vec!["laptop.tailnet.ts.net".to_owned()],
        };
        assert_eq!(
            address_of(&access, access.bind),
            "https://laptop.tailnet.ts.net:7421"
        );
        assert_eq!(
            address_of(&loopback()?, "127.0.0.1:7421".parse()?),
            "http://127.0.0.1:7421"
        );
        Ok(())
    }
}
