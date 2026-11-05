use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use krad_extension_id::{check_id, check_key};
pub use krad_extension_id::{ExtensionId, IdError};

macro_rules! string_id {
    ($name:ident, $check:ident) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl TryFrom<String> for $name {
            type Error = IdError;

            fn try_from(text: String) -> Result<Self, Self::Error> {
                $check(&text)?;
                Ok(Self(text))
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                Self::try_from(text.to_owned())
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.0
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

string_id!(CommandId, check_id);
string_id!(ViewId, check_id);
string_id!(SettingKey, check_id);
string_id!(SecretName, check_id);
string_id!(RuntimeId, check_id);
string_id!(ToolName, check_id);
string_id!(HostKindId, check_id);
string_id!(MachineId, check_key);
string_id!(NodeKey, check_key);
string_id!(ActionId, check_key);
string_id!(ViewInstance, check_key);
string_id!(ProgressToken, check_key);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct QualifiedCommand {
    #[schemars(with = "String")]
    pub extension: ExtensionId,
    pub command: CommandId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct TerminalHandle(pub u64);

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extension_id::MAX_ID_LEN;

    #[test]
    fn ids_accept_lowercase_words_and_refuse_the_rest() {
        assert!("docker".parse::<ExtensionId>().is_ok());
        assert!("containers.refresh-all_2".parse::<CommandId>().is_ok());
        assert_eq!("".parse::<ExtensionId>(), Err(IdError::Empty));
        assert_eq!("Docker".parse::<ExtensionId>(), Err(IdError::Character));
        assert_eq!("a b".parse::<ViewId>(), Err(IdError::Character));
        assert_eq!(
            "x".repeat(MAX_ID_LEN + 1).parse::<ViewId>(),
            Err(IdError::TooLong)
        );
    }

    #[test]
    fn node_keys_take_any_text_but_empty() {
        assert!("sha256:4f2a Web 1".parse::<NodeKey>().is_ok());
        assert_eq!("".parse::<NodeKey>(), Err(IdError::Empty));
    }

    #[test]
    fn an_invalid_id_is_refused_while_reading() {
        let read: Result<ExtensionId, _> = serde_json::from_str(r#""Bad Id""#);
        assert!(read.is_err());
        let read: Result<ExtensionId, _> = serde_json::from_str(r#""docker""#);
        assert_eq!(read.ok(), "docker".parse().ok());
    }
}
