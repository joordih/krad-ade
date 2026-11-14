use krad_domain::{DeviceName, PairingPermissions, PairingToken, Timestamp};
use krad_host::FileMode;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

const TMP_EXTENSION: &str = "tmp";

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct TokenHash(String);

impl TokenHash {
    pub(super) fn of(token: &PairingToken) -> Self {
        let digest = Sha256::digest(token.expose().as_bytes());
        Self(digest.iter().fold(String::new(), |mut hex, byte| {
            write!(hex, "{byte:02x}").ok();
            hex
        }))
    }

    pub(super) fn matches(&self, token: &PairingToken) -> bool {
        same_secret(self.0.as_bytes(), Self::of(token).0.as_bytes())
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub(super) struct StoredPairing {
    pub(super) token_hash: TokenHash,
    pub(super) device: DeviceName,
    pub(super) paired_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) last_used: Option<Timestamp>,
    pub(super) permissions: PairingPermissions,
}

pub(super) fn read_stored(path: &Path) -> Option<StoredPairing> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes)
        .inspect_err(|error| tracing::debug!(%error, "an unreadable pairing was ignored"))
        .ok()
}

pub(super) fn write_stored(path: &Path, stored: &StoredPairing) -> io::Result<()> {
    let tmp = path.with_extension(TMP_EXTENSION);
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(FileMode::PRIVATE_FILE.0)
        .open(&tmp)?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(FileMode::PRIVATE_FILE.0))?;
    file.write_all(&serde_json::to_vec(stored)?)?;
    file.flush()?;
    fs::rename(tmp, path)
}

fn same_secret(stored: &[u8], offered: &[u8]) -> bool {
    stored.len() == offered.len()
        && stored
            .iter()
            .zip(offered)
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_match_only_byte_for_byte() {
        assert!(same_secret(b"abcd", b"abcd"));
        assert!(!same_secret(b"abcd", b"abce"));
        assert!(!same_secret(b"abcd", b"abc"));
    }
}
