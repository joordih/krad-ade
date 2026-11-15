use krad_domain::{Pairing, PairingPermissions};
use serde::{Deserialize, Serialize};

use crate::command::{ErrorCode, Reply};
use crate::ids::{ProtocolVersion, RequestId};

const OTHER_VERSION: &str = "this runtime speaks another major version of the protocol";

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(transparent)]
pub struct RuntimeRun(pub u64);

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Hello {
    pub protocol: ProtocolVersion,
    pub build: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pairing: Option<Pairing>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Welcome {
    pub protocol: ProtocolVersion,
    pub build: String,
    pub run: RuntimeRun,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PairingPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_request: Option<RequestId>,
}

impl Hello {
    #[must_use]
    pub fn new(build: &str) -> Self {
        Self {
            protocol: ProtocolVersion::CURRENT,
            build: build.to_owned(),
            pairing: None,
        }
    }

    #[must_use]
    pub fn paired(build: &str, pairing: Pairing) -> Self {
        Self {
            pairing: Some(pairing),
            ..Self::new(build)
        }
    }

    /// # Errors
    ///
    /// Refuses with `unsupported` when the client speaks another major version.
    pub fn welcome(&self, build: &str, run: RuntimeRun) -> Result<Welcome, Reply> {
        if self.protocol != ProtocolVersion::CURRENT {
            return Err(Reply::Refused {
                code: ErrorCode::Unsupported,
                message: OTHER_VERSION.to_owned(),
            });
        }
        Ok(Welcome {
            protocol: ProtocolVersion::CURRENT,
            build: build.to_owned(),
            run,
            permissions: None,
            first_request: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILD: &str = "0.3.3";

    #[test]
    fn a_client_of_the_same_major_version_is_welcomed_with_the_runtime_build_and_run() {
        let welcome = Hello::new("0.3.2").welcome(BUILD, RuntimeRun(7));
        assert_eq!(
            welcome,
            Ok(Welcome {
                protocol: ProtocolVersion::CURRENT,
                build: BUILD.to_owned(),
                run: RuntimeRun(7),
                permissions: None,
                first_request: None,
            })
        );
    }

    #[test]
    fn a_client_of_another_major_version_is_refused_as_unsupported() {
        let hello = Hello {
            protocol: ProtocolVersion(ProtocolVersion::CURRENT.0 + 1),
            build: BUILD.to_owned(),
            pairing: None,
        };
        assert!(matches!(
            hello.welcome(BUILD, RuntimeRun(1)),
            Err(Reply::Refused {
                code: ErrorCode::Unsupported,
                ..
            })
        ));
    }

    #[test]
    fn a_hello_from_a_newer_client_with_more_fields_still_reads() -> serde_json::Result<()> {
        let hello: Hello =
            serde_json::from_str(r#"{"protocol":1,"build":"0.9.0","features":["pairing"]}"#)?;
        assert_eq!(hello.protocol, ProtocolVersion::CURRENT);
        Ok(())
    }
}
