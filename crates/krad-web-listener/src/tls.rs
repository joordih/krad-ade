use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use std::fs;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use crate::config::{TlsFiles, WebAccessError};

const HTTP_1_1: &[u8] = b"http/1.1";

pub(crate) enum Wire {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ServerConnection, TcpStream>>),
}

impl Wire {
    pub(crate) fn socket(&self) -> &TcpStream {
        match self {
            Self::Plain(socket) => socket,
            Self::Tls(tls) => &tls.sock,
        }
    }

    pub(crate) fn set_read_timeout(&self, timeout: Duration) -> io::Result<()> {
        self.socket().set_read_timeout(Some(timeout))
    }
}

impl Read for Wire {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(socket) => socket.read(buf),
            Self::Tls(tls) => tls.read(buf),
        }
    }
}

impl Write for Wire {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(socket) => socket.write(buf),
            Self::Tls(tls) => tls.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(socket) => socket.flush(),
            Self::Tls(tls) => tls.flush(),
        }
    }
}

struct Loaded {
    config: Arc<ServerConfig>,
    changed: (Option<SystemTime>, Option<SystemTime>),
}

pub(crate) struct Certificates {
    files: TlsFiles,
    loaded: Mutex<Loaded>,
}

fn modified(path: &std::path::Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

fn load(files: &TlsFiles) -> Result<Arc<ServerConfig>, WebAccessError> {
    let chain = CertificateDer::pem_file_iter(&files.certificate)
        .map_err(|_| WebAccessError::NoCertificate)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| WebAccessError::NoCertificate)?;
    if chain.is_empty() {
        return Err(WebAccessError::NoCertificate);
    }
    let key =
        PrivateKeyDer::from_pem_file(&files.key).map_err(|_| WebAccessError::NoCertificate)?;
    let mut config =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(WebAccessError::Certificate)?
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .map_err(WebAccessError::Certificate)?;
    config.alpn_protocols = vec![HTTP_1_1.to_vec()];
    Ok(Arc::new(config))
}

impl Certificates {
    pub(crate) fn load(files: TlsFiles) -> Result<Self, WebAccessError> {
        let config = load(&files)?;
        let changed = (modified(&files.certificate), modified(&files.key));
        Ok(Self {
            files,
            loaded: Mutex::new(Loaded { config, changed }),
        })
    }

    fn loaded(&self) -> MutexGuard<'_, Loaded> {
        self.loaded.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn current(&self) -> Arc<ServerConfig> {
        let changed = (modified(&self.files.certificate), modified(&self.files.key));
        let mut loaded = self.loaded();
        if loaded.changed != changed {
            match load(&self.files) {
                Ok(config) => {
                    tracing::info!("web access reloaded its certificate");
                    *loaded = Loaded { config, changed };
                }
                Err(error) => {
                    tracing::warn!(%error, "web access kept its previous certificate");
                }
            }
        }
        Arc::clone(&loaded.config)
    }

    pub(crate) fn wrap(&self, socket: TcpStream) -> io::Result<Wire> {
        let connection = ServerConnection::new(self.current()).map_err(io::Error::other)?;
        Ok(Wire::Tls(Box::new(StreamOwned::new(connection, socket))))
    }
}
