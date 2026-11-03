use krad_agents::{notice_text, Response};
use krad_domain::{parse_wsl_path, AgentState, EnvironmentName, TerminalId};
use krad_session::PtyInput;
use std::time::Instant;

use super::super::{EnvironmentShell, LiveEnv};
use super::{cli_name, handle, AgentBus, PendingEnter, Settlement, ENTER, NO_SUCH_TERMINAL};

impl EnvironmentShell {
    pub(in crate::environment_shell) fn service_agent_bus(&mut self, now: Instant) {
        let Some(mut bus) = self.agent_bus.take() else {
            return;
        };
        bus.settle_waiters.retain(|(_, reply)| !reply.is_canceled());
        let waiters = std::mem::take(&mut bus.settle_waiters);
        for (target, reply) in waiters {
            match self.settled_state(&target) {
                Settlement::Settled(_) if bus.is_starting(&target) => {
                    bus.settle_waiters.push((target, reply));
                }
                Settlement::Settled(state) => reply
                    .send(Response::Settled {
                        target: target.terminal,
                        state,
                    })
                    .unwrap_or_default(),
                Settlement::Working => bus.settle_waiters.push((target, reply)),
                Settlement::Missing => reply
                    .send(Response::Error(NO_SUCH_TERMINAL.to_owned()))
                    .unwrap_or_default(),
            }
        }
        let (keys_now, keys_later): (Vec<_>, Vec<_>) =
            bus.keys_due.drain(..).partition(|keys| keys.due <= now);
        bus.keys_due = keys_later;
        for pending in keys_now {
            self.type_into(&pending.target, pending.keys);
        }
        let (due, later): (Vec<_>, Vec<_>) =
            bus.enters_due.drain(..).partition(|enter| enter.due <= now);
        bus.enters_due = later;
        for enter in due {
            if enter.is_retry && !self.submit_unconfirmed(&enter.target) {
                continue;
            }
            self.type_into(&enter.target, ENTER.to_vec());
            if self.submit_unconfirmed(&enter.target) {
                bus.enters_due.extend(enter.retry(now));
            }
        }
        let (ready, waiting): (Vec<_>, Vec<_>) = bus
            .pending_prompts
            .drain(..)
            .partition(|prompt| prompt.due <= now);
        bus.pending_prompts = waiting;
        for prompt in ready {
            self.paste_text_into(&prompt.target, &prompt.text);
            bus.enters_due.push(PendingEnter::new(prompt.target, now));
        }
        for (env, live) in &mut self.envs {
            let settled: Vec<TerminalId> = live
                .data
                .nodes
                .iter()
                .filter(|node| node.state != AgentState::Working)
                .filter(|node| {
                    live.sessions
                        .get(&node.id)
                        .is_some_and(|session| session.is_agent && session.state.is_running())
                })
                .map(|node| node.id)
                .collect();
            for id in settled {
                let target = handle(env, id);
                let blocked = bus
                    .inbox_waiters
                    .iter()
                    .any(|(waiter, _)| *waiter == target);
                if blocked {
                    continue;
                }
                let in_wsl = live
                    .data
                    .group_of(id)
                    .is_some_and(|group| parse_wsl_path(&group.root).is_some());
                if let Some(count) = live.mailbox.notify(id.as_u64()) {
                    if let Some(session) = live.sessions.get(&id) {
                        let notice = notice_text(cli_name(in_wsl), count).into_bytes();
                        session
                            .writer_tx
                            .send(PtyInput::Bytes(notice))
                            .unwrap_or_default();
                    }
                    bus.enters_due.push(PendingEnter::new(target, now));
                }
            }
            Self::notice_free_chats(env, live, &bus);
        }
        self.agent_bus = Some(bus);
    }

    pub(super) fn notice_free_chats(env: &EnvironmentName, live: &mut LiveEnv, bus: &AgentBus) {
        let free: Vec<TerminalId> = live
            .chats
            .iter()
            .filter(|(_, chat)| chat.takes_bus_notice())
            .map(|(id, _)| *id)
            .collect();
        for id in free {
            let target = handle(env, id);
            if bus
                .inbox_waiters
                .iter()
                .any(|(waiter, _)| *waiter == target)
            {
                continue;
            }
            let Some(count) = live.mailbox.notify(id.as_u64()) else {
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
