use krad_domain::EnvironmentName;
use krad_protocol::{HostPrompt, HostState, HostsSnapshot, PromptId, RemoteBusState};
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
        self.prompts.pop_front()
    }

    pub(in crate::environment_shell) fn replace(
        &mut self,
        snapshot: HostsSnapshot,
    ) -> Vec<HostMove> {
        let mut known = std::mem::take(&mut self.hosts);
        let moves = snapshot
            .hosts
            .into_iter()
            .map(|host| {
                let before = known
                    .remove(&host.environment)
                    .map_or_else(HostState::default, |shown| shown.state);
                self.hosts.insert(
                    host.environment.clone(),
                    Shown {
                        state: host.state.clone(),
                        bus: host.bus,
                    },
                );
                HostMove {
                    environment: host.environment,
                    before,
                    after: host.state,
                }
            })
            .filter(|moved| moved.before != moved.after)
            .collect();
        self.prompts = snapshot.prompts.into();
        moves
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{HostPlatform, PromptAsker, PromptKind};
    use krad_protocol::EnvironmentHost;

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
        let unchanged = mirror.replace(HostsSnapshot {
            hosts: vec![EnvironmentHost {
                environment: main(),
                state: HostState::Connecting,
                bus: RemoteBusState::Absent,
            }],
            prompts: vec![prompt.clone()],
        });
        assert!(unchanged.is_empty());
        mirror.prompt_settled(PromptId(9));
        assert_eq!(mirror.next_prompt(), Some(prompt));
        assert_eq!(mirror.next_prompt(), None);
    }
}
