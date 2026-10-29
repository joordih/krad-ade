use krad_domain::GithubToken;

use crate::vault::{delete_login, find_login, write_login, LoginPlace, LoginStoreError, Vault};

const GITHUB_VAULT: Vault = Vault {
    keyring_service: "krad-ade-github",
    fallback_dir: "github-logins",
};
const GITHUB_HOST: &str = "github.com";

/// # Errors
///
/// Fails when the keyring or the fallback file cannot be read, or holds
/// something else than a login.
pub fn read_github_login() -> Result<Option<GithubToken>, LoginStoreError> {
    Ok(find_github_login()?.map(|(token, _)| token))
}

/// # Errors
///
/// As [`read_github_login`].
pub fn find_github_login() -> Result<Option<(GithubToken, LoginPlace)>, LoginStoreError> {
    find_login(&GITHUB_VAULT, GITHUB_HOST)
}

/// # Errors
///
/// Fails when neither the keyring nor the config folder can be written.
pub fn write_github_login(token: &GithubToken) -> Result<(), LoginStoreError> {
    write_login(&GITHUB_VAULT, GITHUB_HOST, token)
}

/// # Errors
///
/// Fails when either holds a login that cannot be removed.
pub fn delete_github_login() -> Result<(), LoginStoreError> {
    delete_login(&GITHUB_VAULT, GITHUB_HOST)
}
