use krad_domain::{EnvironmentName, TerminalId};
use krad_protocol::MailCount;
use std::collections::HashMap;

use super::{spot, AgentSpot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Unread {
    count: usize,
    noticed: bool,
}

#[derive(Default)]
pub(in crate::environment_shell) struct MailNotices(HashMap<AgentSpot, Unread>);

impl MailNotices {
    pub(super) fn set(&mut self, mail: MailCount) {
        let key = spot(mail.agent);
        if mail.unread == 0 {
            self.0.remove(&key);
            return;
        }
        let unread = self.0.entry(key).or_insert(Unread {
            count: 0,
            noticed: false,
        });
        if mail.unread > unread.count {
            unread.noticed = false;
        }
        unread.count = mail.unread;
    }

    pub(super) fn replace(&mut self, counts: Vec<MailCount>) {
        let known = std::mem::take(&mut self.0);
        for mail in counts {
            let key = spot(mail.agent.clone());
            let noticed = known
                .get(&key)
                .is_some_and(|unread| unread.noticed && unread.count >= mail.unread);
            self.0.insert(
                key,
                Unread {
                    count: mail.unread,
                    noticed,
                },
            );
        }
    }

    pub(in crate::environment_shell) fn notify(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<usize> {
        let unread = self.0.get_mut(&(env.clone(), id))?;
        if unread.noticed {
            return None;
        }
        unread.noticed = true;
        Some(unread.count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_protocol::AgentRef;

    fn mail(unread: usize) -> MailCount {
        MailCount {
            agent: AgentRef {
                environment: EnvironmentName::new("main".to_owned()),
                terminal: TerminalId(2),
            },
            unread,
        }
    }

    #[test]
    fn an_agent_is_told_of_its_mail_once_until_more_arrives() {
        let env = EnvironmentName::new("main".to_owned());
        let mut notices = MailNotices::default();
        assert_eq!(notices.notify(&env, TerminalId(2)), None);
        notices.set(mail(1));
        assert_eq!(notices.notify(&env, TerminalId(2)), Some(1));
        assert_eq!(notices.notify(&env, TerminalId(2)), None);
        notices.set(mail(2));
        assert_eq!(notices.notify(&env, TerminalId(2)), Some(2));
        notices.set(mail(0));
        assert_eq!(notices.notify(&env, TerminalId(2)), None);
    }

    #[test]
    fn a_snapshot_keeps_a_notice_already_given_for_the_same_mail() {
        let env = EnvironmentName::new("main".to_owned());
        let mut notices = MailNotices::default();
        notices.set(mail(1));
        assert_eq!(notices.notify(&env, TerminalId(2)), Some(1));
        notices.replace(vec![mail(1)]);
        assert_eq!(notices.notify(&env, TerminalId(2)), None);
        notices.replace(vec![mail(3)]);
        assert_eq!(notices.notify(&env, TerminalId(2)), Some(3));
    }
}
