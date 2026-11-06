use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::id::{HostKindId, MachineId};
use crate::protocol::request;
use crate::protocol::terminal::TerminalCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MachineState {
    Running,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Machine {
    pub id: MachineId,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    pub state: MachineState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListMachinesParams {
    pub kind: HostKindId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConnectMachineParams {
    pub kind: HostKindId,
    pub machine: MachineId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MachineChannel {
    pub command: TerminalCommand,
}

request!(
    ListMachines,
    "host/list",
    Program,
    ListMachinesParams,
    Vec<Machine>
);
request!(
    ConnectMachine,
    "host/connect",
    Program,
    ConnectMachineParams,
    MachineChannel
);
