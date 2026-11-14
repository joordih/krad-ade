use std::fmt;
use std::num::ParseIntError;

const MISSING_SEPARATOR_TEXT: &str = "terminal handle must look like `<environment>:<terminal id>`";
const EMPTY_ENVIRONMENT_TEXT: &str = "terminal handle has an empty environment";
const INVALID_TERMINAL_TEXT: &str = "terminal handle has an invalid terminal or group id";

#[derive(Debug, PartialEq, Eq)]
pub enum HandleError {
    MissingSeparator,
    EmptyEnvironment,
    InvalidTerminal(ParseIntError),
}

impl fmt::Display for HandleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSeparator => formatter.write_str(MISSING_SEPARATOR_TEXT),
            Self::EmptyEnvironment => formatter.write_str(EMPTY_ENVIRONMENT_TEXT),
            Self::InvalidTerminal(error) => write!(formatter, "{INVALID_TERMINAL_TEXT}: {error}"),
        }
    }
}

impl std::error::Error for HandleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidTerminal(error) => Some(error),
            Self::MissingSeparator | Self::EmptyEnvironment => None,
        }
    }
}

impl From<ParseIntError> for HandleError {
    fn from(error: ParseIntError) -> Self {
        Self::InvalidTerminal(error)
    }
}
