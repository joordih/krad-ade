use krad_domain::{EnvironmentName, TerminalId};
use krad_protocol::{Command, TerminalCommand, Typing, WriteBy};

use super::super::live_terminal::terminal_ref;
use super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn type_typing(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        typing: Typing,
    ) {
        self.terminals()
            .post(Command::Terminal(TerminalCommand::Type {
                terminal: terminal_ref(env, id),
                typing,
            }));
    }

    pub(in crate::environment_shell) fn submit_text_into(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        text: &str,
    ) {
        let text = text.to_owned();
        self.type_typing(env, id, Typing::Submit { text });
    }

    pub(in crate::environment_shell) fn type_in_steps(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        steps: Vec<Vec<u8>>,
    ) {
        self.type_typing(env, id, Typing::Steps { steps });
    }

    pub(in crate::environment_shell) fn type_into_terminal(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        bytes: Vec<u8>,
    ) {
        let runs = self
            .envs
            .get(env)
            .is_some_and(|live| live.sessions.contains_key(&id));
        if runs {
            self.write_terminal(env, id, bytes, WriteBy::App);
        }
    }
}
