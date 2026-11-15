use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use crate::reach::{is_loopback, is_private_network};

const WEB_ACCESS_FILE: &str = "web-access.json";

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TlsFiles {
    pub certificate: PathBuf,
    pub key: PathBuf,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WebAccess {
    pub bind: SocketAddr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsFiles>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum WebAccessError {
    #[error("web access only listens on loopback or the private network, not on {0}")]
    PublicBind(SocketAddr),
    #[error("web access off loopback needs a certificate and its key")]
    NeedsTls,
    #[error("the web access settings could not be read or written")]
    Io(#[from] io::Error),
    #[error("the web access settings are not valid")]
    Json(#[from] serde_json::Error),
    #[error("the certificate or its key could not be loaded")]
    Certificate(#[source] rustls::Error),
    #[error("the certificate file holds no certificate or key")]
    NoCertificate,
}

impl WebAccess {
    /// # Errors
    ///
    /// Refuses a bind outside loopback and the private network, and a bind off loopback
    /// without a certificate.
    pub fn check(&self) -> Result<(), WebAccessError> {
        let ip = self.bind.ip();
        if is_loopback(ip) {
            return Ok(());
        }
        if !is_private_network(ip) {
            return Err(WebAccessError::PublicBind(self.bind));
        }
        if self.tls.is_none() {
            return Err(WebAccessError::NeedsTls);
        }
        Ok(())
    }

    #[must_use]
    pub const fn secure(&self) -> bool {
        self.tls.is_some()
    }
}

#[must_use]
pub fn web_access_file(root: &Path) -> PathBuf {
    root.join(WEB_ACCESS_FILE)
}

/// # Errors
///
/// Fails when the file exists but cannot be read or does not hold valid settings.
pub fn read_web_access(root: &Path) -> Result<Option<WebAccess>, WebAccessError> {
    match fs::read(web_access_file(root)) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// # Errors
///
/// Refuses settings that fail [`WebAccess::check`], and fails when the file cannot be written.
pub fn write_web_access(root: &Path, access: &WebAccess) -> Result<(), WebAccessError> {
    access.check()?;
    fs::write(web_access_file(root), serde_json::to_vec_pretty(access)?)?;
    Ok(())
}

/// # Errors
///
/// Fails when the file exists and cannot be removed.
pub fn turn_off_web_access(root: &Path) -> io::Result<bool> {
    match fs::remove_file(web_access_file(root)) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access(bind: &str, tls: bool) -> Result<WebAccess, std::net::AddrParseError> {
        Ok(WebAccess {
            bind: bind.parse()?,
            tls: tls.then(|| TlsFiles {
                certificate: PathBuf::from("machine.crt"),
                key: PathBuf::from("machine.key"),
            }),
            names: Vec::new(),
        })
    }

    #[test]
    fn loopback_needs_no_certificate() -> Result<(), Box<dyn std::error::Error>> {
        access("127.0.0.1:7421", false)?.check()?;
        access("[::1]:7421", false)?.check()?;
        Ok(())
    }

    #[test]
    fn the_tailnet_needs_a_certificate() -> Result<(), Box<dyn std::error::Error>> {
        assert!(matches!(
            access("100.101.102.103:7421", false)?.check(),
            Err(WebAccessError::NeedsTls)
        ));
        access("100.101.102.103:7421", true)?.check()?;
        access("[fd7a:115c:a1e0::1]:7421", true)?.check()?;
        Ok(())
    }

    #[test]
    fn a_public_or_wildcard_bind_is_refused_even_with_a_certificate(
    ) -> Result<(), Box<dyn std::error::Error>> {
        for bind in [
            "0.0.0.0:7421",
            "203.0.113.7:7421",
            "[::]:7421",
            "192.168.1.4:7421",
        ] {
            assert!(matches!(
                access(bind, true)?.check(),
                Err(WebAccessError::PublicBind(_))
            ));
        }
        Ok(())
    }
}
