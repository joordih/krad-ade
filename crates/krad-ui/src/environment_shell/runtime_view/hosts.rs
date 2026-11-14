use krad_domain::{EnvironmentName, PromptAsker};
use krad_protocol::{EnvironmentHost, HostPrompt, HostState, PromptId, RemoteBusState};
use std::collections::{HashMap, VecDeque};

static NOT_SHOWN: HostState = HostState::NotShown;
static NO_BUS: RemoteBusState = RemoteBusState::Absent;

#[derive(Default)]
struct Shown {
    state: HostState,
    bus: RemoteBusState,
}

#[derive(Default)]
pub(in crate::environment_shell) struct HostsMirror {
    hosts: HashMap<EnvironmentName, Shown>,
    prompts: VecDeque<HostPrompt>,
}

pub(in crate::environment_shell) struct HostMove {
    pub(in crate::environment_shell) environment: EnvironmentName,
    pub(in crate::environment_shell) before: HostState,
    pub(in crate::environment_shell) after: HostState,
}

impl HostsMirror {
    pub(in crate::environment_shell) fn state(&self, environment: &EnvironmentName) -> &HostState {
        self.hosts
            .get(environment)
            .map_or(&NOT_SHOWN, |shown| &shown.state)
    }

    pub(in crate::environment_shell) fn bus(
        &self,
        environment: &EnvironmentName,
    ) -> &RemoteBusState {
        self.hosts
            .get(environment)
            .map_or(&NO_BUS, |shown| &shown.bus)
    }

    pub(in crate::environment_shell) fn changed(
        &mut self,
        environment: EnvironmentName,
        state: HostState,
    ) -> HostMove {
        let shown = self.hosts.entry(environment.clone()).or_default();
        let before = std::mem::replace(&mut shown.state, state.clone());
        HostMove {
            environment,
            before,
            after: state,
        }
    }

    pub(in crate::environment_shell) fn bus_changed(
        &mut self,
        environment: EnvironmentName,
        bus: RemoteBusState,
    ) -> RemoteBusState {
        let shown = self.hosts.entry(environment).or_default();
        std::mem::replace(&mut shown.bus, bus)
    }

    pub(in crate::environment_shell) fn prompted(&mut self, prompt: HostPrompt) {
        if self
            .prompts
            .iter()
            .all(|waiting| waiting.prompt != prompt.prompt)
        {
            self.prompts.push_back(prompt);
        }
    }

    pub(in crate::environment_shell) fn prompt_settled(&mut self, prompt: PromptId) {
        self.prompts.retain(|waiting| waiting.prompt != prompt);
    }

    pub(in crate::environment_shell) fn next_prompt(&mut self) -> Option<HostPrompt> {
        self.take_first(|asker| !matches!(asker, PromptAsker::Runner { .. }))
    }

    pub(in crate::environment_shell) fn next_runner_prompt(&mut self) -> Option<HostPrompt> {
        self.take_first(|asker| matches!(asker, PromptAsker::Runner { .. }))
    }

    pub(in crate::environment_shell) fn runner_prompt_waits(&self) -> bool {
        self.prompts
            .iter()
            .any(|prompt| matches!(prompt.asker, PromptAsker::Runner { .. }))
    }

    fn take_first(&mut self, wanted: impl Fn(&PromptAsker) -> bool) -> Option<HostPrompt> {
        let at = self
            .prompts
            .iter()
            .position(|prompt| wanted(&prompt.asker))?;
        self.prompts.remove(at)
    }

    pub(in crate::environment_shell) fn replace(
        &mut self,
        hosts: &[EnvironmentHost],
        prompts: Vec<HostPrompt>,
    ) -> Vec<HostMove> {
        let mut known = std::mem::take(&mut self.hosts);
        let moves = hosts
            .iter()
            .map(|host| {
                let before = known
                    .remove(&host.environment)
                    .map_or_else(HostState::default, |shown| shown.state);
                self.hosts.insert(
                    host.environment.clone(),
                    Shown {
                        state: host.state.clone(),
                        bus: host.bus.clone(),
                    },
                );
                HostMove {
                    environment: host.environment.clone(),
                    before,
                    after: host.state.clone(),
                }
            })
            .filter(|moved| moved.before != moved.after)
            .collect();
        self.prompts = prompts.into();
        moves
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{HostPlatform, PromptAsker, PromptKind};

    fn main() -> EnvironmentName {
        EnvironmentName::new("main".to_owned())
    }

    #[test]
    fn a_host_never_heard_of_is_not_shown_and_has_no_bus() {
        let mirror = HostsMirror::default();
        assert_eq!(*mirror.state(&main()), HostState::NotShown);
        assert_eq!(*mirror.bus(&main()), RemoteBusState::Absent);
    }

    #[test]
    fn a_change_tells_where_the_host_came_from() {
        let mut mirror = HostsMirror::default();
        mirror.changed(main(), HostState::Connecting);
        let moved = mirror.changed(
            main(),
            HostState::Connected {
                platform: HostPlatform::LinuxX86_64,
                home: None,
                login_env: krad_protocol::LoginEnv::Read,
            },
        );
        assert_eq!(moved.before, HostState::Connecting);
        assert!(mirror.state(&main()).is_connected());
        assert_eq!(
            mirror.bus_changed(main(), RemoteBusState::Preparing),
            RemoteBusState::Absent
        );
        assert!(mirror.bus(&main()).is_preparing());
    }

    #[test]
    fn a_snapshot_reports_only_the_hosts_that_moved_and_brings_the_prompts() {
        let mut mirror = HostsMirror::default();
        mirror.changed(main(), HostState::Connecting);
        let prompt = HostPrompt {
            prompt: PromptId(2),
            asker: PromptAsker::Git,
            kind: PromptKind::Secret,
            text: "Password: ".to_owned(),
        };
        let hosts = [EnvironmentHost {
            environment: main(),
            state: HostState::Connecting,
            bus: RemoteBusState::Absent,
            agents: Vec::new(),
        }];
        let unchanged = mirror.replace(&hosts, vec![prompt.clone()]);
        assert!(unchanged.is_empty());
        mirror.prompt_settled(PromptId(9));
        assert_eq!(mirror.next_prompt(), Some(prompt));
        assert_eq!(mirror.next_prompt(), None);
    }

    #[test]
    fn a_runner_s_prompts_are_kept_for_the_settings_and_never_reach_the_host_dialog() {
        let mut mirror = HostsMirror::default();
        let asked = |prompt: u64, asker: PromptAsker| HostPrompt {
            prompt: PromptId(prompt),
            asker,
            kind: PromptKind::Secret,
            text: "Password: ".to_owned(),
        };
        let runner = asked(
            1,
            PromptAsker::Runner {
                runner: krad_domain::CloudRunnerId(3),
            },
        );
        let git = asked(2, PromptAsker::Git);
        mirror.prompted(runner.clone());
        mirror.prompted(git.clone());
        assert_eq!(mirror.next_prompt(), Some(git));
        assert_eq!(mirror.next_prompt(), None);
        assert!(mirror.runner_prompt_waits());
        assert_eq!(mirror.next_runner_prompt(), Some(runner));
        assert!(!mirror.runner_prompt_waits());
        assert_eq!(mirror.next_runner_prompt(), None);
    }
}
