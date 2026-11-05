use krad_domain::{AcpSecretKey, McpSecret};

use crate::vault::{delete_login, find_login, write_login, LoginPlace, LoginStoreError, Vault};

const ACP_VAULT: Vault = Vault {
    keyring_service: "krad-ade-acp",
    fallback_dir: "acp-secrets",
};

/// # Errors
///
/// Fails when the keyring or the fallback file cannot be read, or holds
/// something else than a secret.
pub fn find_acp_secret(
    key: &AcpSecretKey,
) -> Result<Option<(McpSecret, LoginPlace)>, LoginStoreError> {
    find_login(&ACP_VAULT, &key.to_string())
}

/// # Errors
///
/// Fails when neither the keyring nor the config folder can be written.
pub fn write_acp_secret(key: &AcpSecretKey, secret: &McpSecret) -> Result<(), LoginStoreError> {
    write_login(&ACP_VAULT, &key.to_string(), secret)
}

/// # Errors
///
/// Fails when either holds a secret that cannot be removed.
pub fn delete_acp_secret(key: &AcpSecretKey) -> Result<(), LoginStoreError> {
    delete_login(&ACP_VAULT, &key.to_string())
}
