use gpui_kit::{App, Context, PathPromptOptions, SharedString, Window};
use krad_domain::{
    parse_wsl_path, AgentPreset, EnvironmentName, GroupEdit, GroupId, Host, TerminalGroup,
    TerminalId, ToastKind, WorktreeOrigin,
};
use krad_extensions::host::HostLink;
use krad_protocol::{AgentPlace, Command, HostCommand, LocalAgents};
use std::path::{Path, PathBuf};

use super::runtime_view::AgentsMirror;
use super::EnvironmentShell;

mod git_poll;

pub(super) use git_poll::{GitKey, GroupGit};

const FOLDER_PROMPT: &str = "Choose the group's root folder";
const NO_GROUP_TEXT: &str = "Select a group first";
const HOST_NOT_READY_TEXT: &str = "Connect to the environment's host before adding a group";

impl EnvironmentShell {
    pub(super) fn installed_agents<'a>(&self, cx: &'a App) -> &'a [AgentPreset] {
        self.agents_for(
            &self.active,
            self.active_group().map(|group| group.root.as_path()),
            cx,
        )
    }

    pub(super) fn offered_agents(&self, cx: &App) -> Vec<AgentPreset> {
        AgentPreset::offered(self.installed_agents(cx))
    }

    pub(super) fn offered_default_harness(&self, cx: &App) -> AgentPreset {
        self.settings
            .default_harness
            .as_offered(self.installed_agents(cx))
    }

    pub(super) fn host_agents_known(&self, env: &EnvironmentName, cx: &App) -> bool {
        self.envs.get(env).is_some_and(|live| {
            live.data.host.is_local()
                || (self.host_state(env, cx).is_connected()
                    && self.agents(cx).on_host(&live.data.host).is_some())
        })
    }

    pub(super) fn agents<'a>(&self, cx: &'a App) -> &'a AgentsMirror {
        &self.runtime.view.read(cx).agents
    }

    pub(super) fn agents_for<'a>(
        &self,
        env: &EnvironmentName,
        root: Option<&Path>,
        cx: &'a App,
    ) -> &'a [AgentPreset] {
        let agents = self.agents(cx);
        if let Some(host) = self
            .envs
            .get(env)
            .map(|live| &live.data.host)
            .filter(|host| !host.is_local())
        {
            return agents.on_host_or_shell(host);
        }
        root.and_then(parse_wsl_path)
            .and_then(|folder| agents.in_wsl(&folder.distro))
            .unwrap_or_else(|| agents.native())
    }

    pub(in crate::environment_shell) fn detect_wsl_agents(
        &mut self,
        roots: &[PathBuf],
        cx: &mut App,
    ) {
        let distros: Vec<String> = roots
            .iter()
            .filter_map(|root| parse_wsl_path(root))
            .map(|folder| folder.distro)
            .collect();
        for distro in distros {
            let asked = self
                .runtime
                .view
                .update(cx, |model, _| model.agents.ask_in_wsl(&distro));
            if asked {
                self.detect_agents_in(distro);
            }
        }
    }

    pub(super) fn detect_host_agents(&mut self, env: &EnvironmentName) {
        if self.env_is_local(env) {
            return;
        }
        self.runtime
            .client
            .post(Command::Host(HostCommand::DetectAgents {
                environment: env.clone(),
            }));
    }

    pub(super) fn host_agents_found(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        self.list_host_models(env, cx);
        self.place_usage();
        self.load_missing_histories(env, cx);
        let harnesses: Vec<AgentPreset> =
            AgentPreset::listed(&self.settings.acp.all_agents()).collect();
        for harness in harnesses {
            self.flush_chats_of(harness, cx);
        }
        cx.notify();
    }

    pub(super) fn redetect_after_acp_change(&mut self, cx: &mut App) {
        let agents = self.settings.acp.all_agents();
        let changed = self
            .runtime
            .view
            .update(cx, |model, _| model.agents.acp_asked(agents));
        if !changed {
            return;
        }
        self.detect_agents();
        let distros = self.agents(cx).wsl_distros();
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
    }

    fn detect_agents_in(&self, distro: String) {
        let place = AgentPlace::Wsl { distro };
        self.runtime
            .client
            .post(Command::Host(HostCommand::DetectLocalAgents { place }));
    }

    pub(super) fn local_agents_found(&mut self, found: &LocalAgents, cx: &mut Context<Self>) {
        if found.place == AgentPlace::Native {
            self.ensure_opencode(cx);
            self.list_harness_models(cx);
            self.place_usage();
        }
        cx.notify();
    }

    pub(super) fn env_host(&self, env: &EnvironmentName) -> Host {
        self.envs
            .get(env)
            .map_or_default(|live| live.data.host.clone())
    }

    pub(super) fn env_on_host(&self, host: &Host) -> Option<EnvironmentName> {
        self.envs
            .iter()
            .find(|(_, live)| live.data.host == *host)
            .map(|(name, _)| name.clone())
    }

    pub(super) fn active_host(&self) -> Host {
        self.env_host(&self.active)
    }

    pub(super) fn env_is_local(&self, env: &EnvironmentName) -> bool {
        self.envs
            .get(env)
            .is_none_or(|live| live.data.host.is_local())
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
        if !self.env_is_local(&self.active) {
            if self.host_ready(&self.active, cx) {
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
        self.forget_group_archive(env, id);
        cx.notify();
        self.store();
    }

    pub(super) fn detect_agents(&self) {
        self.runtime
            .client
            .post(Command::Host(HostCommand::DetectLocalAgents {
                place: AgentPlace::Native,
            }));
    }

    pub(super) fn group_roots(&self) -> Vec<PathBuf> {
        self.envs.get(&self.active).map_or_default(|live| {
            live.data
                .groups
                .iter()
                .map(|group| group.root.clone())
                .collect()
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
