use gpui_kit::Context;
use krad_domain::{AppEvent, ToastKind, WorktreeState};
use krad_protocol::{Answer, RepoAsk, WorktreeChecked, WorktreeProbe, WorktreeRemoval};

use super::super::super::super::repo_asks::done;
use super::super::super::EnvironmentShell;
use super::{Listed, Probe};

impl EnvironmentShell {
    pub(in crate::environment_shell) fn refresh_worktree_states(&mut self, cx: &mut Context<Self>) {
        let listed = self.listed_worktree_entries();
        let Some(ui) = self.settings_ui.as_mut() else {
            return;
        };
        for entry in &listed {
            ui.worktrees
                .states
                .probes
                .insert(entry.path.clone(), Probe::Checking);
        }
        let base = self.runtime.settings.worktrees.base;
        let worktrees = listed
            .into_iter()
            .map(|entry| WorktreeProbe {
                main: entry.main,
                path: entry.path,
                branch: entry.branch,
            })
            .collect();
        let checking = self.ask_active_repo(
            RepoAsk::WorktreeStates { base, worktrees },
            |answer| match answer {
                Answer::WorktreeStates { states } => Some(states),
                _ => None,
            },
            cx,
        );
        cx.spawn(async move |this, cx| {
            let checked = checking.await;
            this.update(cx, |shell, cx| {
                let checked = match checked {
                    Ok(checked) => checked,
                    Err(error) => return shell.toast_failure(&error, cx),
                };
                let Some(ui) = shell.settings_ui.as_mut() else {
                    return;
                };
                for WorktreeChecked { path, state } in checked {
                    let probe = state.map_or_else(
                        |error| {
                            tracing::debug!(%error, "worktree state unreadable");
                            Probe::Failed
                        },
                        Probe::Known,
                    );
                    ui.worktrees.states.probes.insert(path, probe);
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn remove_listed(
        &mut self,
        entry: Listed,
        probe: Option<&Probe>,
        cx: &mut Context<Self>,
    ) {
        if let Some(ui) = self.settings_ui.as_mut() {
            ui.worktrees.states.confirming = None;
        }
        let env = self.active.clone();
        let group = self.envs.get(&env).and_then(|live| {
            live.data
                .groups
                .iter()
                .find(|group| group.root == entry.path)
                .map(|group| group.id)
        });
        let clean = matches!(
            probe,
            Some(Probe::Known(
                WorktreeState::Fresh | WorktreeState::Merged | WorktreeState::Ahead(_)
            ))
        );
        let removal = if clean {
            WorktreeRemoval::Clean
        } else {
            WorktreeRemoval::Discard
        };
        let removing = self.ask_repo(
            &env,
            RepoAsk::RemoveWorktree {
                repo: entry.main,
                path: entry.path,
                removal,
            },
            done,
            cx,
        );
        cx.spawn(async move |this, cx| {
            let removed = removing.await;
            this.update(cx, |shell, cx| {
                match removed {
                    Ok(()) => {
                        shell.publish_event(&env, group, AppEvent::WorktreeRemoved);
                        if let Some(group) = group {
                            shell.remove_group_in(&env, group, cx);
                        }
                    }
                    Err(error) => shell.toast(ToastKind::Error, &error.to_string(), cx),
                }
                shell.refresh_git(cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }
}
