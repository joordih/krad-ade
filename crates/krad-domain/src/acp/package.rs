use std::fmt;

const NPM_GLOBAL_INSTALL: &str = "npm i -g";
const LATEST_TAG: &str = "latest";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NpmPackage {
    pub name: &'static str,
    pub bin: &'static str,
}

impl NpmPackage {
    #[must_use]
    pub fn latest(self) -> String {
        format!("{}@{LATEST_TAG}", self.name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AcpPackage {
    Npm(NpmPackage),
    Script(&'static str),
}

impl AcpPackage {
    #[must_use]
    pub const fn npm(self) -> Option<NpmPackage> {
        match self {
            Self::Npm(package) => Some(package),
            Self::Script(_) => None,
        }
    }
}

impl fmt::Display for AcpPackage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Npm(package) => write!(formatter, "{NPM_GLOBAL_INSTALL} {}", package.name),
            Self::Script(script) => formatter.write_str(script),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcpPackageState {
    Unchecked,
    Checking,
    Missing,
    Installing,
    Ready { version: Option<String> },
    Failed(String),
}

impl AcpPackageState {
    #[must_use]
    pub const fn is_busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Installing)
    }

    #[must_use]
    pub const fn can_install(&self) -> bool {
        matches!(self, Self::Missing | Self::Failed(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADAPTER: NpmPackage = NpmPackage {
        name: "@agentclientprotocol/claude-agent-acp",
        bin: "claude-agent-acp",
    };

    #[test]
    fn an_npm_package_installs_globally_and_asks_for_its_latest_release() {
        assert_eq!(
            AcpPackage::Npm(ADAPTER).to_string(),
            "npm i -g @agentclientprotocol/claude-agent-acp"
        );
        assert_eq!(
            ADAPTER.latest(),
            "@agentclientprotocol/claude-agent-acp@latest"
        );
        assert_eq!(AcpPackage::Script("curl x | bash").npm(), None);
    }

    #[test]
    fn only_a_missing_or_failed_package_offers_an_install() {
        assert!(AcpPackageState::Missing.can_install());
        assert!(AcpPackageState::Failed("EACCES".to_owned()).can_install());
        assert!(!AcpPackageState::Installing.can_install());
        assert!(AcpPackageState::Installing.is_busy());
        assert!(!AcpPackageState::Ready { version: None }.can_install());
    }
}
