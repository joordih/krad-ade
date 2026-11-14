use gpui_kit::App;
use krad_client::RuntimeClient;
use krad_domain::{EnvironmentData, EnvironmentName, RuntimeId};
use std::collections::HashMap;
use std::sync::Arc;

use super::super::{EnvironmentShell, LiveEnv};
use super::ChatPlace;

const UNREACHED: &str = "The cloud runner is not connected";

struct HostedPlace {
    views: LiveEnv,
    client: Arc<dyn RuntimeClient>,
}

#[derive(Default)]
pub(in crate::environment_shell) struct HostedPlaces(HashMap<ChatPlace, HostedPlace>);

fn hosted_place(data: EnvironmentData, client: Arc<dyn RuntimeClient>) -> HostedPlace {
    let mut views = LiveEnv::new(data, None);
    views.pending_chats.clear();
    HostedPlace { views, client }
}

fn reached<'a>(
    environments: &'a [EnvironmentData],
    environment: &EnvironmentName,
) -> Option<&'a EnvironmentData> {
    environments.iter().find(|data| data.name == *environment)
}

impl HostedPlaces {
    pub(in crate::environment_shell) fn get(&self, place: &ChatPlace) -> Option<&LiveEnv> {
        self.0.get(place).map(|hosted| &hosted.views)
    }

    pub(in crate::environment_shell) fn get_mut(
        &mut self,
        place: &ChatPlace,
    ) -> Option<&mut LiveEnv> {
        self.0.get_mut(place).map(|hosted| &mut hosted.views)
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
                hosted.views.chats.values_mut().for_each(|chat| {
                    chat.chat.problem = Some(UNREACHED.to_owned());
                });
            }
            return;
        };
        match self.0.get_mut(place) {
            Some(hosted) => {
                hosted.views.data.adopt(data);
                hosted.client = client;
            }
            None => {
                self.0
                    .insert(place.clone(), hosted_place(data.clone(), client));
            }
        }
    }

    pub(in crate::environment_shell) fn retain(&mut self, keep: impl Fn(&ChatPlace) -> bool) {
        self.0.retain(|place, _| keep(place));
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn follow_hosted_place(
        &mut self,
        place: &ChatPlace,
        cx: &App,
    ) {
        let Some(runner) = place.runner() else {
            return;
        };
        let remotes = &self.runtime.view.read(cx).remotes;
        let data = remotes
            .get(runner)
            .and_then(|remote| reached(remote.environments.all(), &place.environment));
        let client = remotes.route(&self.runtime.client, RuntimeId::Runner(runner));
        self.chat.hosted.follow(place, data, client);
    }

    pub(in crate::environment_shell) fn follow_hosted_places(&mut self, cx: &App) {
        let places: Vec<ChatPlace> = self.chat.hosted.places().cloned().collect();
        for place in places {
            self.follow_hosted_place(&place, cx);
        }
    }
}
