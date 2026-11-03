use krad_agents::TerminalHandle;
use krad_domain::{AgentState, EnvironmentName, TerminalId};
use krad_session::PtyInput;
use std::time::Instant;

use super::super::terminal_actions::paste_bytes;
use super::super::EnvironmentShell;
use super::{handle, PendingEnter, PendingKeys, ENTER, KEY_STEP_DELAY};

impl EnvironmentShell {
    pub(in crate::environment_shell) fn submit_text_into(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        text: &str,
    ) {
        let target = handle(env, id);
        self.paste_text_into(&target, text);
        match &mut self.agent_bus {
            Some(bus) => bus
                .enters_due
                .push(PendingEnter::new(target, Instant::now())),
            None => self.type_into(&target, ENTER.to_vec()),
        }
    }

    pub(super) fn submit_unconfirmed(&self, target: &TerminalHandle) -> bool {
        let Some(live) = self
            .envs
            .get(&EnvironmentName::new(target.environment.clone()))
        else {
            return false;
        };
        let id = TerminalId(target.terminal);
        let hooked = live
            .sessions
            .get(&id)
            .is_some_and(|session| session.hooked && session.state.is_running());
        hooked
            && live
                .data
                .nodes
                .iter()
                .any(|node| node.id == id && node.state != AgentState::Working)
    }

    pub(super) fn paste_text_into(&self, target: &TerminalHandle, text: &str) {
        let bracketed = self
            .envs
            .get(&EnvironmentName::new(target.environment.clone()))
            .and_then(|live| live.sessions.get(&TerminalId(target.terminal)))
            .is_some_and(|session| session.engine.bracketed_paste());
        self.type_into(target, paste_bytes(text, bracketed));
    }

    pub(in crate::environment_shell) fn type_in_steps(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        steps: Vec<Vec<u8>>,
    ) {
        let target = handle(env, id);
        let mut steps = steps.into_iter();
        if let Some(first) = steps.next() {
            self.type_into(&target, first);
        }
        let Some(bus) = &mut self.agent_bus else {
            return;
        };
        let now = Instant::now();
        let mut due = now;
        for keys in steps {
            due += KEY_STEP_DELAY;
            bus.keys_due.push(PendingKeys {
                target: target.clone(),
                keys,
                due,
            });
        }
    }

    pub(in crate::environment_shell) fn type_into_terminal(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        bytes: Vec<u8>,
    ) {
        self.type_into(&handle(env, id), bytes);
    }

    pub(super) fn type_into(&self, target: &TerminalHandle, bytes: Vec<u8>) {
        if let Some(session) = self
            .envs
            .get(&EnvironmentName::new(target.environment.clone()))
            .and_then(|live| live.sessions.get(&TerminalId(target.terminal)))
        {
            session
                .writer_tx
                .send(PtyInput::Bytes(bytes))
                .unwrap_or_default();
        }
    }
}
