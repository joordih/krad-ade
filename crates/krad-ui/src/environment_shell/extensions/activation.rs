use gpui_kit::{AppContext as _, Context};
use krad_domain::{EnvironmentName, Host};
use krad_extensions::api::manifest::{ExtensionManifest, Placement};
use std::collections::HashSet;
use std::path::PathBuf;

use super::super::EnvironmentShell;
use super::events::Seen;
use super::matching::{activates_on_environment, activates_on_startup, workspace_markers};
use super::SlotKey;

#[derive(Default)]
pub(super) struct Watch {
    pub(super) seen: Seen,
    checked: HashSet<(String, PathBuf)>,
}

fn key_for(manifest: &ExtensionManifest, environment: &EnvironmentName) -> Option<SlotKey> {
    let placement = manifest.program.as_ref()?.placement;
    Some(SlotKey {
        extension: manifest.id.clone(),
        environment: match placement {
            Placement::App => None,
            Placement::Environment => Some(environment.clone()),
        },
    })
}

impl EnvironmentShell {
    fn start_if_idle(&mut self, key: SlotKey, cx: &mut Context<Self>) {
        if !self.extensions.slots.contains_key(&key) {
            self.start_program(key, Vec::new(), cx);
        }
    }

    pub(super) fn activate_on_startup(&mut self, cx: &mut Context<Self>) {
        let active = self.active.clone();
        for manifest in self.usable_manifests() {
            if activates_on_startup(&manifest) {
                if let Some(key) = key_for(&manifest, &active) {
                    self.start_if_idle(key, cx);
                }
            }
        }
        self.extensions.watch.checked.clear();
        self.extensions.watch.seen = Seen::default();
    }

    pub(super) fn activate_on_environment(
        &mut self,
        name: &EnvironmentName,
        cx: &mut Context<Self>,
    ) {
        for manifest in self.usable_manifests() {
            if activates_on_environment(&manifest) {
                if let Some(key) = key_for(&manifest, name) {
                    self.start_if_idle(key, cx);
                }
            }
        }
    }

    pub(super) fn activate_on_workspace(
        &mut self,
        name: &EnvironmentName,
        roots: &[PathBuf],
        cx: &mut Context<Self>,
    ) {
        let local = self
            .envs
            .get(name)
            .is_some_and(|live| matches!(live.data.host, Host::Local));
        if !local {
            return;
        }
        let mut candidates = Vec::new();
        for manifest in self.usable_manifests() {
            let markers = workspace_markers(&manifest);
            for root in roots {
                let checked = (manifest.id.to_string(), root.clone());
                if markers.is_empty() || !self.extensions.watch.checked.insert(checked) {
                    continue;
                }
                let files: Vec<PathBuf> = markers.iter().map(|marker| root.join(marker)).collect();
                if let Some(key) = key_for(&manifest, name) {
                    candidates.push((key, files));
                }
            }
        }
        if candidates.is_empty() {
            return;
        }
        let checking = cx.background_spawn(async move {
            candidates
                .into_iter()
                .filter(|(_, files)| files.iter().any(|file| file.exists()))
                .map(|(key, _)| key)
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let found = checking.await;
            this.update(cx, |shell, cx| {
                for key in found {
                    shell.start_if_idle(key, cx);
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }
}
