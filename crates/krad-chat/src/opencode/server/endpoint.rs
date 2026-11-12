use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use krad_domain::OpencodePassword;
use serde::Deserialize;
use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use ureq::RequestBuilder;

const USER: &str = "opencode";
const BASIC: &str = "Basic ";
const AUTHORIZATION: &str = "authorization";
const PASSWORD_WORDS: usize = 4;
const REDACTED: &str = "[REDACTED]";

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub(in crate::opencode) struct Secret(pub(in crate::opencode) String);

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

pub(in crate::opencode) fn new_password() -> Secret {
    let bytes: Vec<u8> = (0..PASSWORD_WORDS)
        .flat_map(|_| RandomState::new().build_hasher().finish().to_le_bytes())
        .collect();
    Secret(URL_SAFE_NO_PAD.encode(bytes))
}

#[must_use]
pub fn new_opencode_password() -> OpencodePassword {
    OpencodePassword::from(new_password().0)
}

impl From<&OpencodePassword> for Secret {
    fn from(password: &OpencodePassword) -> Self {
        Self(password.expose().to_owned())
    }
}

#[derive(Clone)]
pub(in crate::opencode) struct Endpoint {
    pub(in crate::opencode) url: String,
    pub(in crate::opencode) password: Option<Secret>,
}

impl Endpoint {
    pub(in crate::opencode) const fn new(url: String, password: Option<Secret>) -> Self {
        Self { url, password }
    }

    pub(in crate::opencode) fn sign<B>(&self, request: RequestBuilder<B>) -> RequestBuilder<B> {
        let Some(password) = &self.password else {
            return request;
        };
        let credentials = STANDARD.encode(format!("{USER}:{}", password.0));
        request.header(AUTHORIZATION, format!("{BASIC}{credentials}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_password_is_long_and_never_printed() {
        let password = new_opencode_password();
        assert_eq!(format!("{password:?}"), REDACTED);
        assert!(password.expose().len() > PASSWORD_WORDS * 8);
        assert_eq!(Secret::from(&password).0, password.expose());
    }
}
