use gpui_kit::{Context, PathPromptOptions, SharedString, Window};
use krad_domain::{
    parse_wsl_path, AgentPreset, EnvironmentName, GroupEdit, GroupId, Host, TerminalGroup,
    TerminalId, ToastKind, WorktreeOrigin,
};
use krad_host::HostLink;
use krad_protocol::{AgentPlace, Command, HostCommand, LocalAgents};
use std::path::{Path, PathBuf};

use super::EnvironmentShell;

mod git_poll;

pub(super) use git_poll::{GitKey, GroupGit};

const SHELL_ONLY: &[AgentPreset] = &[AgentPreset::Shell];
const FOLDER_PROMPT: &str = "Choose the group's root folder";
const NO_GROUP_TEXT: &str = "Select a group first";
const HOST_NOT_READY_TEXT: &str = "Connect to the environment's host before adding a group";

impl EnvironmentShell {
    pub(super) fn installed_agents(&self) -> &[AgentPreset] {
        self.agents_for(
            &self.active,
            self.active_group().map(|group| group.root.as_path()),
        )
    }

    pub(super) fn offered_agents(&self) -> Vec<AgentPreset> {
        AgentPreset::offered(self.installed_agents())
    }

    pub(super) fn offered_default_harness(&self) -> AgentPreset {
        self.settings
            .default_harness
            .as_offered(self.installed_agents())
    }

    pub(super) fn host_agents_known(&self, env: &EnvironmentName) -> bool {
        self.envs.get(env).is_some_and(|live| {
            live.data.host.is_local()
                || (self.host_state(env).is_connected()
                    && self.host_agents.contains_key(&live.data.host))
        })
    }

    pub(super) fn agents_for(&self, env: &EnvironmentName, root: Option<&Path>) -> &[AgentPreset] {
        if let Some(host) = self
            .envs
            .get(env)
            .map(|live| &live.data.host)
            .filter(|host| !host.is_local())
        {
            return self.host_agents.get(host).map_or(SHELL_ONLY, Vec::as_slice);
        }
        root.and_then(parse_wsl_path)
            .and_then(|folder| self.wsl_agents.get(&folder.distro))
            .unwrap_or(&self.native_agents)
    }

    pub(in crate::environment_shell) fn detect_wsl_agents(&mut self, roots: &[PathBuf]) {
        let distros: Vec<String> = roots
            .iter()
            .filter_map(|root| parse_wsl_path(root))
            .map(|folder| folder.distro)
            .collect();
        for distro in distros {
            if !self.wsl_agents.contains_key(&distro) {
                self.wsl_agents
                    .insert(distro.clone(), vec![AgentPreset::Shell]);
                self.detect_agents_in(distro);
            }
        }
    }

    pub(super) fn detect_host_agents(&mut self, env: &EnvironmentName) {
        if self.env_link(env).is_local() {
            return;
        }
        self.runtime
            .handle
            .post(Command::Host(HostCommand::DetectAgents {
                environment: env.clone(),
            }));
    }

    pub(super) fn host_agents_found(
        &mut self,
        env: &EnvironmentName,
        (host, installed): (Host, Vec<AgentPreset>),
        cx: &mut Context<Self>,
    ) {
        self.host_agents.insert(host, installed);
        self.list_host_models(env);
        self.place_usage();
        self.load_missing_histories(env, cx);
        let harnesses: Vec<AgentPreset> =
            AgentPreset::listed(&self.settings.acp.all_agents()).collect();
        for harness in harnesses {
            self.flush_chats_of(harness, cx);
        }
        cx.notify();
    }

    pub(super) fn redetect_after_acp_change(&mut self) {
        let agents = self.settings.acp.all_agents();
        if agents == self.detected_acp {
            return;
        }
        self.detect_agents();
        let distros: Vec<String> = self.wsl_agents.keys().cloned().collect();
        for distro in distros {
            self.detect_agents_in(distro);
        }
        let remote: Vec<EnvironmentName> = self
            .envs
            .iter()
            .filter(|(_, live)| !live.data.host.is_local())
            .map(|(env, _)| env.clone())
            .collect();
        for env in remote {
            self.detect_host_agents(&env);
        }
        self.detected_acp = agents;
    }

    fn detect_agents_in(&self, distro: String) {
        let place = AgentPlace::Wsl { distro };
        self.runtime
            .handle
            .post(Command::Host(HostCommand::DetectLocalAgents { place }));
    }

    pub(super) fn local_agents_found(&mut self, found: LocalAgents, cx: &mut Context<Self>) {
        match found.place {
            AgentPlace::Wsl { distro } => {
                self.wsl_agents.insert(distro, found.agents);
            }
            AgentPlace::Native => {
                self.native_agents = found.agents;
                self.ensure_opencode();
                self.list_harness_models();
                self.place_usage();
            }
        }
        cx.notify();
    }

