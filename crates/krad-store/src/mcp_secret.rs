use krad_domain::{McpSecret, McpSecretKey};

use crate::vault::{delete_login, find_login, write_login, LoginPlace, LoginStoreError, Vault};

const MCP_VAULT: Vault = Vault {
    keyring_service: "krad-ade-mcp",
    fallback_dir: "mcp-secrets",
};

/// # Errors
///
/// Fails when the keyring or the fallback file cannot be read, or holds
/// something else than a secret.
pub fn find_mcp_secret(
    key: &McpSecretKey,
) -> Result<Option<(McpSecret, LoginPlace)>, LoginStoreError> {
    find_login(&MCP_VAULT, &key.to_string())
}

/// # Errors
///
/// Fails when neither the keyring nor the config folder can be written.
pub fn write_mcp_secret(key: &McpSecretKey, secret: &McpSecret) -> Result<(), LoginStoreError> {
    write_login(&MCP_VAULT, &key.to_string(), secret)
}

/// # Errors
///
/// Fails when either holds a secret that cannot be removed.
pub fn delete_mcp_secret(key: &McpSecretKey) -> Result<(), LoginStoreError> {
    delete_login(&MCP_VAULT, &key.to_string())
}
