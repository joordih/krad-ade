use gpui_kit::{AppContext as _, Context, PathPromptOptions, SharedString, Window};
use krad_domain::{
    parse_wsl_path, AgentPreset, EnvironmentName, GroupId, TerminalGroup, TerminalId,
    WorktreeOrigin,
};
use krad_host::HostLink;
use krad_repo::{host_programs, wsl_programs};
use krad_session::find_program;
use std::path::{Path, PathBuf};

use super::EnvironmentShell;

mod git_poll;

pub(super) use git_poll::{GitKey, GroupGit};

const SHELL_ONLY: &[AgentPreset] = &[AgentPreset::Shell];
const FOLDER_PROMPT: &str = "Choose the group's root folder";
const NO_GROUP_TEXT: &str = "Select a group first";
const HOST_NOT_READY_TEXT: &str = "Connect to the environment's host before adding a group";

fn presets_found(is_installed: impl Fn(&str) -> bool) -> Vec<AgentPreset> {
    AgentPreset::all()
        .into_iter()
        .filter(|preset| preset.program().is_none_or(&is_installed))
        .collect()
}

fn agent_programs() -> Vec<&'static str> {
    AgentPreset::all()
        .into_iter()
        .filter_map(AgentPreset::program)
        .collect()
}

fn presets_on_host(link: &HostLink) -> Vec<AgentPreset> {
    let found = host_programs(link, &agent_programs()).unwrap_or_default();
    presets_found(|program| found.contains(&program))
}

fn presets_in_wsl(distro: &str) -> Vec<AgentPreset> {
    let found = wsl_programs(distro, &agent_programs()).unwrap_or_default();
    presets_found(|program| found.contains(&program))
}

impl EnvironmentShell {
    pub(super) fn installed_agents(&self) -> &[AgentPreset] {
        self.agents_for(
            &self.active,
            self.active_group().map(|group| group.root.as_path()),
        )
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

    fn detect_wsl_agents(&mut self, roots: &[PathBuf], cx: &mut Context<Self>) {
        let distros: Vec<String> = roots
            .iter()
            .filter_map(|root| parse_wsl_path(root))
            .map(|folder| folder.distro)
            .collect();
        for distro in distros {
            if !self.wsl_agents.contains_key(&distro) {
                self.wsl_agents
                    .insert(distro.clone(), vec![AgentPreset::Shell]);
                Self::detect_agents_in(distro, cx);
            }
        }
    }

    pub(super) fn detect_host_agents(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        let link = self.env_link(env);
        if link.is_local() {
            return;
        }
        let host = link.host();
        let env = env.clone();
        let lookup = cx.background_spawn(async move { presets_on_host(&link) });
        cx.spawn(async move |this, cx| {
            let installed = lookup.await;
            this.update(cx, |shell, cx| {
                shell.host_agents.insert(host, installed);
                shell.list_host_models(&env, cx);
                for harness in AgentPreset::all() {
                    shell.flush_chats_of(harness, cx);
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn detect_agents_in(distro: String, cx: &mut Context<Self>) {
        let lookup = cx.background_spawn({
            let distro = distro.clone();
            async move { presets_in_wsl(&distro) }
        });
        cx.spawn(async move |this, cx| {
            let installed = lookup.await;
            this.update(cx, |shell, cx| {
                shell.wsl_agents.insert(distro, installed);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn active_link(&self) -> HostLink {
        self.env_link(&self.active)
    }

    pub(super) fn env_link(&self, env: &EnvironmentName) -> HostLink {
        self.envs.get(env).map_or_default(|live| live.link.clone())
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
                self.toast(HOST_NOT_READY_TEXT, cx);
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
        let id = self.envs.get_mut(env).map(|live| {
            let id = live.data.add_group(root, worktree);
            live.selected_group = Some(id);
            id
        });
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
            self.push_toast(NO_GROUP_TEXT.to_owned());
            cx.notify();
            return;
        };
        let held: Vec<TerminalId> = live
            .data
            .nodes
            .iter()
            .filter(|node| node.group == Some(id))
            .map(|node| node.id)
            .collect();
        for terminal in held {
            self.close_terminal(terminal, cx);
        }
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let removed = live.data.remove_group(id);
        live.selected_group = live.data.groups.first().map(|group| group.id);
        let host = live.data.host.clone();
        if let Some(removed) = removed {
            self.forget_group_toolchains(&host, &removed.root);
        }
        cx.notify();
        self.store();
    }

    pub(super) fn detect_agents(cx: &mut Context<Self>) {
        let lookup =
            cx.background_spawn(
                async move { presets_found(|program| find_program(program).is_some()) },
            );
        cx.spawn(async move |this, cx| {
            let installed = lookup.await;
            this.update(cx, |shell, cx| {
                shell.native_agents = installed;
                shell.ensure_opencode(cx);
                shell.list_harness_models(cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn group_roots(&self) -> (HostLink, Vec<PathBuf>) {
        self.envs
            .get(&self.active)
            .map_or_else(Default::default, |live| {
                (
                    live.link.clone(),
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
