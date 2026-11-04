use gpui_kit::{
    div, px, ClipboardItem, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::FileOpening;
use krad_repo::{ChangeKind, FileChange};
use std::path::PathBuf;

use super::super::super::context_menu::{MenuAction, MenuEntry};
use super::super::super::discard::Discard;
use super::super::super::EnvironmentShell;
use super::row_key;
use crate::components::{icon, tooltip_view, IconName};
use crate::theme::{palette, ICON_SM, ICON_XS, RADIUS_XS};

pub(super) const ROW_GROUP: &str = "side-change-row-group";
const BTN_DISCARD_PREFIX: &str = "side-change-discard";
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
            .id((BTN_DISCARD_PREFIX, row_key(&change.path)))
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
