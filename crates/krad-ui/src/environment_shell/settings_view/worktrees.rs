mod list;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, ElementId, Entity, InteractiveElement,
    IntoElement, ParentElement, StatefulInteractiveElement, Styled, Subscription, Window,
};
use krad_domain::{
    BaseFetch, BranchPrefix, EnvFiles, RemovalConfirm, WorktreeBase, WorktreeCleanup,
    WorktreeFolder, WorktreeSettings,
};
use std::path::PathBuf;

use super::super::dialog_parts::{segment, SegmentSize};
use super::sections::{card_field, pane_column, settings_section};
use super::{form_row, hint, panel_card, segmented_track, EnvironmentShell};
use crate::components::toggle_switch;
use crate::theme::{palette, SPACE_1};
use list::ListedStates;

const CLEANUP_TITLE: &str = "Cleanup";
const CLEANUP_HINT: &str = "What happens to a worktree once its work is done.";
const AUTO_REMOVE_LABEL: &str = "Remove merged worktrees";
const AUTO_REMOVE_HINT: &str = "Once an agent has finished, its worktree goes away when it has no uncommitted changes and its branch is merged into the base. The branch itself stays.";
const CONFIRM_LABEL: &str = "Confirm removal";
const CONFIRM_HINT: &str =
    "Ask before removing a worktree that has uncommitted changes or commits not merged yet.";
const CONFIRM_ASK_LABEL: &str = "When it has changes";
const CONFIRM_NEVER_LABEL: &str = "Never";
const NEW_TITLE: &str = "New worktrees";
const NEW_HINT: &str =
    "Applies to worktrees you, your agents and their workers create from now on.";
const FOLDER_LABEL: &str = "Worktree folder";
const FOLDER_PLACEHOLDER: &str = "Beside the repository, in <repository>.worktrees";
const FOLDER_HINT: &str = "Leave empty to keep worktrees beside the repository. A folder you set holds one subfolder per repository.";
const FOLDER_RELATIVE: &str = "Use a full path, such as /Users/you/worktrees.";
const PREFIX_LABEL: &str = "Branch prefix";
const PREFIX_PLACEHOLDER: &str = "No prefix";
const PREFIX_PREVIEW: &str = "New branches are named ";
const PREFIX_SAMPLE: &str = "fix-login";
const BASE_LABEL: &str = "Base branch";
const BASE_HINT: &str =
    "Start from the branch checked out in the repository, or from its remote's default branch.";
const FETCH_LABEL: &str = "Fetch first";
const FETCH_HINT: &str =
    "Fetch from the remote before creating the worktree, so the base is current.";
const ENV_LABEL: &str = "Copy env files";
const ENV_HINT: &str = "Copy the repository's untracked .env files into new worktrees. They stay readable only by you.";
const BASE_PREFIX: &str = "worktree-base";
const BTN_CONFIRM_ASK: &str = "worktree-confirm-ask";
const BTN_CONFIRM_NEVER: &str = "worktree-confirm-never";
const AUTO_REMOVE_SWITCH: &str = "worktree-auto-remove";
const FETCH_SWITCH: &str = "worktree-fetch";
const ENV_SWITCH: &str = "worktree-env-files";

pub(in crate::environment_shell) struct WorktreesUi {
    folder: Entity<InputState>,
    prefix: Entity<InputState>,
    folder_relative: bool,
    states: ListedStates,
    _edits: [Subscription; 2],
}

fn folder_from(text: &str) -> Option<WorktreeFolder> {
    let text = text.trim();
    if text.is_empty() {
        return Some(WorktreeFolder::BesideRepository);
    }
    let path = PathBuf::from(text);
    path.is_absolute().then_some(WorktreeFolder::Custom(path))
}

fn folder_text(folder: &WorktreeFolder) -> String {
    match folder {
        WorktreeFolder::BesideRepository => String::new(),
        WorktreeFolder::Custom(path) => path.display().to_string(),
    }
}

