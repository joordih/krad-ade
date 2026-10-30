use crate::error::HandleError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const SEPARATOR: char = ':';
const LEAD_PREFIX: &str = "lead-";

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TerminalHandle {
    pub environment: String,
    pub terminal: u64,
}

impl fmt::Display for TerminalHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{SEPARATOR}{}", self.environment, self.terminal)
    }
}

impl FromStr for TerminalHandle {
    type Err = HandleError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (environment, terminal) = text
            .rsplit_once(SEPARATOR)
            .ok_or(HandleError::MissingSeparator)?;
        if environment.is_empty() {
            return Err(HandleError::EmptyEnvironment);
        }
        Ok(Self {
            environment: environment.to_owned(),
            terminal: terminal.parse()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LeadHandle {
    pub environment: String,
    pub session: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Caller {
    Terminal(TerminalHandle),
    Lead(LeadHandle),
}

impl fmt::Display for Caller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Terminal(handle) => handle.fmt(f),
            Self::Lead(handle) => write!(
                f,
                "{}{SEPARATOR}{LEAD_PREFIX}{}",
                handle.environment, handle.session
            ),
        }
    }
}

impl FromStr for Caller {
    type Err = HandleError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (environment, last) = text
            .rsplit_once(SEPARATOR)
            .ok_or(HandleError::MissingSeparator)?;
        let Some(session) = last.strip_prefix(LEAD_PREFIX) else {
            return text.parse().map(Self::Terminal);
        };
        let session = session.parse()?;
        if environment.is_empty() {
            return Err(HandleError::EmptyEnvironment);
        }
        Ok(Self::Lead(LeadHandle {
            environment: environment.to_owned(),
            session,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callers_roundtrip_as_terminals_or_leads() -> Result<(), HandleError> {
        assert_eq!(
            "main:7".parse::<Caller>()?,
            Caller::Terminal(TerminalHandle {
                environment: "main".to_owned(),
                terminal: 7,
            })
        );
        let lead = Caller::Lead(LeadHandle {
            environment: "main".to_owned(),
            session: 2,
        });
        assert_eq!(lead.to_string(), "main:lead-2");
        assert_eq!(lead.to_string().parse::<Caller>()?, lead);
        assert!(matches!(
            "main:lead-x".parse::<Caller>(),
            Err(HandleError::InvalidTerminal(_))
        ));
        Ok(())
    }

    #[test]
    fn display_and_parse_roundtrip_with_colons_in_environment() -> Result<(), HandleError> {
        let handle = TerminalHandle {
            environment: "wsl:Ubuntu".to_owned(),
            terminal: 42,
        };
        assert_eq!(handle.to_string(), "wsl:Ubuntu:42");
        assert_eq!(handle.to_string().parse::<TerminalHandle>()?, handle);
        Ok(())
    }

    #[test]
    fn rejects_malformed_handles() {
        assert_eq!(
            "main".parse::<TerminalHandle>(),
            Err(HandleError::MissingSeparator)
        );
        assert_eq!(
            ":3".parse::<TerminalHandle>(),
            Err(HandleError::EmptyEnvironment)
        );
        assert!(matches!(
            "main:x".parse::<TerminalHandle>(),
            Err(HandleError::InvalidTerminal(_))
        ));
    }
}
