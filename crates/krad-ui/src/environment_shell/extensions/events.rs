use gpui_kit::Context;
use krad_domain::EnvironmentName;
use krad_extensions::api::protocol::{
    GroupInfo, SettingsChanged, WorkspaceEvent, WorkspaceEventNotification,
};
use krad_extensions::api::ExtensionId;
use std::path::{Path, PathBuf};

use super::super::EnvironmentShell;
use super::{Slot, SlotKey};

#[derive(Default, Clone, PartialEq, Eq)]
pub(super) struct Seen {
    environments: Vec<EnvironmentName>,
    groups: Vec<(EnvironmentName, Vec<PathBuf>)>,
    active_file: Option<PathBuf>,
}

impl EnvironmentShell {
    fn running_keys(&self) -> Vec<SlotKey> {
        self.extensions
            .slots
            .iter()
            .filter(|(_, slot)| matches!(slot, Slot::Running(_)))
            .map(|(key, _)| key.clone())
            .collect()
    }

    fn broadcast(&self, environment: Option<&EnvironmentName>, event: &WorkspaceEvent) {
        for key in self.running_keys() {
            let concerned = match (&key.environment, environment) {
                (None, _) | (_, None) => true,
                (Some(own), Some(changed)) => own == changed,
            };
            if !concerned {
                continue;
            }
            if let Some(connection) = self.extensions.connection(&key) {
                if let Err(error) = connection.notify::<WorkspaceEventNotification>(event) {
                    tracing::debug!(%error, "workspace event not delivered");
                }
            }
        }
    }

    pub(in crate::environment_shell) fn announce_settings(&self, extension: &ExtensionId) {
        for key in self.running_keys() {
            if key.extension != *extension {
                continue;
            }
            let values = self.extension_values(&key);
            if let Some(connection) = self.extensions.connection(&key) {
                if let Err(error) = connection.notify::<SettingsChanged>(&values) {
                    tracing::debug!(%error, "settings change not delivered");
                }
            }
        }
    }

    pub(in crate::environment_shell) fn announce_file_saved(&self, path: &Path) {
        let event = WorkspaceEvent::FileSaved {
            path: path.to_path_buf(),
        };
        self.broadcast(Some(&self.active), &event);
    }

    fn seen_now(&self) -> Seen {
        let groups = self
            .order
            .iter()
            .filter_map(|name| {
                let live = self.envs.get(name)?;
                let roots = live.data.groups.iter().map(|group| group.root.clone());
                Some((name.clone(), roots.collect()))
            })
            .collect();
        let active_file = self.envs.get(&self.active).and_then(|live| {
            live.focused
                .and_then(|id| live.data.open_file_of(id))
                .map(|file| file.path.clone())
        });
        Seen {
            environments: self.order.clone(),
            groups,
            active_file,
        }
    }

    pub(in crate::environment_shell) fn sync_extension_events(&mut self, cx: &mut Context<Self>) {
        let now = self.seen_now();
        if now == self.extensions.watch.seen {
            return;
        }
        let before = std::mem::replace(&mut self.extensions.watch.seen, now.clone());
        for name in &now.environments {
            if before.environments.contains(name) {
                continue;
            }
            if let Some(environment) = self.environment_info(name) {
                self.broadcast(
                    Some(name),
                    &WorkspaceEvent::EnvironmentOpened { environment },
                );
            }
            self.activate_on_environment(name, cx);
        }
        for name in &before.environments {
            if now.environments.contains(name) {
                continue;
            }
            let environment = krad_extensions::api::protocol::EnvironmentInfo {
                id: name.as_str().to_owned(),
                name: name.as_str().to_owned(),
                host: krad_extensions::api::protocol::HostInfo::Local,
            };
            self.broadcast(None, &WorkspaceEvent::EnvironmentClosed { environment });
            self.stop_environment_programs(name, cx);
        }
        for (name, roots) in &now.groups {
            let unchanged = before
                .groups
                .iter()
                .any(|(known, known_roots)| known == name && known_roots == roots);
            if unchanged {
                continue;
            }
            let groups = self.group_infos(name);
            self.broadcast(Some(name), &WorkspaceEvent::GroupsChanged { groups });
            self.activate_on_workspace(name, roots, cx);
        }
        if before.active_file != now.active_file {
            let event = WorkspaceEvent::ActiveFileChanged {
                path: now.active_file.clone(),
            };
            let active = self.active.clone();
            self.broadcast(Some(&active), &event);
        }
    }

    fn group_infos(&self, name: &EnvironmentName) -> Vec<GroupInfo> {
        self.envs.get(name).map_or_else(Vec::new, |live| {
            live.data
                .groups
                .iter()
                .map(|group| GroupInfo {
                    id: group.id.as_u64().to_string(),
                    name: group.name.clone(),
                    root: group.root.clone(),
                })
                .collect()
        })
    }

    fn stop_environment_programs(&mut self, name: &EnvironmentName, cx: &mut Context<Self>) {
        let gone: Vec<SlotKey> = self
            .extensions
            .slots
            .keys()
            .filter(|key| key.environment.as_ref() == Some(name))
            .cloned()
            .collect();
        for key in gone {
            self.stop_program(&key, cx);
        }
    }
}
