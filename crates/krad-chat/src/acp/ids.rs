use serde::{Deserialize, Serialize};

macro_rules! protocol_id {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
            #[serde(transparent)]
            pub struct $name(pub String);
        )+
    };
}

protocol_id!(
    SessionId,
    ToolCallId,
    TerminalId,
    PermissionOptionId,
    ModeId,
    ModelId,
    AuthMethodId,
);
