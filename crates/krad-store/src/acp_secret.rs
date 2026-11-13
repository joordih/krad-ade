use krad_domain::{AcpAgent, AcpSecretKey, McpSecret, McpVariableName};
use std::collections::HashMap;

use crate::vault::{
    delete_login, find_login, write_login, LoginPlace, LoginStoreError, Secrets, Vault,
};

const ACP_VAULT: Vault = Vault {
    keyring_service: "krad-ade-acp",
    fallback_dir: "acp-secrets",
};

impl Secrets {
    /// # Errors
    ///
    /// Fails when the keyring or the fallback file cannot be read, or holds
    /// something else than a secret.
    pub fn find_acp_secret(
        &self,
        key: &AcpSecretKey,
    ) -> Result<Option<(McpSecret, LoginPlace)>, LoginStoreError> {
        find_login(self, &ACP_VAULT, &key.to_string())
    }

    /// # Errors
    ///
    /// Fails when neither the keyring nor the config folder can be written.
    pub fn write_acp_secret(
        &self,
        key: &AcpSecretKey,
        secret: &McpSecret,
    ) -> Result<(), LoginStoreError> {
        write_login(self, &ACP_VAULT, &key.to_string(), secret)
    }

    /// # Errors
    ///
    /// Fails when either holds a secret that cannot be removed.
    pub fn delete_acp_secret(&self, key: &AcpSecretKey) -> Result<(), LoginStoreError> {
        delete_login(self, &ACP_VAULT, &key.to_string())
    }

    /// # Errors
    ///
    /// Fails when one of the agent's secrets cannot be read.
    pub fn read_acp_secrets(&self, agent: &AcpAgent) -> Result<AcpSecrets, AcpSecretsUnread> {
        let mut secrets = HashMap::new();
        for key in agent.secret_keys() {
            if let Some((secret, _)) = self.find_acp_secret(&key)? {
                secrets.insert(key.variable, secret);
            }
        }
        Ok(secrets)
    }
}

/// # Errors
///
/// Fails when neither the keyring nor the config folder can be written.
pub fn write_acp_secret(key: &AcpSecretKey, secret: &McpSecret) -> Result<(), LoginStoreError> {
    Secrets::system().write_acp_secret(key, secret)
}

/// # Errors
///
/// Fails when either holds a secret that cannot be removed.
pub fn delete_acp_secret(key: &AcpSecretKey) -> Result<(), LoginStoreError> {
    Secrets::system().delete_acp_secret(key)
}

/// # Errors
///
/// Fails when one of the agent's secrets cannot be read.
pub fn read_acp_secrets(agent: &AcpAgent) -> Result<AcpSecrets, AcpSecretsUnread> {
    Secrets::system().read_acp_secrets(agent)
}

pub type AcpSecrets = HashMap<McpVariableName, McpSecret>;

#[derive(Debug, thiserror::Error)]
#[error("The agent's secrets could not be read: {0}")]
pub struct AcpSecretsUnread(#[from] LoginStoreError);
