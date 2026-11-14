use gpui_kit::App;
use krad_domain::agents_api::notice_text;
use krad_domain::{parse_wsl_path, AgentState, EnvironmentName, Prompt, TerminalId};
use krad_protocol::{ChatMirror, Typing};
use krad_runtime::cli_name;

use super::super::runtime_view::{LocalChats, MailNotices};
use super::super::surface::ChatPlace;
use super::super::{EnvironmentShell, LiveEnv};

type Notice = (EnvironmentName, TerminalId, String);

fn takes_notice(mirror: &ChatMirror) -> bool {
    !mirror.state.working && mirror.state.queued.is_empty() && mirror.asks.is_empty()
}

fn in_wsl(live: &LiveEnv, id: TerminalId) -> bool {
    live.data
        .group_of(id)
        .is_some_and(|group| parse_wsl_path(&group.root).is_some())
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn service_agent_bus(&mut self, cx: &mut App) {
        if self.runtime.handle.bus().is_none() {
            return;
        }
        let mut typed = Vec::new();
        let mut sent = Vec::new();
        let envs = &self.envs;
        let chats = &self.runtime.chats;
        self.runtime.view.update(cx, |model, _| {
            for (env, live) in envs {
                let settled: Vec<TerminalId> = live
                    .data
                    .nodes
                    .iter()
                    .filter(|node| node.state != AgentState::Working)
                    .filter(|node| {
                        live.sessions
                            .get(&node.id)
                            .is_some_and(|session| session.is_agent() && session.is_running())
                    })
                    .map(|node| node.id)
                    .collect();
                for id in settled {
                    if let Some(count) = model.mail.notify(env, id) {
                        typed.push((
                            env.clone(),
                            id,
                            notice_text(cli_name(in_wsl(live, id)), count),
                        ));
                    }
                }
                sent.extend(Self::notice_free_chats(env, live, chats, &mut model.mail));
            }
        });
        for (env, id, text) in typed {
            self.type_typing(&env, id, Typing::Notice { text });
        }
        for (env, id, text) in sent {
            self.send_prompt(&ChatPlace::here(&env), id, Prompt::from(text), cx);
        }
    }

    fn notice_free_chats(
        env: &EnvironmentName,
        live: &LiveEnv,
        chats: &LocalChats,
        mail: &mut MailNotices,
    ) -> Vec<Notice> {
        let place = ChatPlace::here(env);
        live.data
            .nodes
            .iter()
            .map(|node| node.id)
            .filter(|id| chats.get(&place.chat(*id)).is_some_and(takes_notice))
            .filter_map(|id| {
                let count = mail.notify(env, id)?;
                Some((
                    env.clone(),
                    id,
                    notice_text(cli_name(in_wsl(live, id)), count),
                ))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AgentPreset, AskKind};
    use krad_protocol::{ChatAsk, ChatAskId, ChatState};

    #[test]
    fn a_bus_notice_waits_for_a_free_chat_and_never_jumps_the_queue() {
        let mut mirror = ChatMirror::new(ChatState::new(AgentPreset::ClaudeCode));
        assert!(takes_notice(&mirror));
        mirror.state.queued = vec![Prompt::from("Fix the race")];
        assert!(!takes_notice(&mirror), "the user's message goes first");
        mirror.state.queued.clear();
        mirror.state.working = true;
        assert!(!takes_notice(&mirror), "a turn is running");
        mirror.state.working = false;
        mirror.asks.push(ChatAsk {
            chat: krad_protocol::AgentRef {
                environment: EnvironmentName::new("main".to_owned()),
                terminal: TerminalId(2),
            },
            ask: ChatAskId(1),
            kind: AskKind::Plan {
                plan: "Rotate one tab at a time".to_owned(),
            },
            can_allow_always: false,
        });
        assert!(!takes_notice(&mirror), "the agent waits for an answer");
    }
}