    pub(super) fn active_link(&self) -> HostLink {
        self.env_link(&self.active)
    }

    pub(super) fn env_link(&self, env: &EnvironmentName) -> HostLink {
        self.envs
            .get(env)
            .map_or_default(|live| self.runtime.handle.host_link(&live.data.host))
    }

    pub(super) fn active_group(&self) -> Option<&TerminalGroup> {
        let live = self.envs.get(&self.active)?;
        live.data.group(live.selected_group?)
    }

    pub(super) fn select_group(&mut self, id: GroupId, cx: &mut Context<Self>) {
        let env = self.active.clone();
        if let Some(live) = self.envs.get_mut(&env) {
            live.selected_group = Some(id);
        }
        self.refresh_skill_library(cx);
        cx.notify();
    }

    pub(super) fn open_worktree(
        &mut self,
        path: PathBuf,
        origin: Option<WorktreeOrigin>,
        cx: &mut Context<Self>,
    ) {
        let open = self.envs.get(&self.active).and_then(|live| {
            live.data
                .groups
                .iter()
                .find(|group| group.root == path)
                .map(|group| group.id)
        });
        match open {
            Some(id) => self.select_group(id, cx),
            None => self.add_group(path, origin, cx),
        }
    }

    pub(super) fn prompt_new_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.active_link().is_local() {
            if self.host_ready(&self.active) {
                self.open_folder_browser(window, cx);
            } else {
                self.toast(ToastKind::Error, HOST_NOT_READY_TEXT, cx);
            }
            return;
        }
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(FOLDER_PROMPT)),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = answer.await else {
                return;
            };
            let Some(root) = paths.into_iter().next() else {
                return;
            };
            this.update(cx, |shell, cx| shell.add_group(root, None, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn add_group(
        &mut self,
        root: PathBuf,
        worktree: Option<WorktreeOrigin>,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        self.add_group_to(&env, root, worktree, cx);
    }

    pub(super) fn add_group_to(
        &mut self,
        env: &EnvironmentName,
        root: PathBuf,
        worktree: Option<WorktreeOrigin>,
        cx: &mut Context<Self>,
    ) -> Option<GroupId> {
        let id = self.envs.get(env).map(|live| live.data.next_group_id());
        if let Some(group) = id {
            self.edit_environment(
                env,
                GroupEdit::Add {
                    group,
                    root,
                    worktree,
                },
            );
            if let Some(live) = self.envs.get_mut(env) {
                live.selected_group = Some(group);
            }
        }
        self.refresh_git(cx);
        cx.notify();
        self.store();
        id
    }

    pub(super) fn remove_selected_group(&mut self, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let Some(id) = live.selected_group.take() else {
            self.push_toast(ToastKind::Warning, NO_GROUP_TEXT.to_owned());
            cx.notify();
            return;
        };
        self.remove_group_in(&env, id, cx);
    }

    pub(super) fn remove_group_in(
        &mut self,
        env: &EnvironmentName,
        id: GroupId,
        cx: &mut Context<Self>,
    ) {
        let held: Vec<TerminalId> = self.envs.get(env).map_or_default(|live| {
            live.data
                .nodes
                .iter()
                .filter(|node| node.group == Some(id))
                .map(|node| node.id)
                .collect()
        });
        for terminal in held {
            self.close_terminal_in(env, terminal, cx);
        }
        let removed = self
            .envs
            .get(env)
            .and_then(|live| live.data.group(id))
            .cloned();
        self.edit_environment(env, GroupEdit::Remove { group: id });
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        if live.selected_group.is_none_or(|selected| selected == id) {
            live.selected_group = live.data.groups.first().map(|group| group.id);
        }
        let host = live.data.host.clone();
        if let Some(removed) = removed {
            self.forget_group_toolchains(&host, &removed.root);
        }
        self.forget_group_archive(env, id, cx);
        cx.notify();
        self.store();
    }

    pub(super) fn detect_agents(&self) {
        self.runtime
            .handle
            .post(Command::Host(HostCommand::DetectLocalAgents {
                place: AgentPlace::Native,
            }));
    }

    pub(super) fn group_roots(&self) -> (HostLink, Vec<PathBuf>) {
        self.envs
            .get(&self.active)
            .map_or_else(Default::default, |live| {
                (
                    self.runtime.handle.host_link(&live.data.host),
                    live.data
                        .groups
                        .iter()
                        .map(|group| group.root.clone())
                        .collect(),
                )
            })
    }

    pub(super) fn git_at(&self, env: &EnvironmentName, root: &Path) -> Option<&GroupGit> {
        let host = self.envs.get(env)?.data.host.clone();
        self.git.get(&(host, root.to_path_buf()))
    }

    pub(super) fn active_git(&self, root: &Path) -> Option<&GroupGit> {
        self.git_at(&self.active, root)
    }
}
