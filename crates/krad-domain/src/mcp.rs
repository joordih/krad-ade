mod expand;
mod names;
mod project_file;
mod settings;

pub use expand::expand_variables;
pub use names::{McpNameError, McpServerName, McpUrl, McpUrlError, McpVariableName};
pub use project_file::{McpFileError, McpProjectFile, McpSkipReason, McpSkipped, PROJECT_MCP_FILE};
pub use settings::{McpProjectTrust, McpServerTaken, McpSettings};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

const REDACTED: &str = "[REDACTED]";
const SECRET_KEY_SEPARATOR: char = '/';

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpValue {
    Plain(String),
    Secret,
}

impl fmt::Debug for McpValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plain(_) => formatter.debug_tuple("Plain").field(&REDACTED).finish(),
            Self::Secret => formatter.write_str("Secret"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct McpVariable {
    pub name: McpVariableName,
    pub value: McpValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransport {
    Stdio {
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        env: Vec<McpVariable>,
    },
    Http {
        url: McpUrl,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        headers: Vec<McpVariable>,
    },
}

impl McpTransport {
    #[must_use]
    pub fn variables(&self) -> &[McpVariable] {
        match self {
            Self::Stdio { env, .. } => env,
            Self::Http { headers, .. } => headers,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpServerState {
    #[default]
    Enabled,
    Disabled,
}

impl McpServerState {
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Enabled => Self::Disabled,
            Self::Disabled => Self::Enabled,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct McpServer {
    pub name: McpServerName,
    pub transport: McpTransport,
    #[serde(default)]
    pub state: McpServerState,
}

impl McpServer {
    #[must_use]
    pub fn secret_keys(&self) -> Vec<McpSecretKey> {
        self.transport
            .variables()
            .iter()
            .filter(|variable| variable.value == McpValue::Secret)
            .map(|variable| McpSecretKey {
                server: self.name.clone(),
                variable: variable.name.clone(),
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpServerStart {
    pub server: McpServer,
    pub secrets: HashMap<McpVariableName, McpSecret>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct McpSecretKey {
    pub server: McpServerName,
    pub variable: McpVariableName,
}

impl fmt::Display for McpSecretKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}{SECRET_KEY_SEPARATOR}{}",
            self.server, self.variable
        )
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct McpSecret(String);

impl McpSecret {
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        (!text.is_empty()).then(|| Self(text.to_owned()))
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for McpSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> McpServerName {
        text.parse()
            .unwrap_or_else(|_| unreachable!("{text} is a server name"))
    }

    fn variable(text: &str) -> McpVariableName {
        text.parse()
            .unwrap_or_else(|_| unreachable!("{text} is a variable name"))
    }

    #[test]
    fn only_secret_values_have_a_keyring_entry() {
        let server = McpServer {
            name: name("github"),
            transport: McpTransport::Stdio {
                command: "npx".to_owned(),
                args: Vec::new(),
                env: vec![
                    McpVariable {
                        name: variable("GITHUB_TOKEN"),
                        value: McpValue::Secret,
                    },
                    McpVariable {
                        name: variable("LOG_LEVEL"),
                        value: McpValue::Plain("debug".to_owned()),
                    },
                ],
            },
            state: McpServerState::Enabled,
        };
        let keys = server.secret_keys();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].to_string(), "github/GITHUB_TOKEN");
        assert!(!format!("{server:?}").contains("debug"));
    }

    #[test]
    fn a_secret_never_prints() {
        let secret = McpSecret::parse(" ghp_value ").unwrap_or_else(|| unreachable!("not blank"));
        assert_eq!(secret.expose(), "ghp_value");
        assert!(!format!("{secret:?}").contains("ghp"));
        assert_eq!(McpSecret::parse(" "), None);
    }

    #[test]
    fn a_server_roundtrips_and_starts_enabled() -> serde_json::Result<()> {
        let read: McpServer = serde_json::from_str(
            r#"{"name":"docs","transport":{"type":"http","url":"https://mcp.example.test/mcp","headers":[{"name":"Authorization","value":"secret"}]}}"#,
        )?;
        assert_eq!(read.state, McpServerState::Enabled);
        assert_eq!(read.secret_keys().len(), 1);
        let back: McpServer = serde_json::from_str(&serde_json::to_string(&read)?)?;
        assert_eq!(back, read);
        Ok(())
    }
}
