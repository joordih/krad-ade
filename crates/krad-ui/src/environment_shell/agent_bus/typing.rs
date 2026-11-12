use krad_agents::TerminalHandle;
use krad_domain::{AgentState, EnvironmentName, TerminalId};
use krad_protocol::WriteBy;
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
        if self.runtime.handle.bus().is_some() {
            self.bus_work
                .enters_due
                .push(PendingEnter::new(target, Instant::now()));
        } else {
            self.type_into(&target, ENTER.to_vec());
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
            .is_some_and(|session| session.hooked() && session.is_running());
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
            .is_some_and(|session| session.view.status().bracketed_paste);
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
        if self.runtime.handle.bus().is_none() {
            return;
        }
        let mut due = Instant::now();
        for keys in steps {
            due += KEY_STEP_DELAY;
            self.bus_work.keys_due.push(PendingKeys {
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
        let env = EnvironmentName::new(target.environment.clone());
        let id = TerminalId(target.terminal);
        let runs = self
            .envs
            .get(&env)
            .is_some_and(|live| live.sessions.contains_key(&id));
        if runs {
            self.write_terminal(&env, id, bytes, WriteBy::App);
        }
    }
}
