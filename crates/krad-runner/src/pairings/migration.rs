use krad_domain::{DeviceName, InstallId, PairingPermissions, PairingToken, Timestamp};
use krad_host::runner::{clients_dir, pairing_file};
use std::fs;
use std::path::{Path, PathBuf};

use super::stored::{write_stored, StoredPairing, TokenHash};

const PLAIN_TOKEN_EXTENSION: &str = "token";
pub(crate) const SSH_DEVICE: &str = "paired over SSH";

fn plain_tokens(root: &Path) -> Vec<(InstallId, PathBuf)> {
    let Ok(entries) = fs::read_dir(clients_dir(root)) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext == PLAIN_TOKEN_EXTENSION)
        })
        .filter_map(|path| Some((path.file_stem()?.to_str()?.parse().ok()?, path)))
        .collect()
}

pub(crate) fn migrate_plain_tokens(root: &Path) {
    for (install, path) in plain_tokens(root) {
        let Some(token) = fs::read_to_string(&path)
            .ok()
            .and_then(|text| text.parse::<PairingToken>().ok())
        else {
            continue;
        };
        let Ok(device) = SSH_DEVICE.parse::<DeviceName>() else {
            continue;
        };
        let stored = StoredPairing {
            token_hash: TokenHash::of(&token),
            device,
            paired_at: Timestamp::now(),
            last_used: None,
            permissions: PairingPermissions::all(),
        };
        let kept = write_stored(&pairing_file(root, &install), &stored)
            .and_then(|()| fs::remove_file(&path));
        match kept {
            Ok(()) => tracing::info!(
                install = install.as_str(),
                "kept a plain pairing token as its hash"
            ),
            Err(error) => tracing::warn!(%error, "a plain pairing token could not be hashed"),
        }
    }
}
