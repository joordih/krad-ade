pub use krad_domain::{
    AcpAuthMethodId as AuthMethodId, AcpConfigOptionId as ConfigOptionId, AcpModeId as ModeId,
    AcpModelId as ModelId, AcpPermissionOptionId as PermissionOptionId,
};
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

protocol_id!(SessionId, ToolCallId, TerminalId);
