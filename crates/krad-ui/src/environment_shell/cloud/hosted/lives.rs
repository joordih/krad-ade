use gpui_kit::App;
use krad_client::RuntimeClient;
use krad_domain::{EnvironmentData, RuntimeId};
use std::collections::HashMap;
use std::sync::Arc;

use super::super::super::surface::ChatPlace;
use super::super::super::{EnvironmentShell, LiveEnv};

pub(in crate::environment_shell) struct HostedLive {
    live: LiveEnv,
    client: Arc<dyn RuntimeClient>,
}

#[derive(Default)]
pub(in crate::environment_shell) struct HostedLives(HashMap<ChatPlace, HostedLive>);

fn hosted_live(data: EnvironmentData, client: Arc<dyn RuntimeClient>) -> HostedLive {
    let mut live = LiveEnv::new(data, None);
    live.pending_chats.clear();
    HostedLive { live, client }
}

impl HostedLives {
    pub(in crate::environment_shell) fn get(&self, place: &ChatPlace) -> Option<&LiveEnv> {
        self.0.get(place).map(|hosted| &hosted.live)
    }

    pub(in crate::environment_shell) fn get_mut(
        &mut self,
        place: &ChatPlace,
    ) -> Option<&mut LiveEnv> {
        self.0.get_mut(place).map(|hosted| &mut hosted.live)
    }

    pub(in crate::environment_shell) fn client(
        &self,
        place: &ChatPlace,
    ) -> Option<&Arc<dyn RuntimeClient>> {
        self.0.get(place).map(|hosted| &hosted.client)
    }

    pub(in crate::environment_shell) fn places(&self) -> impl Iterator<Item = &ChatPlace> {
        self.0.keys()
    }

    pub(in crate::environment_shell) fn follow(
        &mut self,
        place: &ChatPlace,
        data: Option<&EnvironmentData>,
        client: Option<Arc<dyn RuntimeClient>>,
    ) {
        let (Some(data), Some(client)) = (data, client) else {
            if let Some(hosted) = self.0.get_mut(place) {
                hosted.live.chats.values_mut().for_each(|chat| {
                    chat.chat.problem = Some(super::UNREACHED.to_owned());
                });
            }
            return;
        };
        match self.0.get_mut(place) {
            Some(hosted) => {
                hosted.live.data.adopt(data);
                hosted.client = client;
            }
            None => {
                self.0
                    .insert(place.clone(), hosted_live(data.clone(), client));
            }
        }
    }

    pub(in crate::environment_shell) fn retain(&mut self, keep: impl Fn(&ChatPlace) -> bool) {
        self.0.retain(|place, _| keep(place));
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn hosted_lives(&self) -> &HostedLives {
        &self.cloud.lives
    }

    pub(in crate::environment_shell) fn hosted_lives_mut(&mut self) -> &mut HostedLives {
        &mut self.cloud.lives
    }

    pub(in crate::environment_shell) fn follow_hosted_place(
        &mut self,
        place: &ChatPlace,
        cx: &App,
    ) {
        let Some(runner) = place.runner() else {
            return;
        };
        let view = self.runtime.view.read(cx);
        let data = view.remotes.get(runner).and_then(|remote| {
            remote
                .environments
                .all()
                .iter()
                .find(|data| data.name == place.environment)
        });
        let client = view
            .remotes
            .route(&self.runtime.client, RuntimeId::Runner(runner));
        self.cloud.lives.follow(place, data, client);
    }

    pub(in crate::environment_shell) fn follow_hosted_places(&mut self, cx: &App) {
        let places: Vec<ChatPlace> = self.cloud.lives.places().cloned().collect();
        for place in places {
            self.follow_hosted_place(&place, cx);
        }
    }
}
