use krad_domain::{GatewayKey, GatewayUrl};

use crate::vault::{delete_login, find_login, write_login, LoginPlace, LoginStoreError, Vault};

const GATEWAY_VAULT: Vault = Vault {
    keyring_service: "krad-ade-gateway",
    fallback_dir: "gateway-keys",
};

/// # Errors
///
/// Fails when the keyring or the fallback file cannot be read, or holds
/// something else than a key.
pub fn find_gateway_key(
    url: &GatewayUrl,
) -> Result<Option<(GatewayKey, LoginPlace)>, LoginStoreError> {
    find_login(&GATEWAY_VAULT, url.as_str())
}

/// # Errors
///
/// Fails when neither the keyring nor the config folder can be written.
pub fn write_gateway_key(url: &GatewayUrl, key: &GatewayKey) -> Result<(), LoginStoreError> {
    write_login(&GATEWAY_VAULT, url.as_str(), key)
}

/// # Errors
///
/// Fails when either holds a key that cannot be removed.
pub fn delete_gateway_key(url: &GatewayUrl) -> Result<(), LoginStoreError> {
    delete_login(&GATEWAY_VAULT, url.as_str())
}
