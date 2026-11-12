use krad_domain::{InstallId, Pairing, PairingToken};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::events::webhooks_file;
use crate::mode::FileMode;
use crate::random::{random_hex, TOKEN_BYTES};

const CLIENTS_DIR: &str = "clients";
const TOKEN_EXTENSION: &str = "token";
const BAD_TOKEN: &str = "the cloud runner made an invalid pairing token";

pub(super) fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(FileMode::PRIVATE_DIR.0))
}

fn token_file(root: &Path, install: &InstallId) -> PathBuf {
    root.join(CLIENTS_DIR)
        .join(install.as_str())
        .with_extension(TOKEN_EXTENSION)
}

pub(super) fn pair(root: &Path, install: &InstallId) -> io::Result<PairingToken> {
    let token: PairingToken = random_hex(TOKEN_BYTES)
        .parse()
        .map_err(|_| io::Error::other(BAD_TOKEN))?;
    ensure_private_dir(root)?;
    ensure_private_dir(&root.join(CLIENTS_DIR))?;
    let path = token_file(root, install);
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(FileMode::PRIVATE_FILE.0)
        .open(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(FileMode::PRIVATE_FILE.0))?;
    file.write_all(token.expose().as_bytes())?;
    file.flush()?;
    Ok(token)
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        removed => removed,
    }
}

pub(super) fn unpair(root: &Path, install: &InstallId) -> io::Result<()> {
    remove_if_present(&token_file(root, install))?;
    remove_if_present(&webhooks_file(root, install))
}

pub fn paired_installs(root: &Path) -> Vec<InstallId> {
    let Ok(entries) = fs::read_dir(root.join(CLIENTS_DIR)) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == TOKEN_EXTENSION))
        .filter_map(|path| path.file_stem()?.to_str()?.parse().ok())
        .collect()
}

pub(super) fn is_paired(root: &Path, pairing: &Pairing) -> bool {
    fs::read_to_string(token_file(root, &pairing.install)).is_ok_and(|stored| {
        same_secret(stored.trim().as_bytes(), pairing.token.expose().as_bytes())
    })
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

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn mode(path: &Path) -> io::Result<u32> {
        Ok(fs::metadata(path)?.permissions().mode() & 0o777)
    }

    #[test]
    fn pairing_writes_a_private_token_file_and_unpairing_revokes_it() -> TestResult {
        let root = std::env::temp_dir().join(format!("krad-clients-{}", std::process::id()));
        let install: InstallId = "0123456789abcdef".parse()?;
        let token = pair(&root, &install)?;
        let file = root.join("clients/0123456789abcdef.token");
        assert_eq!(fs::read_to_string(&file)?, token.expose());
        assert_eq!(mode(&file)?, 0o600);
        assert_eq!(mode(&root.join(CLIENTS_DIR))?, 0o700);
        assert_eq!(mode(&root)?, 0o700);
        let paired = Pairing {
            install: install.clone(),
            token,
        };
        assert!(is_paired(&root, &paired));
        let renewed = pair(&root, &install)?;
        assert!(!is_paired(&root, &paired));
        assert!(is_paired(
            &root,
            &Pairing {
                install: install.clone(),
                token: renewed,
            }
        ));
        assert_eq!(paired_installs(&root), vec![install.clone()]);
        let webhooks = webhooks_file(&root, &install);
        ensure_private_dir(webhooks.parent().ok_or("no parent")?)?;
        fs::write(&webhooks, "{}")?;
        unpair(&root, &install)?;
        assert!(!file.exists());
        assert!(!webhooks.exists());
        assert_eq!(paired_installs(&root), Vec::<InstallId>::new());
        unpair(&root, &install)?;
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn secrets_match_only_byte_for_byte() {
        assert!(same_secret(b"abcd", b"abcd"));
        assert!(!same_secret(b"abcd", b"abce"));
        assert!(!same_secret(b"abcd", b"abc"));
    }
}
