use krad_domain::CloudRunnerId;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SignInRun(u64);

#[derive(Debug, Default)]
pub(crate) struct SshSignIns {
    running: HashMap<CloudRunnerId, SignInRun>,
    last: SignInRun,
}

impl SshSignIns {
    pub(crate) fn start(&mut self, runner: CloudRunnerId) -> SignInRun {
        self.last = SignInRun(self.last.0 + 1);
        self.running.insert(runner, self.last);
        self.last
    }

    pub(crate) fn finish(&mut self, runner: CloudRunnerId, run: SignInRun) -> bool {
        if self.running.get(&runner) != Some(&run) {
            return false;
        }
        self.running.remove(&runner);
        true
    }

    pub(crate) fn end(&mut self, runner: CloudRunnerId) -> bool {
        self.running.remove(&runner).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_latest_sign_in_of_a_runner_finishes_and_an_ended_one_never_does() {
        let runner = CloudRunnerId(2);
        let mut sign_ins = SshSignIns::default();
        assert!(!sign_ins.end(runner));
        let first = sign_ins.start(runner);
        let second = sign_ins.start(runner);
        assert!(!sign_ins.finish(runner, first));
        assert!(sign_ins.finish(runner, second));
        assert!(!sign_ins.finish(runner, second));
        let third = sign_ins.start(runner);
        assert!(sign_ins.end(runner));
        assert!(!sign_ins.finish(runner, third));
    }
}
