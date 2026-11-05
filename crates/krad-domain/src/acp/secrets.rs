use super::{AcpAgent, AcpAgentId};
use crate::mcp::{McpSecret, McpValue, McpVariable, McpVariableName};
use std::collections::HashMap;
use std::fmt;

const REDACTED: &str = "[REDACTED]";
const SECRET_KEY_SEPARATOR: char = '/';

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AcpSecretKey {
    pub agent: AcpAgentId,
    pub variable: McpVariableName,
}

impl fmt::Display for AcpSecretKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}{SECRET_KEY_SEPARATOR}{}",
            self.agent, self.variable
        )
    }
}

impl AcpAgent {
    #[must_use]
    pub fn secret_keys(&self) -> Vec<AcpSecretKey> {
        self.env
            .iter()
            .filter(|variable| variable.value == McpValue::Secret)
            .map(|variable| AcpSecretKey {
                agent: self.id,
                variable: variable.name.clone(),
            })
            .collect()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum EnteredEnvValue {
    Plain(String),
    Secret(McpSecret),
}

impl fmt::Debug for EnteredEnvValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plain(_) => formatter.debug_tuple("Plain").field(&REDACTED).finish(),
            Self::Secret(_) => formatter.debug_tuple("Secret").field(&REDACTED).finish(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnteredEnvVariable {
    pub name: McpVariableName,
    pub value: EnteredEnvValue,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SplitEnv {
    pub env: Vec<McpVariable>,
    pub secrets: HashMap<McpVariableName, McpSecret>,
}

impl FromIterator<EnteredEnvVariable> for SplitEnv {
    fn from_iter<I: IntoIterator<Item = EnteredEnvVariable>>(entered: I) -> Self {
        let mut split = Self::default();
        for EnteredEnvVariable { name, value } in entered {
            let value = match value {
                EnteredEnvValue::Plain(text) => McpValue::Plain(text),
                EnteredEnvValue::Secret(secret) => {
                    split.secrets.insert(name.clone(), secret);
                    McpValue::Secret
                }
            };
            split.env.push(McpVariable { name, value });
        }
        split
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpAgentStart {
    pub agent: AcpAgent,
    pub secrets: HashMap<McpVariableName, McpSecret>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variable(text: &str) -> McpVariableName {
        text.parse()
            .unwrap_or_else(|_| unreachable!("{text} is a variable name"))
    }

    fn secret(text: &str) -> McpSecret {
        McpSecret::parse(text).unwrap_or_else(|| unreachable!("{text} is not blank"))
    }

    #[test]
    fn secret_values_leave_the_definition_for_the_keyring() {
        let split: SplitEnv = [
            EnteredEnvVariable {
                name: variable("ANTHROPIC_API_KEY"),
                value: EnteredEnvValue::Secret(secret("sk-ant-value")),
            },
            EnteredEnvVariable {
                name: variable("LOG_LEVEL"),
                value: EnteredEnvValue::Plain("debug".to_owned()),
            },
        ]
        .into_iter()
        .collect();
        assert_eq!(
            split.env,
            [
                McpVariable {
                    name: variable("ANTHROPIC_API_KEY"),
                    value: McpValue::Secret,
                },
                McpVariable {
                    name: variable("LOG_LEVEL"),
                    value: McpValue::Plain("debug".to_owned()),
                },
            ]
        );
        assert_eq!(
            split
                .secrets
                .get(&variable("ANTHROPIC_API_KEY"))
                .map(McpSecret::expose),
            Some("sk-ant-value")
        );
        assert_eq!(split.secrets.len(), 1);
        assert!(!format!("{split:?}").contains("sk-ant"));

        let agent = AcpAgent {
            id: "my-agent"
                .parse()
                .unwrap_or_else(|_| unreachable!("a valid id")),
            name: "Mine".to_owned(),
            program: "my-agent".to_owned(),
            args: Vec::new(),
            env: split.env,
            terminal: None,
        };
        let keys: Vec<String> = agent
            .secret_keys()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(keys, ["my-agent/ANTHROPIC_API_KEY"]);
        let persisted = serde_json::to_string(&agent).unwrap_or_default();
        assert!(!persisted.contains("sk-ant"), "{persisted}");
    }
}
