use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::{
    div, px, rgb, AnyElement, App, AppContext as _, ClickEvent, Context, Entity, FontWeight,
    InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    Subscription, Window,
};
use std::collections::HashSet;
use std::path::PathBuf;

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::EnvironmentShell;
use super::{CommitIntent, GitAction};
use crate::components::{icon, IconName};
use crate::keymap::COMMIT_HINT;
use crate::theme::{
    palette, ICON_SM, LINE_SM, RADIUS_SM, RADIUS_XS, SPACE_0_5, SPACE_1, SPACE_1_5, SPACE_2,
    TEXT_SM,
};

const MESSAGE_MAX_ROWS: usize = 6;
const COMMIT_LABEL: &str = "Commit";
const COMMIT_PUSH_LABEL: &str = "Commit & Push";
const AMEND_LABEL: &str = "Commit (Amend)";
const BTN_COMMIT: &str = "side-changes-commit";
const BTN_COMMIT_MORE: &str = "side-changes-commit-more";
const COMMIT_BUTTON_H: f32 = 28.0;
const MORE_BUTTON_W: f32 = 28.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::environment_shell) enum ChangeSide {
    Staged,
    Unstaged,
}

#[derive(Default)]
pub(in crate::environment_shell) struct GitUi {
    message: Option<(Entity<TextareaState>, Subscription)>,
    clearing: bool,
    pub(super) menu_origin: (f32, f32),
    pub(in crate::environment_shell) collapsed: HashSet<ChangeSide>,
    pub(in crate::environment_shell) expanded: HashSet<PathBuf>,
}

impl GitUi {
    pub(super) fn commit_message(&self, cx: &App) -> String {
        self.message
            .as_ref()
            .map_or_default(|(message, _)| message.read(cx).value().to_string())
    }

    pub(super) const fn clear_message(&mut self) {
        self.clearing = true;
    }
}

fn placeholder() -> String {
    format!("Message ({COMMIT_HINT} to commit)")
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn sync_commit_box(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.git_ui.message.is_none() {
            let message = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(1, MESSAGE_MAX_ROWS)
                    .placeholder(placeholder())
            });
            let events = cx.subscribe_in(
                &message,
                window,
                |this: &mut Self, _, event: &InputEvent, _, cx| {
                    if let InputEvent::PressEnter {
                        secondary: true, ..
                    } = event
                    {
                        this.commit_changes(CommitIntent::Commit, cx);
                    }
                },
            );
            self.git_ui.message = Some((message, events));
        }
        if std::mem::take(&mut self.git_ui.clearing) {
            if let Some((message, _)) = &self.git_ui.message {
                message.update(cx, |message, cx| message.set_value("", window, cx));
            }
        }
    }

    pub(in crate::environment_shell) fn commit_box(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let field = self.git_ui.message.as_ref().map(|(message, _)| {
            div()
                .px(px(SPACE_2))
                .py(px(SPACE_1))
                .rounded(px(RADIUS_SM))
                .border_1()
                .border_color(pal.divider)
                .bg(rgb(pal.bg_surface))
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .child(Textarea::new(message).appearance(false))
        });
        let commit = div()
            .id(BTN_COMMIT)
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(SPACE_1_5))
            .h(px(COMMIT_BUTTON_H))
            .rounded_l(px(RADIUS_XS))
            .bg(rgb(pal.accent))
            .hover(|style| style.bg(rgb(pal.accent_hover)))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(pal.on_accent))
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.commit_changes(CommitIntent::Commit, cx);
            }))
            .child(icon(IconName::Tick, ICON_SM, pal.on_accent))
            .child(COMMIT_LABEL);
        let more = div()
            .id(BTN_COMMIT_MORE)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .w(px(MORE_BUTTON_W))
            .h(px(COMMIT_BUTTON_H))
            .rounded_r(px(RADIUS_XS))
            .border_l_1()
            .border_color(rgb(pal.accent_hover))
            .bg(rgb(pal.accent))
            .hover(|style| style.bg(rgb(pal.accent_hover)))
            .cursor_pointer()
            .on_click(cx.listener(|this: &mut Self, event: &ClickEvent, _, cx| {
                let at = event.position();
                this.open_menu(
                    (at.x.as_f32(), at.y.as_f32()),
                    MenuPlacement::Below,
                    commit_choices(),
                    cx,
                );
            }))
            .child(icon(IconName::ChevronDown, ICON_SM, pal.on_accent));
        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(SPACE_1))
            .px(px(SPACE_1))
            .pb(px(SPACE_0_5))
            .children(field)
            .child(div().flex().flex_row().child(commit).child(more))
            .into_any_element()
    }
}

fn commit_choices() -> Vec<MenuEntry> {
    [
        (COMMIT_LABEL, CommitIntent::Commit),
        (COMMIT_PUSH_LABEL, CommitIntent::CommitAndPush),
        (AMEND_LABEL, CommitIntent::Amend),
    ]
    .into_iter()
    .map(|(label, intent)| MenuEntry::item(label, None, MenuAction::Git(GitAction::Commit(intent))))
    .collect()
}
