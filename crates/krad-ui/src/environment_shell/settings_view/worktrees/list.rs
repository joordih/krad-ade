use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, FontWeight, IntoElement, ParentElement,
    SharedString, Styled, Window,
};
use krad_domain::{AppEvent, RemovalConfirm, ToastKind, WorktreeState};
use krad_repo::{discard_worktree, remove_worktree, worktree_state, RepoError};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use super::super::super::dialog_parts::sheet_padded;
use super::super::super::native_alert::Confirmation;
use super::super::sections::{empty_row, settings_section};
use super::super::{panel_card, EnvironmentShell};
use crate::components::{chip, icon, Button, ButtonVariant, IconName};
use crate::theme::{
    palette, Palette, ICON_SM, LINE_SM, LINE_XS, SPACE_0_5, SPACE_2, SPACE_3, TEXT_SM, TEXT_XS,
};

const LIST_TITLE: &str = "Existing worktrees";
const LIST_HINT: &str = "The worktrees of the repositories open in this environment.";
const EMPTY_TEXT: &str = "No worktrees besides the repositories themselves.";
const CHECKING_TEXT: &str = "Checking…";
const UNKNOWN_TEXT: &str = "Unknown";
const DETACHED_TEXT: &str = "Detached";
const REMOVE_LABEL: &str = "Remove";
const REMOVE_ANYWAY_LABEL: &str = "Remove anyway";
const CANCEL_LABEL: &str = "Cancel";
const REFRESH_LABEL: &str = "Check again";
const CONFIRM_TITLE: &str = "Remove this worktree?";
const CONFIRM_DIRTY: &str =
    "It has uncommitted changes. Removing the worktree deletes them for good.";
const CONFIRM_AHEAD: &str =
    "Its branch has commits that are not merged yet. The branch stays, but the folder goes away.";
const BRANCH_SEPARATOR: &str = " · ";
const BTN_REFRESH: &str = "worktrees-refresh";
const BTN_REMOVE: &str = "worktree-remove";
const BTN_ANYWAY: &str = "worktree-remove-anyway";
const BTN_CANCEL: &str = "worktree-remove-cancel";

#[derive(Clone, Debug, PartialEq, Eq)]
struct Listed {
    main: PathBuf,
    path: PathBuf,
    branch: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Probe {
    Checking,
    Known(WorktreeState),
    Failed,
}

#[derive(Default)]
pub(in crate::environment_shell) struct ListedStates {
    probes: HashMap<PathBuf, Probe>,
    confirming: Option<PathBuf>,
}

fn probe_label(probe: Option<&Probe>) -> &'static str {
    match probe {
        None | Some(Probe::Checking) => CHECKING_TEXT,
        Some(Probe::Known(state)) => state.label(),
        Some(Probe::Failed) => UNKNOWN_TEXT,
    }
}

fn needs_confirming(confirm: RemovalConfirm, probe: Option<&Probe>) -> bool {
    confirm == RemovalConfirm::WhenChanged
        && !matches!(
            probe,
            Some(Probe::Known(WorktreeState::Fresh | WorktreeState::Merged))
        )
}

fn confirm_detail(probe: Option<&Probe>) -> &'static str {
    match probe {
        Some(Probe::Known(WorktreeState::Ahead(_))) => CONFIRM_AHEAD,
        _ => CONFIRM_DIRTY,
    }
}

