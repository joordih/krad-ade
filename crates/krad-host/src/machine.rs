use krad_domain::ExtensionMachine;
use std::fmt;
use std::sync::Mutex;

use crate::ssh_link::lock;

#[derive(Clone, PartialEq, Eq)]
pub struct MachineChannel {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl fmt::Debug for MachineChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.env.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("MachineChannel")
            .field("program", &self.program)
            .field("args", &self.args)
            .field("env", &names)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct MachineReach {
    pub(crate) machine: ExtensionMachine,
    channel: Mutex<Option<MachineChannel>>,
}

impl MachineReach {
    pub(crate) const fn new(machine: ExtensionMachine) -> Self {
        Self {
            machine,
            channel: Mutex::new(None),
        }
    }

    pub(crate) fn set_channel(&self, channel: MachineChannel) {
        *lock(&self.channel) = Some(channel);
    }

    pub(crate) fn channel(&self) -> Option<MachineChannel> {
        lock(&self.channel).clone()
    }
}

#[cfg(test)]
mod tests;
