use gpui_kit::{
    div, px, ClipboardItem, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::FileOpening;
use krad_repo::{ChangeKind, FileChange};
use std::path::PathBuf;

use super::super::super::context_menu::{MenuAction, MenuEntry};
use super::super::super::discard::Discard;
use super::super::super::git_actions::{ChangeSide, GitAction};
use super::super::super::EnvironmentShell;
use super::row_key;
use crate::components::{icon, tooltip_view, IconName};
use crate::theme::{palette, ICON_SM, ICON_XS, RADIUS_XS};

pub(super) const ROW_GROUP: &str = "side-change-row-group";
pub(super) const SECTION_GROUP: &str = "side-change-section-group";
const BTN_DISCARD_PREFIX: &str = "side-change-discard";
const BTN_STAGE_PREFIX: &str = "side-change-stage";
const BTN_STAGE_ALL_PREFIX: &str = "side-change-stage-all";
const STAGE_LABEL: &str = "Stage changes";
const UNSTAGE_LABEL: &str = "Unstage changes";
const STAGE_ALL_LABEL: &str = "Stage all changes";
const UNSTAGE_ALL_LABEL: &str = "Unstage all changes";
const OPEN_LABEL: &str = "Open changes";
const DISCARD_LABEL: &str = "Discard changes";
const DELETE_LABEL: &str = "Delete file";
const COPY_PATH_LABEL: &str = "Copy path";

#[derive(Clone, Debug, PartialEq)]
pub(in crate::environment_shell) enum ChangesAction {
    Open(PathBuf),
    Discard { repo: PathBuf, change: FileChange },
    Copy(String),
}

fn discard_label(change: &FileChange) -> &'static str {
    if change.kind == ChangeKind::Untracked {
        DELETE_LABEL
    } else {
        DISCARD_LABEL
    }
}

pub(super) fn change_menu(repo: &std::path::Path, change: &FileChange) -> Vec<MenuEntry> {
    let discard = ChangesAction::Discard {
        repo: repo.to_path_buf(),
        change: change.clone(),
    };
    vec![
        MenuEntry::item(
            OPEN_LABEL,
            Some(IconName::File),
            MenuAction::Changes(ChangesAction::Open(repo.join(&change.path))),
        ),
        MenuEntry::item(
            COPY_PATH_LABEL,
            Some(IconName::Copy),
            MenuAction::Changes(ChangesAction::Copy(change.path.display().to_string())),
        ),
        MenuEntry::Separator,
        MenuEntry::destructive(
            discard_label(change),
            IconName::Undo,
            MenuAction::Changes(discard),
        ),
    ]
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn run_changes_action(
        &mut self,
        action: ChangesAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            ChangesAction::Open(path) => self.open_changes_tab(&path, FileOpening::Pinned, cx),
            ChangesAction::Discard { repo, change } => {
                self.ask_discard(Discard::File { repo, change }, window, cx);
            }
            ChangesAction::Copy(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
        }
    }

    pub(super) fn discard_button(
        repo: PathBuf,
        change: &FileChange,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let pal = palette(cx);
        let tooltip = discard_label(change);
        let discarded = change.clone();
        div()
            .id((
                BTN_DISCARD_PREFIX,
                row_key(ChangeSide::Unstaged, &change.path),
            ))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(px(ICON_SM))
            .rounded(px(RADIUS_XS))
            .invisible()
            .group_hover(ROW_GROUP, Styled::visible)
            .hover(|style| style.bg(pal.bg_selected))
            .tooltip(move |_, cx| tooltip_view(tooltip.into(), cx))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                cx.stop_propagation();
                this.ask_discard(
                    Discard::File {
                        repo: repo.clone(),
                        change: discarded.clone(),
                    },
                    window,
                    cx,
                );
            }))
            .child(icon(IconName::Undo, ICON_XS, pal.text_muted))
    }
}

pub(super) fn side_button(
    group: &'static str,
    side: ChangeSide,
    path: Option<PathBuf>,
    cx: &mut gpui_kit::Context<EnvironmentShell>,
) -> impl IntoElement {
    let pal = palette(cx);
    let (glyph, tooltip) = match (side, path.is_some()) {
        (ChangeSide::Unstaged, true) => (IconName::Add, STAGE_LABEL),
        (ChangeSide::Staged, true) => (IconName::Minus, UNSTAGE_LABEL),
        (ChangeSide::Unstaged, false) => (IconName::Add, STAGE_ALL_LABEL),
        (ChangeSide::Staged, false) => (IconName::Minus, UNSTAGE_ALL_LABEL),
    };
    let id = match &path {
        Some(path) => (BTN_STAGE_PREFIX, row_key(side, path)),
        None => (
            BTN_STAGE_ALL_PREFIX,
            row_key(side, std::path::Path::new("")),
        ),
    };
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(ICON_SM))
        .rounded(px(RADIUS_XS))
        .invisible()
        .group_hover(group, Styled::visible)
        .hover(|style| style.bg(pal.bg_selected))
        .tooltip(move |_, cx| tooltip_view(tooltip.into(), cx))
        .on_click(
            cx.listener(move |this: &mut EnvironmentShell, _, window, cx| {
                cx.stop_propagation();
                match (side, path.clone()) {
                    (ChangeSide::Unstaged, Some(path)) => this.stage_paths(vec![path], cx),
                    (ChangeSide::Staged, Some(path)) => this.unstage_paths(vec![path], cx),
                    (ChangeSide::Unstaged, None) => {
                        this.run_git_action(GitAction::StageAll, window, cx);
                    }
                    (ChangeSide::Staged, None) => {
                        this.run_git_action(GitAction::UnstageAll, window, cx);
                    }
                }
            }),
        )
        .child(icon(glyph, ICON_XS, pal.text_muted))
}
