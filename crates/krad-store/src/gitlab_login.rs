use krad_domain::GitlabTokens;

use crate::vault::{delete_login, find_login, write_login, LoginPlace, LoginStoreError, Vault};

pub(crate) const GITLAB_VAULT: Vault = Vault {
    keyring_service: "krad-ade-gitlab",
    fallback_dir: "gitlab-logins",
};

/// # Errors
///
/// Fails when the keyring or the fallback file cannot be read, or holds
/// something else than a login.
pub fn read_gitlab_login(host: &str) -> Result<Option<GitlabTokens>, LoginStoreError> {
    Ok(find_gitlab_login(host)?.map(|(tokens, _)| tokens))
}

/// # Errors
///
/// As [`read_gitlab_login`].
pub fn find_gitlab_login(
    host: &str,
) -> Result<Option<(GitlabTokens, LoginPlace)>, LoginStoreError> {
    find_login(&GITLAB_VAULT, host)
}

/// # Errors
///
/// Fails when neither the keyring nor the config folder can be written.
pub fn write_gitlab_login(host: &str, tokens: &GitlabTokens) -> Result<(), LoginStoreError> {
    write_login(&GITLAB_VAULT, host, tokens)
}

/// # Errors
///
/// Fails when either holds a login that cannot be removed.
pub fn delete_gitlab_login(host: &str) -> Result<(), LoginStoreError> {
    delete_login(&GITLAB_VAULT, host)
}
