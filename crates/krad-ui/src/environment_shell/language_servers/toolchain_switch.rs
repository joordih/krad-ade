use gpui_kit::component::input::EditorState;
use gpui_kit::{AppContext as _, Context, Entity};
use krad_domain::{EnvironmentName, TerminalId, Toolchain, ToolchainScope};
use krad_extensions::{
    handoff, merged_settings, project_config_present, Handoff, HandoffApply, ServerPlan,
    ServerToolchain,
};
use krad_host::HostLink;
use krad_lsp::ServerSettings;
use std::collections::HashSet;
use std::path::PathBuf;

use super::super::EnvironmentShell;
use super::{Document, ServerKey, ServerSlot};

pub(super) struct HandoffInputs {
    server: ServerToolchain,
    toolchain: Option<Toolchain>,
    pub(super) link: HostLink,
    folder: PathBuf,
}

pub(super) fn computed_handoff(inputs: &HandoffInputs) -> Handoff {
    handoff(&inputs.server, inputs.toolchain.as_ref(), |config| {
        project_config_present(&inputs.link, &inputs.folder, config)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Switch {
    Unchanged,
    Live,
    Restart,
}

fn switch_for<K: PartialEq>(old: &K, new: &K, apply: HandoffApply, taken: bool) -> Switch {
    if old == new {
        return Switch::Unchanged;
    }
    match apply {
        HandoffApply::Live if !taken => Switch::Live,
        HandoffApply::Live | HandoffApply::Restart => Switch::Restart,
    }
}

impl EnvironmentShell {
    pub(super) fn toolchain_server_key(
        &self,
        document: &Document,
        plan: ServerPlan,
    ) -> Option<ServerKey> {
        let server = self.server_toolchain_of(&plan)?;
        let scope = self.toolchains.scope_of(document, &server.family)?;
        let effective = self.toolchains.effective(scope)?;
        Some(ServerKey {
            host: document.host.clone(),
            root: scope.folder(),
            plan,
            toolchain: effective
                .toolchain()
                .map(|toolchain| toolchain.location.clone()),
        })
    }

    pub(super) fn handoff_inputs(&self, key: &ServerKey) -> Option<HandoffInputs> {
        let server = self.server_toolchain_of(&key.plan)?;
        let (link, _) = self.server_link(&key.host)?;
        let toolchain = self
            .toolchains
            .served_toolchain(&key.host, &key.root, &server.family)
            .filter(|toolchain| key.toolchain.as_ref() == Some(&toolchain.location));
        Some(HandoffInputs {
            server,
            toolchain,
            link,
            folder: key.root.clone(),
        })
    }

    pub(in crate::environment_shell) fn toolchain_changed(
        &mut self,
        scope: &ToolchainScope,
        cx: &mut Context<Self>,
    ) {
        self.store();
        self.toolchains.activations.clear();
        self.refresh_toolchain_statuses();
        let mut handled: HashSet<ServerKey> = HashSet::new();
        let mut reopen: Vec<Document> = Vec::new();
        for document in self.toolchains.documents_in(scope) {
            let Some(old) = self.language_servers.documents.get(&document).cloned() else {
                reopen.push(document);
                continue;
            };
            if handled.contains(&old) {
                continue;
            }
            let Some(new) = self.toolchain_server_key(&document, old.plan.clone()) else {
                continue;
            };
            let apply = self
                .server_toolchain_of(&old.plan)
                .map_or_else(HandoffApply::default, |server| server.apply);
            let taken = self.language_servers.slots.contains_key(&new);
            let running = matches!(
                self.language_servers.slots.get(&old),
                Some(ServerSlot::Running(_))
            );
            match switch_for(&old, &new, apply, taken || !running) {
                Switch::Unchanged => {}
                Switch::Live => {
                    handled.insert(old.clone());
                    self.move_server_live(&old, &new, cx);
                }
                Switch::Restart => {
                    handled.insert(old.clone());
                    reopen.extend(self.stop_server(&old));
                }
            }
        }
        for document in reopen {
            self.reattach_document(&document, cx);
        }
        cx.notify();
    }

    fn stop_server(&mut self, key: &ServerKey) -> Vec<Document> {
        self.language_servers.slots.remove(key);
        let served: Vec<Document> = self
            .language_servers
            .documents
            .iter()
            .filter(|(_, served)| *served == key)
            .map(|(document, _)| document.clone())
            .collect();
        self.language_servers
            .documents
            .retain(|_, served| served != key);
        served
    }

    fn move_server_live(&mut self, old: &ServerKey, new: &ServerKey, cx: &mut Context<Self>) {
        let Some(slot) = self.language_servers.slots.remove(old) else {
            return;
        };
        let client = match &slot {
            ServerSlot::Running(running) => Some(running.server.client.clone()),
            _ => None,
        };
        self.language_servers.slots.insert(new.clone(), slot);
        for served in self.language_servers.documents.values_mut() {
            if served == old {
                *served = new.clone();
            }
        }
        let (Some(client), Some(inputs)) = (client, self.handoff_inputs(new)) else {
            return;
        };
        let base = self.server_config(new).sections();
        cx.background_spawn(async move {
            let handed = computed_handoff(&inputs);
            let settings = ServerSettings::new(merged_settings(base, &handed));
            if let Err(error) = client.reconfigure(settings) {
                tracing::warn!(%error, "a language server did not take its new toolchain");
            }
        })
        .detach();
    }

    pub(in crate::environment_shell) fn reattach_document(
        &mut self,
        document: &Document,
        cx: &mut Context<Self>,
    ) {
        let targets: Vec<(EnvironmentName, TerminalId, Entity<EditorState>)> = self
            .envs
            .iter()
            .filter(|(_, live)| live.data.host == document.host)
            .flat_map(|(name, live)| {
                live.editors
                    .iter()
                    .filter(|(_, editor)| editor.path == document.path)
                    .filter_map(|(id, editor)| Some((name.clone(), *id, editor.state.clone()?)))
            })
            .collect();
        for (env, id, state) in targets {
            self.attach_language_server_in(&env, id, &document.path, &state, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::ToolchainLocation;

    fn served(toolchain: Option<&str>) -> (PathBuf, Option<ToolchainLocation>) {
        (
            PathBuf::from("/srv/app"),
            toolchain.map(|path| ToolchainLocation::Path {
                path: PathBuf::from(path),
            }),
        )
    }

    #[test]
    fn live_servers_are_reconfigured_and_others_restart() {
        let (old, new) = (served(Some("/a/python")), served(Some("/b/python")));
        assert_eq!(
            switch_for(&old, &old, HandoffApply::Live, false),
            Switch::Unchanged
        );
        assert_eq!(
            switch_for(&old, &new, HandoffApply::Live, false),
            Switch::Live
        );
        assert_eq!(
            switch_for(&old, &new, HandoffApply::Restart, false),
            Switch::Restart
        );
        assert_eq!(
            switch_for(&old, &new, HandoffApply::Live, true),
            Switch::Restart
        );
        assert_eq!(
            switch_for(&old, &served(None), HandoffApply::Live, false),
            Switch::Live
        );
    }
}
