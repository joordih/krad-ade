use krad_domain::CloudRunnerId;
use krad_protocol::AgentRef;
use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq)]
enum SshSignIn {
    Wanted,
    Open(AgentRef),
}

#[derive(Debug, Default)]
pub(crate) struct SshSignIns(HashMap<CloudRunnerId, SshSignIn>);

impl SshSignIns {
    pub(crate) fn want(&mut self, runner: CloudRunnerId) {
        self.0.insert(runner, SshSignIn::Wanted);
    }

    pub(crate) fn open(&mut self, runner: CloudRunnerId, terminal: &AgentRef) -> bool {
        let Some(sign_in) = self.0.get_mut(&runner) else {
            return false;
        };
        if *sign_in != SshSignIn::Wanted {
            return false;
        }
        *sign_in = SshSignIn::Open(terminal.clone());
        true
    }

    pub(crate) fn opened(&self) -> Vec<(CloudRunnerId, AgentRef)> {
        self.0
            .iter()
            .filter_map(|(runner, sign_in)| match sign_in {
                SshSignIn::Open(terminal) => Some((*runner, terminal.clone())),
                SshSignIn::Wanted => None,
            })
            .collect()
    }

    pub(crate) fn finish(&mut self, runner: CloudRunnerId) -> Option<AgentRef> {
        match self.0.remove(&runner)? {
            SshSignIn::Open(terminal) => Some(terminal),
            SshSignIn::Wanted => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{EnvironmentName, TerminalId};

    fn terminal(id: u64) -> AgentRef {
        AgentRef {
            environment: EnvironmentName::new("main".to_owned()),
            terminal: TerminalId(id),
        }
    }

    #[test]
    fn only_a_wanted_sign_in_opens_once_and_finishing_hands_its_terminal_back() {
        let runner = CloudRunnerId(2);
        let mut sign_ins = SshSignIns::default();
        assert!(!sign_ins.open(runner, &terminal(1)));
        sign_ins.want(runner);
        assert_eq!(sign_ins.opened(), Vec::new());
        assert!(sign_ins.open(runner, &terminal(1)));
        assert!(!sign_ins.open(runner, &terminal(2)));
        assert_eq!(sign_ins.opened(), [(runner, terminal(1))]);
        assert_eq!(sign_ins.finish(runner), Some(terminal(1)));
        assert_eq!(sign_ins.finish(runner), None);
        sign_ins.want(runner);
        assert_eq!(sign_ins.finish(runner), None);
    }
}
