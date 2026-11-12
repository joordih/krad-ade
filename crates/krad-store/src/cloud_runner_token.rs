use krad_domain::{ClaudeOauthToken, CloudRunnerId, OpencodePassword, PairingToken};

use crate::vault::{delete_login, find_login, write_login, LoginPlace, LoginStoreError, Vault};

const CLOUD_RUNNER_VAULT: Vault = Vault {
    keyring_service: "krad-ade-cloud-runner",
    fallback_dir: "cloud-runner-tokens",
};

const OPENCODE_PASSWORD_VAULT: Vault = Vault {
    keyring_service: "krad-ade-cloud-opencode",
    fallback_dir: "cloud-runner-opencode",
};

const CLAUDE_TOKEN_VAULT: Vault = Vault {
    keyring_service: "krad-ade-cloud-claude",
    fallback_dir: "cloud-runner-claude",
};

fn vault_key(id: CloudRunnerId) -> String {
    id.to_string()
}

/// # Errors
///
/// Fails when the keyring or the fallback file cannot be read, or holds
/// something else than a pairing token.
pub fn find_cloud_runner_token(
    id: CloudRunnerId,
) -> Result<Option<(PairingToken, LoginPlace)>, LoginStoreError> {
    find_login(&CLOUD_RUNNER_VAULT, &vault_key(id))
}

/// # Errors
///
/// Fails when neither the keyring nor the config folder can be written.
pub fn write_cloud_runner_token(
    id: CloudRunnerId,
    token: &PairingToken,
) -> Result<(), LoginStoreError> {
    write_login(&CLOUD_RUNNER_VAULT, &vault_key(id), token)
}

/// # Errors
///
/// Fails when either holds a token that cannot be removed.
pub fn delete_cloud_runner_token(id: CloudRunnerId) -> Result<(), LoginStoreError> {
    delete_login(&CLOUD_RUNNER_VAULT, &vault_key(id))
}

/// # Errors
///
/// Fails when the keyring or the fallback file cannot be read.
pub fn find_cloud_opencode_password(
    id: CloudRunnerId,
) -> Result<Option<(OpencodePassword, LoginPlace)>, LoginStoreError> {
    find_login(&OPENCODE_PASSWORD_VAULT, &vault_key(id))
}

/// # Errors
///
/// Fails when neither the keyring nor the config folder can be written.
pub fn write_cloud_opencode_password(
    id: CloudRunnerId,
    password: &OpencodePassword,
) -> Result<(), LoginStoreError> {
    write_login(&OPENCODE_PASSWORD_VAULT, &vault_key(id), password)
}

/// # Errors
///
/// Fails when either holds a password that cannot be removed.
pub fn delete_cloud_opencode_password(id: CloudRunnerId) -> Result<(), LoginStoreError> {
    delete_login(&OPENCODE_PASSWORD_VAULT, &vault_key(id))
}

/// # Errors
///
/// Fails when the keyring or the fallback file cannot be read, or holds
/// something else than a Claude Code token.
pub fn find_cloud_claude_token(
    id: CloudRunnerId,
) -> Result<Option<(ClaudeOauthToken, LoginPlace)>, LoginStoreError> {
    find_login(&CLAUDE_TOKEN_VAULT, &vault_key(id))
}

/// # Errors
///
/// Fails when neither the keyring nor the config folder can be written.
pub fn write_cloud_claude_token(
    id: CloudRunnerId,
    token: &ClaudeOauthToken,
) -> Result<(), LoginStoreError> {
    write_login(&CLAUDE_TOKEN_VAULT, &vault_key(id), token)
}

/// # Errors
///
/// Fails when either holds a token that cannot be removed.
pub fn delete_cloud_claude_token(id: CloudRunnerId) -> Result<(), LoginStoreError> {
    delete_login(&CLAUDE_TOKEN_VAULT, &vault_key(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::fallback_path_in;
    use std::path::Path;

    #[test]
    fn each_runner_keeps_its_token_in_its_own_file() {
        assert_eq!(
            fallback_path_in(
                Path::new("/config"),
                &CLOUD_RUNNER_VAULT,
                &vault_key(CloudRunnerId(7))
            ),
            Path::new("/config/cloud-runner-tokens/7.json")
        );
        assert_eq!(
            fallback_path_in(
                Path::new("/config"),
                &OPENCODE_PASSWORD_VAULT,
                &vault_key(CloudRunnerId(7))
            ),
            Path::new("/config/cloud-runner-opencode/7.json")
        );
        assert_eq!(
            fallback_path_in(
                Path::new("/config"),
                &CLAUDE_TOKEN_VAULT,
                &vault_key(CloudRunnerId(7))
            ),
            Path::new("/config/cloud-runner-claude/7.json")
        );
    }

    #[test]
    fn the_kept_text_reads_back_as_the_same_token() -> Result<(), Box<dyn std::error::Error>> {
        let token: PairingToken = "00112233445566778899aabbccddeeff".parse()?;
        let kept = serde_json::to_string(&token)?;
        assert_eq!(serde_json::from_str::<PairingToken>(&kept)?, token);
        Ok(())
    }
}
