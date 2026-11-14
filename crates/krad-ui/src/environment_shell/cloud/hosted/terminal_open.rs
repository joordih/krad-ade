use gpui_kit::{AnyElement, Context, IntoElement};
use krad_client::ClientError;
use krad_domain::{AgentPreset, CloudRunnerId, LaunchIntent, NodeEdit, RuntimeId, ToastKind};
use krad_protocol::{AgentRef, Answer, Command, NodeCommand, TerminalCommand};

use super::super::super::EnvironmentShell;
use super::terminal::{session_terminals, HostedTerminalKey};
use super::HostedChatKey;
use crate::components::{Button, ButtonVariant};

const CHIP_PREFIX: &str = "hosted-terminal-chip";
const CHIP_LABEL: &str = "Terminal";
const NEW_PREFIX: &str = "hosted-terminal-new";
const NEW_SHELL_LABEL: &str = "+ Terminal";
const NEW_AGENT_PREFIX: &str = "+";
const NOT_OPENED: &str = "The runner could not open the terminal";
const UNREACHED: &str = "The cloud runner is not connected";

pub(in crate::environment_shell) fn new_terminal_intents(
    harness: Option<AgentPreset>,
) -> Vec<LaunchIntent> {
    let agent = harness.filter(|harness| harness.program().is_some());
    std::iter::once(AgentPreset::Shell)
        .chain(agent)
        .map(|preset| LaunchIntent::Preset { preset })
        .collect()
}

pub(in crate::environment_shell) fn open_beside(
    beside: &AgentRef,
    intent: LaunchIntent,
) -> Command {
    Command::Terminal(TerminalCommand::OpenBeside {
        beside: beside.clone(),
        intent,
    })
}

pub(in crate::environment_shell) fn close_on_runner(terminal: &AgentRef) -> [Command; 2] {
    [
        Command::Terminal(TerminalCommand::Close {
            terminal: terminal.clone(),
        }),
        Command::Node(NodeCommand {
            environment: terminal.environment.clone(),
            edit: NodeEdit::Close {
                terminal: terminal.terminal,
            },
        }),
    ]
}

pub(in crate::environment_shell) fn opened_terminal(
    answer: Result<Answer, ClientError>,
) -> Result<AgentRef, String> {
    match answer {
        Ok(Answer::TerminalOpened { terminal }) => Ok(terminal),
        Ok(_) => Err(NOT_OPENED.to_owned()),
        Err(error) => Err(format!("{NOT_OPENED}: {error}")),
    }
}

fn intent_label(intent: &LaunchIntent, name: &str) -> String {
    match intent.harness() {
        Some(_) => format!("{NEW_AGENT_PREFIX} {name}"),
        None => NEW_SHELL_LABEL.to_owned(),
    }
}

