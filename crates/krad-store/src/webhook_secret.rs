use krad_domain::{WebhookId, WebhookSecret};

use crate::vault::{
    delete_login, find_login, write_login, LoginPlace, LoginStoreError, Secrets, Vault,
};

const WEBHOOK_VAULT: Vault = Vault {
    keyring_service: "krad-ade-webhook",
    fallback_dir: "webhook-secrets",
};

impl Secrets {
    /// # Errors
    ///
    /// Fails when the keyring or the fallback file cannot be read, or holds
    /// something else than a secret.
    pub fn find_webhook_secret(
        &self,
        id: WebhookId,
    ) -> Result<Option<(WebhookSecret, LoginPlace)>, LoginStoreError> {
        find_login(self, &WEBHOOK_VAULT, &id.to_string())
    }

    /// # Errors
    ///
    /// Fails when neither the keyring nor the config folder can be written.
    pub fn write_webhook_secret(
        &self,
        id: WebhookId,
        secret: &WebhookSecret,
    ) -> Result<(), LoginStoreError> {
        write_login(self, &WEBHOOK_VAULT, &id.to_string(), secret)
    }

    /// # Errors
    ///
    /// Fails when either holds a secret that cannot be removed.
    pub fn delete_webhook_secret(&self, id: WebhookId) -> Result<(), LoginStoreError> {
        delete_login(self, &WEBHOOK_VAULT, &id.to_string())
    }
}

/// # Errors
///
/// Fails when the keyring or the fallback file cannot be read, or holds
/// something else than a secret.
pub fn find_webhook_secret(
    id: WebhookId,
) -> Result<Option<(WebhookSecret, LoginPlace)>, LoginStoreError> {
    Secrets::system().find_webhook_secret(id)
}

/// # Errors
///
/// Fails when neither the keyring nor the config folder can be written.
pub fn write_webhook_secret(id: WebhookId, secret: &WebhookSecret) -> Result<(), LoginStoreError> {
    Secrets::system().write_webhook_secret(id, secret)
}

/// # Errors
///
/// Fails when either holds a secret that cannot be removed.
pub fn delete_webhook_secret(id: WebhookId) -> Result<(), LoginStoreError> {
    Secrets::system().delete_webhook_secret(id)
}
