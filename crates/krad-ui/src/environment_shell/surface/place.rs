use futures::future::BoxFuture;
use futures::FutureExt as _;
use gpui_kit::App;
use krad_client::{ClientError, RuntimeClient};
use krad_domain::{CloudRunnerId, EnvironmentName, RuntimeId, TerminalId};
use krad_protocol::{AgentRef, Answer, Command};
use std::sync::Arc;

use super::super::cloud::HostedLives;
use super::super::{EnvironmentShell, LiveEnv};

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(in crate::environment_shell) struct ChatPlace {
    pub(in crate::environment_shell) runtime: RuntimeId,
    pub(in crate::environment_shell) environment: EnvironmentName,
}

impl ChatPlace {
    pub(in crate::environment_shell) fn here(environment: &EnvironmentName) -> Self {
        Self {
            runtime: RuntimeId::Local,
            environment: environment.clone(),
        }
    }

    pub(in crate::environment_shell) const fn hosted(
        runner: CloudRunnerId,
        environment: EnvironmentName,
    ) -> Self {
        Self {
            runtime: RuntimeId::Runner(runner),
            environment,
        }
    }

    pub(in crate::environment_shell) fn chat(&self, terminal: TerminalId) -> AgentRef {
        AgentRef {
            environment: self.environment.clone(),
            terminal,
        }
    }

    pub(in crate::environment_shell) const fn local(&self) -> Option<&EnvironmentName> {
        match self.runtime {
            RuntimeId::Local => Some(&self.environment),
            RuntimeId::Runner(_) => None,
        }
    }

    pub(in crate::environment_shell) const fn runner(&self) -> Option<CloudRunnerId> {
        match self.runtime {
            RuntimeId::Local => None,
            RuntimeId::Runner(runner) => Some(runner),
        }
    }
}

pub(in crate::environment_shell) fn route_chat<'a>(
    place: &ChatPlace,
    local: &'a Arc<dyn RuntimeClient>,
    hosted: &'a HostedLives,
) -> Option<&'a Arc<dyn RuntimeClient>> {
    match place.runtime {
        RuntimeId::Local => Some(local),
        RuntimeId::Runner(_) => hosted.client(place),
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn chat_place(&self) -> ChatPlace {
        self.shown_hosted_place()
            .unwrap_or_else(|| ChatPlace::here(&self.active))
    }

    pub(in crate::environment_shell) fn shown_here(&self) -> Option<EnvironmentName> {
        self.shown_hosted_place()
            .is_none()
            .then(|| self.active.clone())
    }

    pub(in crate::environment_shell) fn live(&self, place: &ChatPlace) -> Option<&LiveEnv> {
        match place.runtime {
            RuntimeId::Local => self.envs.get(&place.environment),
            RuntimeId::Runner(_) => self.hosted_lives().get(place),
        }
    }

    pub(in crate::environment_shell) fn live_mut(
        &mut self,
        place: &ChatPlace,
    ) -> Option<&mut LiveEnv> {
        match place.runtime {
            RuntimeId::Local => self.envs.get_mut(&place.environment),
            RuntimeId::Runner(_) => self.hosted_lives_mut().get_mut(place),
        }
    }

    pub(in crate::environment_shell) fn ask_at(
        &self,
        place: &ChatPlace,
        command: Command,
        cx: &mut App,
    ) -> BoxFuture<'static, Result<Answer, ClientError>> {
        match place.runner() {
            None => self.runtime.ask(command, cx).boxed(),
            Some(runner) => self.runtime.ask_runner(runner, command, cx),
        }
    }

    pub(in crate::environment_shell) fn chat_client(
        &self,
        place: &ChatPlace,
    ) -> Option<&Arc<dyn RuntimeClient>> {
        route_chat(place, &self.runtime.client, self.hosted_lives())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hosted_place_names_its_runner_and_never_a_local_environment() {
        let name = EnvironmentName::new("runner".to_owned());
        let hosted = ChatPlace::hosted(CloudRunnerId(2), name.clone());
        assert_eq!(hosted.runner(), Some(CloudRunnerId(2)));
        assert_eq!(hosted.local(), None);
        assert_eq!(hosted.chat(TerminalId(5)).environment, name);
        let here = ChatPlace::here(&name);
        assert_eq!(here.local(), Some(&name));
        assert_ne!(here, hosted);
    }
}