impl EnvironmentShell {
    fn listed_worktree_entries(&self) -> Vec<Listed> {
        let Some(host) = self
            .envs
            .get(&self.active)
            .map(|live| live.data.host.clone())
        else {
            return Vec::new();
        };
        let mut listed: BTreeMap<PathBuf, Listed> = BTreeMap::new();
        for ((on, _), git) in &self.git {
            if *on != host {
                continue;
            }
            let mut worktrees = git.worktrees.iter();
            let Some(main) = worktrees.next() else {
                continue;
            };
            for worktree in worktrees {
                listed
                    .entry(worktree.path.clone())
                    .or_insert_with(|| Listed {
                        main: main.path.clone(),
                        path: worktree.path.clone(),
                        branch: worktree.branch.clone(),
                    });
            }
        }
        listed.into_values().collect()
    }

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
        let link = self.active_link();
        let base = self.settings.worktrees.base;
        let checking = cx.background_spawn(async move {
            listed
                .into_iter()
                .map(|entry| {
                    let state = worktree_state(
                        &link,
                        &entry.main,
                        &entry.path,
                        entry.branch.as_deref(),
                        base,
                    );
                    (entry.path, state)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let checked = checking.await;
            this.update(cx, |shell, cx| {
                let Some(ui) = shell.settings_ui.as_mut() else {
                    return;
                };
                for (path, state) in checked {
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

    fn ask_remove_listed(&mut self, entry: Listed, window: &mut Window, cx: &mut Context<Self>) {
        let probe = self
            .settings_ui
            .as_ref()
            .and_then(|ui| ui.worktrees.states.probes.get(&entry.path).cloned());
        if !needs_confirming(self.settings.worktrees.confirm_removal, probe.as_ref()) {
            self.remove_listed(entry, probe.as_ref(), cx);
            return;
        }
        let confirmation = Confirmation {
            title: CONFIRM_TITLE,
            detail: confirm_detail(probe.as_ref()),
            confirm: REMOVE_ANYWAY_LABEL,
            cancel: CANCEL_LABEL,
        };
        let asked = Self::confirm_natively(&confirmation, window, cx, {
            let entry = entry.clone();
            let probe = probe.clone();
            move |this, confirmed, _, cx| {
                if confirmed {
                    this.remove_listed(entry, probe.as_ref(), cx);
                }
            }
        });
        if !asked {
            if let Some(ui) = self.settings_ui.as_mut() {
                ui.worktrees.states.confirming = Some(entry.path);
            }
            cx.notify();
        }
    }

    fn cancel_remove_listed(&mut self, cx: &mut Context<Self>) {
        if let Some(ui) = self.settings_ui.as_mut() {
            ui.worktrees.states.confirming = None;
        }
        cx.notify();
    }

    #[tracing::instrument(skip_all)]
    fn remove_listed(&mut self, entry: Listed, probe: Option<&Probe>, cx: &mut Context<Self>) {
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
        let link = self.env_link(&env);
        let clean = matches!(
            probe,
            Some(Probe::Known(
                WorktreeState::Fresh | WorktreeState::Merged | WorktreeState::Ahead(_)
            ))
        );
        let removing = cx.background_spawn(async move {
            let removed: Result<(), RepoError> = if clean {
                remove_worktree(&link, &entry.main, &entry.path)
            } else {
                discard_worktree(&link, &entry.main, &entry.path)
            };
            removed
        });
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

    pub(super) fn worktree_list_section(
        &self,
        states: &ListedStates,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let listed = self.listed_worktree_entries();
        let rows: Vec<AnyElement> = if listed.is_empty() {
            vec![empty_row(EMPTY_TEXT, pal)]
        } else {
            listed
                .into_iter()
                .enumerate()
                .map(|(index, entry)| Self::listed_row(index, &entry, states, cx))
                .collect()
        };
        let refresh = Button::new(BTN_REFRESH)
            .label(REFRESH_LABEL)
            .icon(IconName::Refresh)
            .small()
            .on_click(cx.listener(|this: &mut Self, _, _, cx| this.refresh_worktree_states(cx)));
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .child(settings_section(
                LIST_TITLE,
                Some(LIST_HINT),
                [panel_card(rows, pal)],
                pal,
            ))
            .child(div().flex().flex_row().justify_end().child(refresh))
            .into_any_element()
    }

    fn listed_actions(
        index: usize,
        entry: &Listed,
        probe: Option<&Probe>,
        confirming: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if confirming {
            vec![
                Button::new((BTN_CANCEL, index))
                    .label(CANCEL_LABEL)
                    .small()
                    .on_click(
                        cx.listener(|this: &mut Self, _, _, cx| this.cancel_remove_listed(cx)),
                    )
                    .into_any_element(),
                Button::new((BTN_ANYWAY, index))
                    .label(REMOVE_ANYWAY_LABEL)
                    .small()
                    .variant(ButtonVariant::Destructive)
                    .on_click(cx.listener({
                        let entry = entry.clone();
                        let probe = probe.cloned();
                        move |this: &mut Self, _, _, cx| {
                            this.remove_listed(entry.clone(), probe.as_ref(), cx);
                        }
                    }))
                    .into_any_element(),
            ]
        } else {
            vec![Button::new((BTN_REMOVE, index))
                .label(REMOVE_LABEL)
                .small()
                .variant(ButtonVariant::Secondary)
                .on_click(cx.listener({
                    let entry = entry.clone();
                    move |this: &mut Self, _, window, cx| {
                        this.ask_remove_listed(entry.clone(), window, cx);
                    }
                }))
                .into_any_element()]
        }
    }

    fn listed_row(
        index: usize,
        entry: &Listed,
        states: &ListedStates,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let probe = states.probes.get(&entry.path);
        let confirming = states.confirming.as_deref() == Some(entry.path.as_path());
        let name = folder_name(&entry.path);
        let detail = format!(
            "{}{BRANCH_SEPARATOR}{}",
            entry.branch.as_deref().unwrap_or(DETACHED_TEXT),
            entry.path.display()
        );
        let actions = Self::listed_actions(index, entry, probe, confirming, cx);
        sheet_padded(
            div()
                .flex()
                .flex_col()
                .gap(px(SPACE_2))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_3))
                        .child(icon(IconName::GitBranch, ICON_SM, pal.text_muted))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .gap(px(SPACE_0_5))
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(SPACE_2))
                                        .text_size(px(TEXT_SM))
                                        .line_height(px(LINE_SM))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(pal.text_strong))
                                        .child(name)
                                        .child(state_chip(probe, pal)),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(TEXT_XS))
                                        .line_height(px(LINE_XS))
                                        .text_color(rgb(pal.text_muted))
                                        .child(detail),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_none()
                                .flex_row()
                                .gap(px(SPACE_2))
                                .children(actions),
                        ),
                )
                .when(confirming, |row| {
                    row.child(
                        div()
                            .text_size(px(TEXT_XS))
                            .line_height(px(LINE_XS))
                            .text_color(rgb(pal.error_fg))
                            .child(confirm_detail(probe)),
                    )
                }),
        )
        .into_any_element()
    }
}

fn folder_name(path: &Path) -> SharedString {
    path.file_name()
        .map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
        .into()
}

fn state_chip(probe: Option<&Probe>, pal: &Palette) -> AnyElement {
    let color = match probe {
        Some(Probe::Known(WorktreeState::Merged)) => pal.success_fg,
        Some(Probe::Known(WorktreeState::Dirty)) => pal.error_fg,
        _ => pal.text_muted,
    };
    chip(probe_label(probe), pal)
        .text_color(rgb(color))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_clean_worktrees_skip_the_confirmation() {
        let ask = RemovalConfirm::WhenChanged;
        let known = |state| Some(Probe::Known(state));
        assert!(!needs_confirming(
            ask,
            known(WorktreeState::Merged).as_ref()
        ));
        assert!(!needs_confirming(ask, known(WorktreeState::Fresh).as_ref()));
        assert!(needs_confirming(ask, known(WorktreeState::Dirty).as_ref()));
        assert!(needs_confirming(
            ask,
            known(WorktreeState::Ahead(2)).as_ref()
        ));
        assert!(needs_confirming(ask, Some(&Probe::Checking)));
        assert!(needs_confirming(ask, None));
        assert!(!needs_confirming(
            RemovalConfirm::Never,
            known(WorktreeState::Dirty).as_ref()
        ));
    }
}
