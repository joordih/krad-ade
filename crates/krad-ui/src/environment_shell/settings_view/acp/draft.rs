use krad_domain::{
    AcpAgent, AcpAgentId, AcpAgentIdError, McpSecret, McpValue, McpVariable, McpVariableName,
};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ValueKind {
    Plain,
    Secret,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StoredSecret {
    Kept,
    None,
}

pub(super) struct DraftVariable {
    pub(super) name: String,
    pub(super) value: String,
    pub(super) kind: ValueKind,
    pub(super) stored: StoredSecret,
}

pub(super) struct Draft {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) program: String,
    pub(super) args: String,
    pub(super) terminal: String,
    pub(super) variables: Vec<DraftVariable>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DraftError {
    Id(AcpAgentIdError),
    VariableName(String),
    EmptySecret(String),
}

impl fmt::Display for DraftError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id(error) => write!(formatter, "Id: {error}"),
            Self::VariableName(name) => write!(formatter, "{name:?} is not a valid name"),
            Self::EmptySecret(name) => write!(formatter, "Enter a value for {name}"),
        }
    }
}

pub(super) type Secrets = Vec<(McpVariableName, McpSecret)>;

pub(super) struct ReadyAgent {
    pub(super) agent: AcpAgent,
    pub(super) secrets: Secrets,
}

fn variables(rows: &[DraftVariable]) -> Result<(Vec<McpVariable>, Secrets), DraftError> {
    let mut env = Vec::new();
    let mut secrets = Vec::new();
    for row in rows {
        if row.name.trim().is_empty() && row.value.trim().is_empty() {
            continue;
        }
        let name: McpVariableName = row
            .name
            .parse()
            .map_err(|_| DraftError::VariableName(row.name.trim().to_owned()))?;
        let value = match row.kind {
            ValueKind::Plain => McpValue::Plain(row.value.clone()),
            ValueKind::Secret => {
                match (McpSecret::parse(&row.value), row.stored) {
                    (Some(secret), _) => secrets.push((name.clone(), secret)),
                    (None, StoredSecret::Kept) => {}
                    (None, StoredSecret::None) => {
                        return Err(DraftError::EmptySecret(name.to_string()))
                    }
                }
                McpValue::Secret
            }
        };
        env.push(McpVariable { name, value });
    }
    Ok((env, secrets))
}

impl Draft {
    pub(super) fn ready(&self) -> Result<ReadyAgent, DraftError> {
        let id: AcpAgentId = self.id.parse().map_err(DraftError::Id)?;
        let (env, secrets) = variables(&self.variables)?;
        let terminal = Some(self.terminal.trim())
            .filter(|terminal| !terminal.is_empty())
            .map(str::to_owned);
        Ok(ReadyAgent {
            agent: AcpAgent {
                id,
                name: self.name.trim().to_owned(),
                program: self.program.trim().to_owned(),
                args: self.args.split_whitespace().map(str::to_owned).collect(),
                env,
                terminal,
            },
            secrets,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(variables: Vec<DraftVariable>) -> Draft {
        Draft {
            id: " gemini ".to_owned(),
            name: " Gemini ".to_owned(),
            program: "gemini".to_owned(),
            args: " --experimental-acp ".to_owned(),
            terminal: " ".to_owned(),
            variables,
        }
    }

    fn row(name: &str, value: &str, kind: ValueKind, stored: StoredSecret) -> DraftVariable {
        DraftVariable {
            name: name.to_owned(),
            value: value.to_owned(),
            kind,
            stored,
        }
    }

    #[test]
    fn an_agent_keeps_plain_values_and_sets_secrets_aside() {
        let ready = draft(vec![
            row("LOG", "debug", ValueKind::Plain, StoredSecret::None),
            row(
                "GEMINI_API_KEY",
                "g-1",
                ValueKind::Secret,
                StoredSecret::None,
            ),
            row("KEPT", "", ValueKind::Secret, StoredSecret::Kept),
            row("", "", ValueKind::Plain, StoredSecret::None),
        ])
        .ready()
        .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(ready.agent.id.as_str(), "gemini");
        assert_eq!(ready.agent.name, "Gemini");
        assert_eq!(ready.agent.args, ["--experimental-acp"]);
        assert_eq!(ready.agent.terminal, None);
        let values: Vec<&McpValue> = ready.agent.env.iter().map(|var| &var.value).collect();
        assert_eq!(
            values,
            [
                &McpValue::Plain("debug".to_owned()),
                &McpValue::Secret,
                &McpValue::Secret
            ]
        );
        assert_eq!(ready.secrets.len(), 1);
        assert_eq!(ready.secrets[0].1.expose(), "g-1");
    }

    #[test]
    fn a_draft_without_a_valid_id_or_secret_is_explained() {
        let mut bad = draft(Vec::new());
        bad.id = "My Agent".to_owned();
        assert_eq!(
            bad.ready().err(),
            Some(DraftError::Id(AcpAgentIdError::Character))
        );
        let blank = draft(vec![row(
            "TOKEN",
            " ",
            ValueKind::Secret,
            StoredSecret::None,
        )]);
        assert_eq!(
            blank.ready().err(),
            Some(DraftError::EmptySecret("TOKEN".to_owned()))
        );
    }
}
