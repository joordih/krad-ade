use std::borrow::Cow;
use std::fmt;

const WAITING_TEXT: &str = "opencode is still starting";
const NO_CHAT_TEXT: &str = "has no chat";

pub(in crate::environment_shell) type HarnessName = Cow<'static, str>;

#[derive(Debug)]
pub(in crate::environment_shell) enum StartError {
    NoChat(HarnessName),
    Waiting,
}

impl fmt::Display for StartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoChat(name) => write!(formatter, "{name} {NO_CHAT_TEXT}"),
            Self::Waiting => formatter.write_str(WAITING_TEXT),
        }
    }
}
