use krad_domain::{ExtensionSecret, ExtensionSecretKey};

use crate::vault::{delete_login, find_login, write_login, LoginPlace, LoginStoreError, Vault};

const EXTENSION_VAULT: Vault = Vault {
    keyring_service: "krad-ade-extensions",
    fallback_dir: "extension-secrets",
};

/// # Errors
///
/// Fails when the keyring or the fallback file cannot be read, or holds
/// something else than a secret.
pub fn find_extension_secret(
    key: &ExtensionSecretKey,
) -> Result<Option<(ExtensionSecret, LoginPlace)>, LoginStoreError> {
    find_login(&EXTENSION_VAULT, &key.to_string())
}

/// # Errors
///
/// Fails when neither the keyring nor the config folder can be written.
pub fn write_extension_secret(
    key: &ExtensionSecretKey,
    secret: &ExtensionSecret,
) -> Result<(), LoginStoreError> {
    write_login(&EXTENSION_VAULT, &key.to_string(), secret)
}

/// # Errors
///
/// Fails when either holds a secret that cannot be removed.
pub fn delete_extension_secret(key: &ExtensionSecretKey) -> Result<(), LoginStoreError> {
    delete_login(&EXTENSION_VAULT, &key.to_string())
}
