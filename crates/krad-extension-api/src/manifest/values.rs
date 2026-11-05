use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Component, Path};
use thiserror::Error;

const SHA256_HEX_LEN: usize = 64;
const HTTPS_SCHEME: &str = "https://";
const PARENT: &str = "..";
const SEPARATORS: [char; 2] = ['/', '\\'];
const DRIVE_MARK: char = ':';

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValueError {
    #[error("a program path is a relative path inside the extension, without '..'")]
    ProgramPath,
    #[error("a sha256 is {SHA256_HEX_LEN} hexadecimal digits")]
    Sha256,
    #[error("a download address starts with https:// and names a host")]
    Url,
}

macro_rules! checked_text {
    ($name:ident, $check:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl TryFrom<String> for $name {
            type Error = ValueError;

            fn try_from(text: String) -> Result<Self, Self::Error> {
                $check(text).map(Self)
            }
        }

        impl std::str::FromStr for $name {
            type Err = ValueError;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                Self::try_from(text.to_owned())
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

fn program_path(text: String) -> Result<String, ValueError> {
    let path = Path::new(&text);
    let absolute = text.starts_with(SEPARATORS) || text.contains(DRIVE_MARK) || path.is_absolute();
    let escapes = text.split(SEPARATORS).any(|part| part == PARENT);
    let plain = path
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
    if text.is_empty() || absolute || escapes || !plain {
        return Err(ValueError::ProgramPath);
    }
    Ok(text)
}

fn sha256(text: String) -> Result<String, ValueError> {
    let hex = text.trim().to_ascii_lowercase();
    if hex.len() == SHA256_HEX_LEN && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(hex)
    } else {
        Err(ValueError::Sha256)
    }
}

fn https_url(text: String) -> Result<String, ValueError> {
    let host = text.strip_prefix(HTTPS_SCHEME).ok_or(ValueError::Url)?;
    let named = host
        .split(SEPARATORS)
        .next()
        .is_some_and(|name| !name.is_empty());
    if named && !text.chars().any(|c| c.is_whitespace() || c.is_control()) {
        Ok(text)
    } else {
        Err(ValueError::Url)
    }
}

checked_text!(ProgramPath, program_path);
checked_text!(Sha256Hex, sha256);
checked_text!(HttpsUrl, https_url);

impl ProgramPath {
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_paths_stay_inside_the_extension() {
        for good in ["bin/tool", "./dist/main", "a/b/c.exe"] {
            assert!(good.parse::<ProgramPath>().is_ok(), "{good}");
        }
        for bad in [
            "",
            "/usr/bin/env",
            "../x",
            "bin/../../x",
            "C:\\\\Windows\\\\x.exe",
            "\\\\server\\\\x",
            "bin\\\\..\\\\x",
        ] {
            assert_eq!(
                bad.parse::<ProgramPath>(),
                Err(ValueError::ProgramPath),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_sha256_is_exactly_sixty_four_hex_digits() {
        let digest = "A".repeat(SHA256_HEX_LEN);
        assert_eq!(
            digest.parse::<Sha256Hex>().map(String::from),
            Ok("a".repeat(SHA256_HEX_LEN))
        );
        for bad in [
            "",
            "abc",
            "../../../../etc",
            &"g".repeat(SHA256_HEX_LEN),
            &"a".repeat(SHA256_HEX_LEN + 1),
        ] {
            assert_eq!(bad.parse::<Sha256Hex>(), Err(ValueError::Sha256), "{bad}");
        }
    }

    #[test]
    fn downloads_come_only_from_https_hosts() {
        assert!("https://example.com/tool".parse::<HttpsUrl>().is_ok());
        for bad in [
            "http://example.com/tool",
            "file:///etc/passwd",
            "https:///x",
            "https://a b",
            "-https://x",
        ] {
            assert_eq!(bad.parse::<HttpsUrl>(), Err(ValueError::Url), "{bad}");
        }
    }
}
