mod commands;
mod hosts;
mod lifecycle;
mod settings;
mod terminal;
mod tools;
mod views;
mod window;
mod workspace;

pub use commands::{
    CommandTarget, ExecuteCommand, ExecuteCommandParams, RunCommand, RunCommandParams,
};
pub use hosts::{
    ConnectMachine, ConnectMachineParams, ListMachines, ListMachinesParams, Machine,
    MachineChannel, MachineState,
};
pub use lifecycle::{
    AppInfo, Exit, Feature, Initialize, InitializeParams, InitializeResult, Initialized, Log,
    LogLevel, LogParams, Shutdown,
};
pub use settings::{
    GetSecret, GetSecretParams, GetSettings, Secret, SetSecret, SetSecretParams, SettingValue,
    SettingsChanged, SettingsValues,
};
pub use terminal::{
    OpenTerminal, OpenTerminalParams, SendTerminal, SendTerminalParams, TerminalCommand,
    TerminalZone,
};
pub use tools::{CallTool, CallToolParams, CallToolResult, ToolContent, ToolOutcome};
pub use views::{
    CloseView, OpenView, OpenViewParams, PatchView, PatchViewParams, RenderView, Rows, RowsParams,
    RowsResult, ViewClosed, ViewEventNotification, ViewEventParams, ViewRef, MAX_ROWS_PER_REQUEST,
};
pub use window::{
    Answer, Confirm, ConfirmParams, InputBox, InputBoxParams, Pick, PickItem, PickMode, PickParams,
    Progress, ProgressParams, ProgressState, Toast, ToastParams, MAX_PERCENT,
};
pub use workspace::{
    EnvironmentInfo, FilePosition, GroupInfo, HostInfo, OpenFile, OpenFileParams, WorkspaceContext,
    WorkspaceContextResult, WorkspaceEvent, WorkspaceEventNotification,
};

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Party {
    App,
    Program,
}

pub trait Request {
    const METHOD: &'static str;
    const ANSWERED_BY: Party;
    type Params: Serialize + DeserializeOwned + JsonSchema;
    type Result: Serialize + DeserializeOwned + JsonSchema;
}

pub trait Notification {
    const METHOD: &'static str;
    const RECEIVED_BY: Party;
    type Params: Serialize + DeserializeOwned + JsonSchema;
}

macro_rules! request {
    ($name:ident, $method:literal, $party:ident, $params:ty, $result:ty) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {}

        impl $crate::protocol::Request for $name {
            const METHOD: &'static str = $method;
            const ANSWERED_BY: $crate::protocol::Party = $crate::protocol::Party::$party;
            type Params = $params;
            type Result = $result;
        }
    };
}

macro_rules! notification {
    ($name:ident, $method:literal, $party:ident, $params:ty) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {}

        impl $crate::protocol::Notification for $name {
            const METHOD: &'static str = $method;
            const RECEIVED_BY: $crate::protocol::Party = $crate::protocol::Party::$party;
            type Params = $params;
        }
    };
}

pub(crate) use notification;
pub(crate) use request;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Empty {}
