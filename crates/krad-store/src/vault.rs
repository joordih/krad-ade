use crate::config_dir::config_dir;
use keyring::{Entry, Error as KeyringError};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

const FALLBACK_EXTENSION: &str = "json";
const TMP_SUFFIX: &str = ".tmp";
const FILE_NAME_REPLACEMENT: char = '_';
#[cfg(unix)]
const OWNER_ONLY: u32 = 0o600;

pub(crate) struct Vault {
    pub(crate) keyring_service: &'static str,
    pub(crate) fallback_dir: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum LoginStoreError {
    #[error("The system keyring failed: {0}")]
    Keyring(KeyringError),
    #[error("The saved login is unreadable; sign in again")]
    Unreadable,
    #[error("There is no system keyring and no config folder to keep the login in")]
    NoConfigDir,
    #[error("Could not keep the login in the config folder: {0}")]
    Io(#[from] std::io::Error),
}

impl From<KeyringError> for LoginStoreError {
    fn from(error: KeyringError) -> Self {
        match error {
            KeyringError::BadEncoding(_) | KeyringError::BadDataFormat(..) => Self::Unreadable,
            error => Self::Keyring(error),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoginPlace {
    Keyring,
    ConfigFolder,
}

pub(crate) fn find_login<T: DeserializeOwned>(
    vault: &Vault,
    host: &str,
) -> Result<Option<(T, LoginPlace)>, LoginStoreError> {
    let saved = in_keyring(vault.keyring_service, host, saved_password)?.flatten();
    let (text, place) = match saved {
        Some(text) => (text, LoginPlace::Keyring),
        None => match std::fs::read_to_string(fallback_path(vault, host)?) {
            Ok(text) => (text, LoginPlace::ConfigFolder),
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        },
    };
    serde_json::from_str(&text)
        .map(|login| Some((login, place)))
        .map_err(|_| LoginStoreError::Unreadable)
}

pub(crate) fn write_login<T: Serialize>(
    vault: &Vault,
    host: &str,
    login: &T,
) -> Result<(), LoginStoreError> {
    let text = serde_json::to_string(login).map_err(|_| LoginStoreError::Unreadable)?;
    if in_keyring(vault.keyring_service, host, |entry| {
        entry.set_password(&text)
    })?
    .is_some()
    {
        return remove_fallback(vault, host);
    }
    write_owner_only(&fallback_path(vault, host)?, text.as_bytes())?;
    Ok(())
}

pub(crate) fn delete_login(vault: &Vault, host: &str) -> Result<(), LoginStoreError> {
    in_keyring(vault.keyring_service, host, |entry| {
        match entry.delete_credential() {
            Err(KeyringError::NoEntry) => Ok(()),
            result => result,
        }
    })?;
    remove_fallback(vault, host)
}

pub(crate) fn adopt_keyring_login(
    vault: &Vault,
    legacy_service: &str,
    host: &str,
) -> Result<(), LoginStoreError> {
    let Some(text) = in_keyring(legacy_service, host, saved_password)?.flatten() else {
        return Ok(());
    };
    if in_keyring(vault.keyring_service, host, saved_password)?
        .flatten()
        .is_none()
    {
        in_keyring(vault.keyring_service, host, |entry| {
            entry.set_password(&text)
        })?;
    }
    in_keyring(legacy_service, host, Entry::delete_credential)?;
    Ok(())
}

fn saved_password(entry: &Entry) -> Result<Option<String>, KeyringError> {
    match entry.get_password() {
        Ok(text) => Ok(Some(text)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(error),
    }
}

fn in_keyring<T>(
    service: &str,
    host: &str,
    operation: impl FnOnce(&Entry) -> Result<T, KeyringError>,
) -> Result<Option<T>, LoginStoreError> {
    match Entry::new(service, host).and_then(|entry| operation(&entry)) {
        Ok(value) => Ok(Some(value)),
        Err(
            KeyringError::NoDefaultStore
            | KeyringError::NoStorageAccess(_)
            | KeyringError::PlatformFailure(_),
        ) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn fallback_path(vault: &Vault, host: &str) -> Result<PathBuf, LoginStoreError> {
    let dir = config_dir().ok_or(LoginStoreError::NoConfigDir)?;
    Ok(fallback_path_in(&dir, vault, host))
}

fn fallback_path_in(dir: &Path, vault: &Vault, host: &str) -> PathBuf {
    let name: String = host
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() || matches!(char, '-' | '.') {
                char
            } else {
                FILE_NAME_REPLACEMENT
            }
        })
        .collect();
    dir.join(vault.fallback_dir)
        .join(format!("{name}.{FALLBACK_EXTENSION}"))
}

fn remove_fallback(vault: &Vault, host: &str) -> Result<(), LoginStoreError> {
    let Some(dir) = config_dir() else {
        return Ok(());
    };
    match std::fs::remove_file(fallback_path_in(&dir, vault, host)) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

fn write_owner_only(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(TMP_SUFFIX);
    let tmp = PathBuf::from(tmp);
    match std::fs::remove_file(&tmp) {
        Err(error) if error.kind() != ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, OWNER_ONLY);
    options.open(&tmp)?.write_all(bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VAULT: Vault = Vault {
        keyring_service: "krad-ade-test",
        fallback_dir: "gitlab-logins",
    };

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("krad-store-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).unwrap_or_default();
        dir
    }

    #[test]
    fn the_fallback_file_name_is_safe_for_any_host() {
        let path = fallback_path_in(
            Path::new("/config"),
            &VAULT,
            "https://gitlab.example.test:8443",
        );
        assert_eq!(
            path,
            Path::new("/config/gitlab-logins/https___gitlab.example.test_8443.json")
        );
    }

    #[test]
    fn a_rewrite_replaces_the_login_and_only_the_owner_reads_it() -> std::io::Result<()> {
        let dir = temp_dir("gitlab-login");
        let path = fallback_path_in(&dir, &VAULT, "gitlab.example.test");
        write_owner_only(&path, b"old")?;
        write_owner_only(&path, b"new")?;
        assert_eq!(std::fs::read(&path)?, b"new");
        assert_eq!(std::fs::read_dir(path.parent().unwrap_or(&dir))?.count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)?.permissions().mode();
            assert_eq!(mode & 0o777, OWNER_ONLY);
        }
        std::fs::remove_dir_all(dir)
    }

    #[test]
    fn keyring_errors_never_carry_the_secret() {
        let error = LoginStoreError::from(KeyringError::BadEncoding(b"secret".to_vec()));
        assert!(matches!(error, LoginStoreError::Unreadable));
        assert!(!format!("{error:?}").contains("secret"));
    }
}