impl EnvironmentShell {
    fn open_hosted_terminal(
        &mut self,
        key: &HostedChatKey,
        intent: LaunchIntent,
        cx: &mut Context<Self>,
    ) {
        let runner = key.runner;
        let command = open_beside(&key.chat, intent);
        let answer = self.runtime.ask_runner(runner, command, cx);
        cx.spawn(async move |this, cx| {
            let opened = opened_terminal(answer.await);
            this.update(cx, |shell, cx| match opened {
                Ok(terminal) => {
                    shell.show_hosted_terminal(&HostedTerminalKey { runner, terminal }, cx);
                }
                Err(message) => shell.toast(ToastKind::Warning, &message, cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn close_hosted_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.shown_hosted_terminal().cloned() else {
            return;
        };
        let Some(client) = self.client_for(RuntimeId::Runner(key.runner), cx) else {
            return self.toast(ToastKind::Warning, UNREACHED, cx);
        };
        for command in close_on_runner(&key.terminal) {
            client.post(command);
        }
        self.leave_hosted_terminal();
        cx.notify();
    }

    pub(in crate::environment_shell) fn hosted_terminal_chips(
        &self,
        key: &HostedChatKey,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let runner = key.runner;
        let beside = &key.chat;
        let model = self.runtime.view.read(cx);
        let remote = model.remotes.get(runner);
        let terminals = remote
            .and_then(|remote| {
                remote
                    .environments
                    .all()
                    .iter()
                    .find(|data| data.name == beside.environment)
            })
            .map_or_else(Vec::new, |data| session_terminals(data, beside.terminal));
        let harness = remote
            .and_then(|remote| remote.chats.get(beside))
            .map(|chat| chat.mirror.state.harness);
        let mut shown: Vec<AnyElement> = terminals
            .into_iter()
            .map(|chip| self.terminal_chip(runner, beside, chip, cx))
            .collect();
        for (index, intent) in new_terminal_intents(harness).into_iter().enumerate() {
            shown.push(self.new_terminal_button(key, (index, intent), cx));
        }
        shown
    }

    fn terminal_chip(
        &self,
        runner: CloudRunnerId,
        beside: &AgentRef,
        (terminal, harness): (krad_domain::TerminalId, Option<AgentPreset>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = harness.map_or_else(
            || CHIP_LABEL.to_owned(),
            |harness| self.settings.acp.harness_name(harness).into_owned(),
        );
        let key = HostedTerminalKey {
            runner,
            terminal: AgentRef {
                environment: beside.environment.clone(),
                terminal,
            },
        };
        Button::new((CHIP_PREFIX, terminal.as_u64()))
            .label(format!("{name} {}", terminal.as_u64()))
            .variant(ButtonVariant::Ghost)
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.show_hosted_terminal(&key, cx);
            }))
            .into_any_element()
    }

    fn new_terminal_button(
        &self,
        key: &HostedChatKey,
        (index, intent): (usize, LaunchIntent),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = intent.harness().map_or_else(String::new, |harness| {
            self.settings.acp.harness_name(harness).into_owned()
        });
        let label = intent_label(&intent, &name);
        let key = key.clone();
        Button::new((NEW_PREFIX, index))
            .label(label)
            .variant(ButtonVariant::Ghost)
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.open_hosted_terminal(&key, intent.clone(), cx);
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_client::RecordedClient;
    use krad_client::RuntimeClient;
    use krad_domain::{EnvironmentName, TerminalId};
    use krad_protocol::ErrorCode;
    use std::sync::Arc;

    fn on_runner(terminal: u64) -> AgentRef {
        AgentRef {
            environment: EnvironmentName::new("runner".to_owned()),
            terminal: TerminalId(terminal),
        }
    }

    #[test]
    fn a_hosted_chat_offers_a_shell_and_its_own_terminal_agent() {
        let shell = LaunchIntent::Preset {
            preset: AgentPreset::Shell,
        };
        let claude = LaunchIntent::Preset {
            preset: AgentPreset::ClaudeCode,
        };
        assert_eq!(
            new_terminal_intents(Some(AgentPreset::ClaudeCode)),
            [shell.clone(), claude]
        );
        assert_eq!(
            new_terminal_intents(Some(AgentPreset::Gateway)),
            std::slice::from_ref(&shell)
        );
        assert_eq!(new_terminal_intents(None), [shell]);
    }

    #[test]
    fn a_new_terminal_is_opened_and_closed_on_the_runner_through_its_client() {
        let runner = RecordedClient::new(Vec::new());
        let client: Arc<dyn RuntimeClient> = Arc::clone(&runner) as Arc<dyn RuntimeClient>;
        let chat = on_runner(3);
        let shell = LaunchIntent::Preset {
            preset: AgentPreset::Shell,
        };
        client.post(open_beside(&chat, shell.clone()));
        let opened = on_runner(8);
        for command in close_on_runner(&opened) {
            client.post(command);
        }
        assert_eq!(
            runner.sent(),
            [
                Command::Terminal(TerminalCommand::OpenBeside {
                    beside: chat,
                    intent: shell,
                }),
                Command::Terminal(TerminalCommand::Close {
                    terminal: opened.clone(),
                }),
                Command::Node(NodeCommand {
                    environment: opened.environment.clone(),
                    edit: NodeEdit::Close {
                        terminal: opened.terminal,
                    },
                }),
            ]
        );
    }

    #[test]
    fn only_a_terminal_opened_answer_names_the_terminal_to_watch() {
        let opened = on_runner(8);
        let answer = Ok(Answer::TerminalOpened {
            terminal: opened.clone(),
        });
        assert_eq!(opened_terminal(answer), Ok(opened));
        assert!(opened_terminal(Ok(Answer::Done)).is_err());
        let refused = Err(ClientError::Refused {
            code: ErrorCode::NotFound,
            message: "the terminal to open beside is in no session".to_owned(),
        });
        let message = opened_terminal(refused).err().unwrap_or_default();
        assert!(message.contains("in no session"), "{message}");
    }
}