impl EnvironmentShell {
    pub(super) fn worktrees_ui(&self, window: &mut Window, cx: &mut Context<Self>) -> WorktreesUi {
        let saved = &self.settings.worktrees;
        let folder_value = folder_text(&saved.folder);
        let prefix_value = saved.branch_prefix.as_str().to_owned();
        let folder = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(FOLDER_PLACEHOLDER)
                .default_value(folder_value)
        });
        let prefix = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(PREFIX_PLACEHOLDER)
                .default_value(prefix_value)
        });
        let folder_edit =
            cx.subscribe(&folder, |this: &mut Self, field, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = field.read(cx).value().to_string();
                    this.edit_worktree_folder(&text, cx);
                }
            });
        let prefix_edit =
            cx.subscribe(&prefix, |this: &mut Self, field, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = field.read(cx).value().to_string();
                    this.edit_worktree_settings(
                        |settings| {
                            settings.branch_prefix = BranchPrefix::from(text.as_str());
                        },
                        cx,
                    );
                }
            });
        WorktreesUi {
            folder,
            prefix,
            folder_relative: false,
            states: ListedStates::default(),
            _edits: [folder_edit, prefix_edit],
        }
    }

    fn edit_worktree_folder(&mut self, text: &str, cx: &mut Context<Self>) {
        let folder = folder_from(text);
        if let Some(ui) = self.settings_ui.as_mut() {
            ui.worktrees.folder_relative = folder.is_none();
        }
        match folder {
            Some(folder) => self.edit_worktree_settings(|settings| settings.folder = folder, cx),
            None => cx.notify(),
        }
    }

    fn edit_worktree_settings(
        &mut self,
        edit: impl FnOnce(&mut WorktreeSettings),
        cx: &mut Context<Self>,
    ) {
        let before = self.settings.worktrees.clone();
        edit(&mut self.settings.worktrees);
        if self.settings.worktrees != before {
            self.save_settings(cx);
        }
        cx.notify();
    }

    pub(super) fn worktrees_pane(&self, ui: &WorktreesUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let cleanup = settings_section(
            CLEANUP_TITLE,
            Some(CLEANUP_HINT),
            [panel_card(
                vec![self.auto_remove_row(cx), self.confirm_row(cx)],
                pal,
            )],
            pal,
        );
        let created = settings_section(
            NEW_TITLE,
            Some(NEW_HINT),
            [panel_card(
                vec![
                    Self::folder_row(ui, cx),
                    self.prefix_row(ui, cx),
                    self.base_row(cx),
                    self.fetch_row(cx),
                    self.env_row(cx),
                ],
                pal,
            )],
            pal,
        );
        pane_column([cleanup, created, self.worktree_list_section(&ui.states, cx)])
    }

    fn worktree_switch_row(
        id: &'static str,
        (label, detail): (&str, &str),
        on: bool,
        flip: fn(&mut WorktreeSettings),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let control = div()
            .id(ElementId::Name(id.into()))
            .cursor_pointer()
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_worktree_settings(flip, cx);
            }))
            .child(toggle_switch(ElementId::Name(id.into()), on, pal))
            .into_any_element();
        form_row(label, Some(detail), control, pal)
    }

    fn auto_remove_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.worktrees.cleanup == WorktreeCleanup::RemoveWhenMerged;
        Self::worktree_switch_row(
            AUTO_REMOVE_SWITCH,
            (AUTO_REMOVE_LABEL, AUTO_REMOVE_HINT),
            on,
            |settings| {
                settings.cleanup = match settings.cleanup {
                    WorktreeCleanup::Keep => WorktreeCleanup::RemoveWhenMerged,
                    WorktreeCleanup::RemoveWhenMerged => WorktreeCleanup::Keep,
                };
            },
            cx,
        )
    }

    fn fetch_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.worktrees.fetch == BaseFetch::FetchFirst;
        Self::worktree_switch_row(
            FETCH_SWITCH,
            (FETCH_LABEL, FETCH_HINT),
            on,
            |settings| {
                settings.fetch = match settings.fetch {
                    BaseFetch::Off => BaseFetch::FetchFirst,
                    BaseFetch::FetchFirst => BaseFetch::Off,
                };
            },
            cx,
        )
    }

    fn env_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.worktrees.env_files == EnvFiles::Copy;
        Self::worktree_switch_row(
            ENV_SWITCH,
            (ENV_LABEL, ENV_HINT),
            on,
            |settings| {
                settings.env_files = match settings.env_files {
                    EnvFiles::Skip => EnvFiles::Copy,
                    EnvFiles::Copy => EnvFiles::Skip,
                };
            },
            cx,
        )
    }

    fn confirm_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.worktrees.confirm_removal;
        let option = |id: &'static str, label: &'static str, choice: RemovalConfirm| {
            segment(
                id,
                label,
                None,
                current == choice,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_worktree_settings(|settings| settings.confirm_removal = choice, cx);
            }))
        };
        let segmented = segmented_track(pal)
            .child(option(
                BTN_CONFIRM_ASK,
                CONFIRM_ASK_LABEL,
                RemovalConfirm::WhenChanged,
            ))
            .child(option(
                BTN_CONFIRM_NEVER,
                CONFIRM_NEVER_LABEL,
                RemovalConfirm::Never,
            ))
            .into_any_element();
        form_row(CONFIRM_LABEL, Some(CONFIRM_HINT), segmented, pal)
    }

    fn base_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.worktrees.base;
        let segments = WorktreeBase::ALL.map(|base| {
            segment(
                (BASE_PREFIX, base as u64),
                base.display_name(),
                None,
                current == base,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_worktree_settings(|settings| settings.base = base, cx);
            }))
        });
        let segmented = segmented_track(pal).children(segments).into_any_element();
        form_row(BASE_LABEL, Some(BASE_HINT), segmented, pal)
    }

    fn folder_row(ui: &WorktreesUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let note = if ui.folder_relative {
            FOLDER_RELATIVE
        } else {
            FOLDER_HINT
        };
        let field = div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .child(Input::new(&ui.folder))
            .child(hint(note, pal))
            .into_any_element();
        card_field(FOLDER_LABEL, field, pal)
    }

    fn prefix_row(&self, ui: &WorktreesUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let preview = format!(
            "{PREFIX_PREVIEW}{}",
            self.settings.worktrees.branch_prefix.branch(PREFIX_SAMPLE)
        );
        let field = div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .child(Input::new(&ui.prefix))
            .child(hint(&preview, pal))
            .into_any_element();
        card_field(PREFIX_LABEL, field, pal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_folder_keeps_worktrees_beside_the_repository_and_relative_ones_are_refused() {
        assert_eq!(folder_from("  "), Some(WorktreeFolder::BesideRepository));
        assert_eq!(folder_from("trees"), None);
        let custom = folder_from("/work/trees");
        assert_eq!(
            custom,
            Some(WorktreeFolder::Custom(PathBuf::from("/work/trees")))
        );
        assert_eq!(
            custom.map(|folder| folder_text(&folder)),
            Some("/work/trees".to_owned())
        );
    }
}
