use krad_domain::{CloudRunnerId, EnvironmentName, Pairing};
use krad_protocol::{AgentRef, Event, EventKind, Seq};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::SystemTime;

pub const OWN_RUNNER: CloudRunnerId = CloudRunnerId(0);

#[derive(Clone)]
pub struct OwnRunner {
    pub cli: PathBuf,
    pub pairing: Pairing,
    pub environment: EnvironmentName,
}

impl fmt::Debug for OwnRunner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnRunner")
            .field("cli", &self.cli)
            .field("environment", &self.environment)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
pub(crate) struct OwnActivity(HashMap<AgentRef, (Seq, SystemTime)>);

impl OwnActivity {
    pub(crate) fn heard(&mut self, own: &EnvironmentName, events: &[Event]) {
        let now = SystemTime::now();
        for event in events {
            let Some(chat) = event.kind.chat().filter(|chat| chat.environment == *own) else {
                continue;
            };
            if matches!(event.kind, EventKind::ChatClosed { .. }) {
                self.0.remove(chat);
                continue;
            }
            self.0.insert(chat.clone(), (event.seq, now));
        }
    }

    pub(crate) fn of(&self, chat: &AgentRef) -> Option<(Seq, SystemTime)> {
        self.0.get(chat).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::TerminalId;

    fn chat(environment: &str) -> AgentRef {
        AgentRef {
            environment: EnvironmentName::new(environment.to_owned()),
            terminal: TerminalId(2),
        }
    }

    fn event(seq: u64, kind: EventKind) -> Event {
        Event {
            seq: Seq(seq),
            kind,
        }
    }

    #[test]
    fn only_the_own_environment_s_chats_leave_a_trace_until_they_close() {
        let own = EnvironmentName::new("runner".to_owned());
        let mut activity = OwnActivity::default();
        activity.heard(
            &own,
            &[
                event(
                    3,
                    EventKind::ChatClosed {
                        chat: chat("other"),
                    },
                ),
                event(
                    4,
                    EventKind::ChatMessagesCut {
                        chat: chat("runner"),
                        len: 0,
                    },
                ),
            ],
        );
        assert_eq!(
            activity.of(&chat("runner")).map(|(seq, _)| seq),
            Some(Seq(4))
        );
        assert_eq!(activity.of(&chat("other")), None);
        activity.heard(
            &own,
            &[event(
                5,
                EventKind::ChatClosed {
                    chat: chat("runner"),
                },
            )],
        );
        assert_eq!(activity.of(&chat("runner")), None);
    }
}
