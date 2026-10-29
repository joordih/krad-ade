use gpui_kit::{AppContext as _, Context, PathPromptOptions, SharedString};
use krad_domain::{
    parse_wsl_path, AgentPreset, EnvironmentName, GroupId, TerminalGroup, TerminalId,
    WorktreeOrigin,
};
use krad_repo::{line_counts, repo_root, status, wsl_programs, GitStatus, LineCount};
use krad_session::find_program;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::EnvironmentShell;

const GIT_POLL: Duration = Duration::from_secs(3);
const FOLDER_PROMPT: &str = "Choose the group's root folder";
const NO_GROUP_TEXT: &str = "Select a group first";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GroupGit {
    pub(super) repo_root: PathBuf,
    pub(super) status: GitStatus,
    pub(super) lines: HashMap<PathBuf, LineCount>,
}

impl GroupGit {
    pub(super) fn total_lines(&self) -> LineCount {
        self.lines
            .values()
            .copied()
            .fold(LineCount::default(), |total, count| total + count)
    }
}

fn read_git(roots: Vec<PathBuf>) -> HashMap<PathBuf, GroupGit> {
    roots
        .into_iter()
        .filter_map(|root| {
            let repo_root = repo_root(&root).ok().flatten()?;
            let status = status(&root).ok()?;
            let lines = line_counts(&repo_root).unwrap_or_default();
            Some((
                root,
                GroupGit {
                    repo_root,
                    status,
                    lines,
                },
            ))
        })
        .collect()
}

fn presets_found(is_installed: impl Fn(&str) -> bool) -> Vec<AgentPreset> {
    AgentPreset::all()
        .into_iter()
        .filter(|preset| preset.program().is_none_or(&is_installed))
        .collect()
}

fn presets_in_wsl(distro: &str) -> Vec<AgentPreset> {
    let programs: Vec<&str> = AgentPreset::all()
        .into_iter()
        .filter_map(AgentPreset::program)
        .collect();
    let found = wsl_programs(distro, &programs).unwrap_or_default();
    presets_found(|program| found.contains(&program))
}

impl EnvironmentShell {
    pub(super) fn installed_agents(&self) -> &[AgentPreset] {
        self.agents_for(self.active_group().map(|group| group.root.as_path()))
    }

    pub(super) fn agents_for(&self, root: Option<&Path>) -> &[AgentPreset] {
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

    pub(super) fn prompt_new_group(cx: &mut Context<Self>) {
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
        live.data.remove_group(id);
        live.selected_group = live.data.groups.first().map(|group| group.id);
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
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn start_git_watch(&mut self, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| loop {
            let Ok(roots) = this.update(cx, |shell, cx| {
                let roots = shell.group_roots();
                shell.detect_wsl_agents(&roots, cx);
                roots
            }) else {
                break;
            };
            let git = cx
                .background_executor()
                .spawn(async move { read_git(roots) })
                .await;
            if this
                .update(cx, |shell, cx| shell.apply_git(git, cx))
                .is_err()
            {
                break;
            }
            cx.background_executor().timer(GIT_POLL).await;
        });
        self.git_task = Some(task);
    }

    pub(super) fn refresh_git(&mut self, cx: &mut Context<Self>) {
        let roots = self.group_roots();
        let read = cx.background_spawn(async move { read_git(roots) });
        cx.spawn(async move |this, cx| {
            let git = read.await;
            this.update(cx, |shell, cx| shell.apply_git(git, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn group_roots(&self) -> Vec<PathBuf> {
        self.envs.get(&self.active).map_or_else(Vec::new, |live| {
            live.data
                .groups
                .iter()
                .map(|group| group.root.clone())
                .collect()
        })
    }

    fn apply_git(&mut self, git: HashMap<PathBuf, GroupGit>, cx: &mut Context<Self>) {
        if self.git != git {
            self.git = git;
            self.forget_diffs();
            self.reload_file_tree(cx);
            cx.notify();
        }
    }
}
