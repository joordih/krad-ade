#[cfg(any(test, feature = "runtime"))]
mod devices;
mod stored;

#[cfg(any(test, feature = "runtime"))]
pub(crate) use devices::paired_devices;

use krad_domain::{DeviceName, InstallId, Pairing, PairingPermissions, PairingToken, Timestamp};
use krad_host::new_pairing_token;
use krad_host::runner::{clients_dir, ensure_private_dir, pairing_file, webhooks_file};
use std::fs;
use std::io;
use std::path::Path;

use self::stored::{read_stored, write_stored, StoredPairing, TokenHash};

pub(crate) const SSH_DEVICE: &str = "paired over SSH";
const BAD_TOKEN: &str = "the cloud runner could not make a pairing token";

pub(crate) fn pair(
    root: &Path,
    install: &InstallId,
    device: DeviceName,
    permissions: PairingPermissions,
) -> io::Result<PairingToken> {
    let token = new_pairing_token().ok_or_else(|| io::Error::other(BAD_TOKEN))?;
    ensure_private_dir(root)?;
    ensure_private_dir(&clients_dir(root))?;
    let stored = StoredPairing {
        token_hash: TokenHash::of(&token),
        device,
        paired_at: Timestamp::now(),
        last_used: None,
        permissions,
    };
    write_stored(&pairing_file(root, install), &stored)?;
    Ok(token)
}

fn remove_if_present(path: &Path) -> io::Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(crate) fn unpair(root: &Path, install: &InstallId) -> io::Result<bool> {
    let removed = remove_if_present(&pairing_file(root, install))?;
    remove_if_present(&webhooks_file(root, install))?;
    Ok(removed)
}

pub(crate) fn authorize(root: &Path, pairing: &Pairing) -> Option<PairingPermissions> {
    let path = pairing_file(root, &pairing.install);
    let mut stored = read_stored(&path)?;
    if !stored.token_hash.matches(&pairing.token) {
        return None;
    }
    stored.last_used = Some(Timestamp::now());
    if let Err(error) = write_stored(&path, &stored) {
        tracing::debug!(%error, "the last use of a pairing was not kept");
    }
    Some(stored.permissions)
}

#[cfg(test)]
mod tests;
