use krad_domain::{WebhookId, WebhookSecret};

use crate::vault::{delete_login, find_login, write_login, LoginPlace, LoginStoreError, Vault};

const WEBHOOK_VAULT: Vault = Vault {
    keyring_service: "krad-ade-webhook",
    fallback_dir: "webhook-secrets",
};

/// # Errors
///
/// Fails when the keyring or the fallback file cannot be read, or holds
/// something else than a secret.
pub fn find_webhook_secret(
    id: WebhookId,
) -> Result<Option<(WebhookSecret, LoginPlace)>, LoginStoreError> {
    find_login(&WEBHOOK_VAULT, &id.to_string())
}

/// # Errors
///
/// Fails when neither the keyring nor the config folder can be written.
pub fn write_webhook_secret(id: WebhookId, secret: &WebhookSecret) -> Result<(), LoginStoreError> {
    write_login(&WEBHOOK_VAULT, &id.to_string(), secret)
}

/// # Errors
///
/// Fails when either holds a secret that cannot be removed.
pub fn delete_webhook_secret(id: WebhookId) -> Result<(), LoginStoreError> {
    delete_login(&WEBHOOK_VAULT, &id.to_string())
}
