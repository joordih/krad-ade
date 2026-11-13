use gpui_kit::App;
use krad_agents::{notice_text, ErrorCode, Response};
use krad_domain::{parse_wsl_path, AgentState, EnvironmentName, TerminalId};
use krad_protocol::Typing;
use krad_runtime::cli_name;

use super::super::runtime_view::MailNotices;
use super::super::{EnvironmentShell, LiveEnv};
use super::{Settlement, NO_SUCH_TERMINAL};

impl EnvironmentShell {
    pub(in crate::environment_shell) fn service_agent_bus(&mut self, cx: &mut App) {
        if self.runtime.handle.bus().is_none() {
            return;
        }
        let mut waiters = std::mem::take(&mut self.bus_work.settle_waiters);
        waiters.retain(|(_, reply)| !reply.is_canceled());
        for (target, reply) in waiters {
            match self.settled_state(&target) {
                Settlement::Settled(state) => reply
                    .send(Response::Settled {
                        target: target.terminal,
                        state,
                    })
                    .unwrap_or_default(),
                Settlement::Working => self.bus_work.settle_waiters.push((target, reply)),
                Settlement::Missing => reply
                    .send(Response::error(ErrorCode::NotFound, NO_SUCH_TERMINAL))
                    .unwrap_or_default(),
            }
        }
        let mut notices = Vec::new();
        let envs = &mut self.envs;
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
                    let in_wsl = live
                        .data
                        .group_of(id)
                        .is_some_and(|group| parse_wsl_path(&group.root).is_some());
                    if let Some(count) = model.mail.notify(env, id) {
                        notices.push((env.clone(), id, notice_text(cli_name(in_wsl), count)));
                    }
                }
                Self::notice_free_chats(env, live, &mut model.mail);
            }
        });
        for (env, id, text) in notices {
            self.type_typing(&env, id, Typing::Notice { text });
        }
    }

    pub(super) fn notice_free_chats(
        env: &EnvironmentName,
        live: &mut LiveEnv,
        mail: &mut MailNotices,
    ) {
        let free: Vec<TerminalId> = live
            .chats
            .iter()
            .filter(|(_, chat)| chat.takes_bus_notice())
            .map(|(id, _)| *id)
            .collect();
        for id in free {
            let Some(count) = mail.notify(env, id) else {
                continue;
            };
            let in_wsl = live
                .data
                .group_of(id)
                .is_some_and(|group| parse_wsl_path(&group.root).is_some());
            if let Some(chat) = live.chats.get_mut(&id) {
                chat.queue_bus_notice(notice_text(cli_name(in_wsl), count));
                live.noticed_chats.push(id);
            }
        }
    }
}
