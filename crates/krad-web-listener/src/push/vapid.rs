use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, KeyPair as _, ECDSA_P256_SHA256_FIXED_SIGNING};
use std::fs;
use std::io;
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const KEY_FILE: &str = "web-push.key";
const OWNER_ONLY: u32 = 0o600;
const HEADER: &str = r#"{"typ":"JWT","alg":"ES256"}"#;
const SUBJECT: &str = "mailto:web-push@krad.invalid";
const VALID_FOR: Duration = Duration::from_hours(12);
const SECURE_SCHEME: &str = "https://";

#[derive(Debug, thiserror::Error)]
pub(crate) enum VapidError {
    #[error("the push key could not be read or written")]
    Io(#[from] io::Error),
    #[error("the push key is not valid")]
    Key,
}

pub(crate) struct Vapid {
    pair: EcdsaKeyPair,
}

fn create(path: &Path, random: &SystemRandom) -> Result<Vec<u8>, VapidError> {
    let document = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, random)
        .map_err(|_| VapidError::Key)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(OWNER_ONLY)
        .open(path)?;
    file.write_all(document.as_ref())?;
    Ok(document.as_ref().to_vec())
}

pub(crate) fn origin_of(endpoint: &str) -> Option<&str> {
    let rest = endpoint.strip_prefix(SECURE_SCHEME)?;
    let host = rest.split('/').next().filter(|host| !host.is_empty())?;
    endpoint.get(..SECURE_SCHEME.len() + host.len())
}

impl Vapid {
    pub(crate) fn open(root: &Path, random: &SystemRandom) -> Result<Self, VapidError> {
        let path = root.join(KEY_FILE);
        let pkcs8 = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => create(&path, random)?,
            Err(error) => return Err(error.into()),
        };
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &pkcs8, random)
            .map_err(|_| VapidError::Key)?;
        Ok(Self { pair })
    }

    pub(crate) fn public_key(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.pair.public_key().as_ref())
    }

    pub(crate) fn authorization(
        &self,
        audience: &str,
        random: &SystemRandom,
    ) -> Result<String, VapidError> {
        let expires = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .saturating_add(VALID_FOR)
            .as_secs();
        let claims = serde_json::json!({ "aud": audience, "exp": expires, "sub": SUBJECT });
        let unsigned = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(HEADER),
            URL_SAFE_NO_PAD.encode(claims.to_string())
        );
        let signature = self
            .pair
            .sign(random, unsigned.as_bytes())
            .map_err(|_| VapidError::Key)?;
        Ok(format!(
            "vapid t={unsigned}.{}, k={}",
            URL_SAFE_NO_PAD.encode(signature.as_ref()),
            self.public_key()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_secure_endpoints_have_an_origin() {
        assert_eq!(
            origin_of("https://web.push.apple.com/QGx4?x=1"),
            Some("https://web.push.apple.com")
        );
        assert_eq!(origin_of("http://127.0.0.1:9/push"), None);
        assert_eq!(origin_of("https:///nohost"), None);
    }
}
