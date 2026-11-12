use krad_domain::{AgentPreset, EnvironmentName, GridSize, TerminalId};
use krad_protocol::{AgentRef, Command, TerminalCommand, WriteBy};
use krad_runtime::{RuntimeHandle, TerminalAction, TerminalView};
use std::time::Instant;

use super::EnvironmentShell;

pub(super) struct LiveTerminal {
    pub(super) view: TerminalView,
    pub(super) harness: Option<AgentPreset>,
    pub(super) spawned_at: Instant,
    pub(super) activity: Option<String>,
    hooked_here: bool,
    ended_heard: bool,
    grid: Option<GridSize>,
}

impl LiveTerminal {
    pub(super) fn new(view: TerminalView, harness: Option<AgentPreset>) -> Self {
        Self {
            view,
            harness,
            spawned_at: Instant::now(),
            activity: None,
            hooked_here: false,
            ended_heard: false,
            grid: None,
        }
    }

    pub(super) const fn is_agent(&self) -> bool {
        self.harness.is_some()
    }

    pub(super) fn is_running(&self) -> bool {
        self.view.is_running()
    }

    pub(super) fn hooked(&self) -> bool {
        self.hooked_here || self.view.status().hooked
    }

    pub(super) fn hear_end(&mut self) -> bool {
        !std::mem::replace(&mut self.ended_heard, true)
    }

    fn mark_hooked(&mut self) -> bool {
        !std::mem::replace(&mut self.hooked_here, true)
    }

    fn regrid(&mut self, size: GridSize) -> bool {
        self.grid.replace(size) != Some(size)
    }
}

pub(super) fn terminal_ref(env: &EnvironmentName, id: TerminalId) -> AgentRef {
    AgentRef {
        environment: env.clone(),
        terminal: id,
    }
}

pub(super) fn end_terminal_on(runtime: &RuntimeHandle, env: &EnvironmentName, id: TerminalId) {
    runtime.post(Command::Terminal(TerminalCommand::Close {
        terminal: terminal_ref(env, id),
    }));
}

impl EnvironmentShell {
    pub(super) fn terminals(&self) -> &RuntimeHandle {
        &self.runtime.handle
    }

    pub(super) fn write_terminal(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        bytes: Vec<u8>,
        by: WriteBy,
    ) {
        self.terminals()
            .write_terminal(terminal_ref(env, id), bytes, by);
    }

    pub(super) fn act_on_terminal(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        action: TerminalAction,
    ) {
        self.terminals()
            .act_on_terminal(terminal_ref(env, id), action);
    }

    pub(super) fn end_terminal(&self, env: &EnvironmentName, id: TerminalId) {
        end_terminal_on(self.terminals(), env, id);
    }

    pub(super) fn resize_terminal(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        size: GridSize,
    ) {
        let regridded = self
            .envs
            .get_mut(env)
            .and_then(|live| live.sessions.get_mut(&id))
            .is_some_and(|terminal| terminal.regrid(size));
        if regridded {
            self.terminals()
                .post(Command::Terminal(TerminalCommand::Resize {
                    terminal: terminal_ref(env, id),
                    size,
                }));
        }
    }

    pub(super) fn mark_terminal_hooked(&mut self, env: &EnvironmentName, id: TerminalId) {
        let marked = self
            .envs
            .get_mut(env)
            .and_then(|live| live.sessions.get_mut(&id))
            .is_some_and(LiveTerminal::mark_hooked);
        if marked {
            self.act_on_terminal(env, id, TerminalAction::Hooked);
        }
    }
}
