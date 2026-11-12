use gpui_kit::component::input::EditorState;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled,
    Window,
};
use krad_lsp::lsp_types::{CodeAction, CodeActionKind};

use krad_domain::ToastKind;

use super::super::language_servers::Document;
use super::super::EnvironmentShell;
use super::{place, placed, popover_panel, EditorPopover};
use crate::components::{icon, IconName};
use crate::theme::{
    palette, Palette, ICON_SM, LINE_SM, LINE_XS, SPACE_1, SPACE_2, TEXT_SM, TEXT_XS,
};

const MENU_W: f32 = 380.0;
const MENU_MAX_H: f32 = 340.0;
const ROW_H: f32 = 30.0;
const ROW_DETAIL_H: f32 = 44.0;
const HEADER_H: f32 = 24.0;
const ROW_RADIUS: f32 = 8.0;
const MENU_ID: &str = "editor-code-actions";
const ROW_PREFIX: &str = "editor-code-action";
const KEY_UP: &str = "up";
const KEY_DOWN: &str = "down";
const KEY_ENTER: &str = "enter";
const KEY_ESCAPE: &str = "escape";
const PREFERRED_LABEL: &str = "Preferred";
const NO_ACTIONS_TEXT: &str = "No code actions here";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ActionGroup {
    QuickFix,
    Refactor,
    Source,
    Other,
}

impl ActionGroup {
    fn of(kind: Option<&CodeActionKind>) -> Self {
        let Some(kind) = kind.map(CodeActionKind::as_str) else {
            return Self::QuickFix;
        };
        let under = |parent: &CodeActionKind| {
            kind == parent.as_str() || kind.starts_with(&format!("{}.", parent.as_str()))
        };
        if under(&CodeActionKind::QUICKFIX) {
            Self::QuickFix
        } else if under(&CodeActionKind::REFACTOR) {
            Self::Refactor
        } else if under(&CodeActionKind::SOURCE) {
            Self::Source
        } else {
            Self::Other
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::QuickFix => "Quick fixes",
            Self::Refactor => "Refactor",
            Self::Source => "Source",
            Self::Other => "Other",
        }
    }

    const fn glyph(self) -> IconName {
        match self {
            Self::QuickFix => IconName::Flash,
            Self::Refactor => IconName::Magic,
            Self::Source => IconName::Layers,
            Self::Other => IconName::Workflow,
        }
    }
}

pub(super) fn ordered(actions: Vec<CodeAction>) -> Vec<(ActionGroup, CodeAction)> {
    let mut grouped: Vec<(ActionGroup, CodeAction)> = actions
        .into_iter()
        .map(|action| (ActionGroup::of(action.kind.as_ref()), action))
        .collect();
    grouped.sort_by_key(|(group, action)| {
        (
            *group,
            action.disabled.is_some(),
            !action.is_preferred.unwrap_or(false),
        )
    });
    grouped
}

pub(in crate::environment_shell) struct ActionMenu {
    pub(super) tab: krad_domain::TerminalId,
    document: Document,
    at: usize,
    actions: Vec<(ActionGroup, CodeAction)>,
    selected: usize,
    focus: FocusHandle,
}

impl ActionMenu {
    fn enabled(&self, index: usize) -> bool {
        self.actions
            .get(index)
            .is_some_and(|(_, action)| action.disabled.is_none())
    }

    fn step(&mut self, forward: bool) {
        let count = self.actions.len();
        if count == 0 {
            return;
        }
        let mut next = self.selected;
        for _ in 0..count {
            next = if forward {
                (next + 1) % count
            } else {
                (next + count - 1) % count
            };
            if self.enabled(next) {
                self.selected = next;
                return;
            }
        }
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn show_code_actions(
        &mut self,
        document: &Document,
        at: usize,
        actions: Vec<CodeAction>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_showing(document) else {
            return;
        };
        if actions.is_empty() {
            self.push_toast(ToastKind::Warning, NO_ACTIONS_TEXT.to_owned());
            return;
        }
        let actions = ordered(actions);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut menu = ActionMenu {
            tab,
            document: document.clone(),
            at,
            selected: 0,
            actions,
            focus,
        };
        if !menu.enabled(0) {
            menu.step(true);
        }
        self.editor_popover = Some(EditorPopover::Actions(menu));
        cx.notify();
    }

    fn close_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(EditorPopover::Actions(menu)) = self.editor_popover.take() else {
            return;
        };
        if let Some((state, _)) = self.popover_editor(menu.tab) {
            state.update(cx, |state, cx| state.focus(window, cx));
        }
        cx.notify();
    }

    fn pick_action(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(EditorPopover::Actions(menu)) = &self.editor_popover else {
            return;
        };
        if !menu.enabled(index) {
            return;
        }
        let Some((_, action)) = menu.actions.get(index).cloned() else {
            return;
        };
        let (tab, document) = (menu.tab, menu.document.clone());
        self.close_actions(window, cx);
        if let Some((state, _)) = self.popover_editor(tab) {
            self.run_code_action(&document, state, action, window, cx);
        }
    }

    fn on_actions_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(EditorPopover::Actions(menu)) = &mut self.editor_popover else {
            return;
        };
        match event.keystroke.key.as_str() {
            KEY_UP => menu.step(false),
            KEY_DOWN => menu.step(true),
            KEY_ENTER => {
                let selected = menu.selected;
                self.pick_action(selected, window, cx);
            }
            KEY_ESCAPE => self.close_actions(window, cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn action_row(
        index: usize,
        (group, action): &(ActionGroup, CodeAction),
        selected: bool,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let reason = action
            .disabled
            .as_ref()
            .map(|disabled| disabled.reason.clone());
        let enabled = reason.is_none();
        let preferred = action.is_preferred.unwrap_or(false);
        let title_color = if enabled {
            pal.text_strong
        } else {
            pal.text_disabled
        };
        div()
            .id((ROW_PREFIX, u64::try_from(index).unwrap_or(u64::MAX)))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(if reason.is_some() {
                ROW_DETAIL_H
            } else {
                ROW_H
            }))
            .px(px(SPACE_2))
            .rounded(px(ROW_RADIUS))
            .when(enabled, Styled::cursor_pointer)
            .when(selected && enabled, |row| row.bg(pal.bg_hover))
            .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                let Some(EditorPopover::Actions(menu)) = &mut this.editor_popover else {
                    return;
                };
                if *hovered && menu.enabled(index) {
                    menu.selected = index;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.pick_action(index, window, cx);
            }))
            .child(icon(
                group.glyph(),
                ICON_SM,
                if enabled {
                    pal.text_muted
                } else {
                    pal.text_disabled
                },
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_SM))
                            .text_color(rgb(title_color))
                            .child(SharedString::from(action.title.clone())),
                    )
                    .children(reason.map(|reason| {
                        div()
                            .truncate()
                            .text_size(px(TEXT_XS))
                            .line_height(px(LINE_XS))
                            .text_color(rgb(pal.text_muted))
                            .child(reason)
                    })),
            )
            .when(preferred && enabled, |row| {
                row.child(
                    div()
                        .flex_none()
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.accent))
                        .child(PREFERRED_LABEL),
                )
            })
            .into_any_element()
    }

    pub(super) fn actions_overlay(
        &self,
        menu: &ActionMenu,
        state: &Entity<EditorState>,
        root: (f32, f32),
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let anchor = Self::anchor_bounds(state, &(menu.at..menu.at), cx)?;
        let pal = palette(cx);
        let placement = place(anchor, self.frame_inset, root, (MENU_W, MENU_MAX_H));
        let mut rows = Vec::new();
        let mut group = None;
        for (index, entry) in menu.actions.iter().enumerate() {
            if group != Some(entry.0) {
                group = Some(entry.0);
                rows.push(
                    div()
                        .flex()
                        .flex_none()
                        .items_end()
                        .h(px(HEADER_H))
                        .px(px(SPACE_2))
                        .pb(px(SPACE_1 / 2.0))
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_subtle))
                        .child(entry.0.label())
                        .into_any_element(),
                );
            }
            rows.push(Self::action_row(
                index,
                entry,
                index == menu.selected,
                pal,
                cx,
            ));
        }
        Some(
            div()
                .absolute()
                .inset_0()
                .track_focus(&menu.focus)
                .on_key_down(
                    cx.listener(|this: &mut Self, event: &KeyDownEvent, window, cx| {
                        this.on_actions_key(event, window, cx);
                    }),
                )
                .child(div().absolute().inset_0().occlude().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this: &mut Self, _, window, cx| {
                        this.close_actions(window, cx);
                    }),
                ))
                .child(
                    placed(popover_panel(pal, cx), placement)
                        .id(MENU_ID)
                        .w(px(MENU_W))
                        .overflow_y_scroll()
                        .occlude()
                        .children(rows),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_lsp::lsp_types::CodeActionDisabled;

    fn action(title: &str, kind: Option<CodeActionKind>) -> CodeAction {
        CodeAction {
            title: title.to_owned(),
            kind,
            ..CodeAction::default()
        }
    }

    #[test]
    fn actions_group_by_kind_with_preferred_first_and_disabled_last() {
        let mut preferred = action("Import List", Some(CodeActionKind::QUICKFIX));
        preferred.is_preferred = Some(true);
        let mut disabled = action("Inline", Some(CodeActionKind::REFACTOR_INLINE));
        disabled.disabled = Some(CodeActionDisabled {
            reason: "Nothing to inline".to_owned(),
        });
        let titles: Vec<(ActionGroup, String)> = ordered(vec![
            action(
                "Organize imports",
                Some(CodeActionKind::SOURCE_ORGANIZE_IMPORTS),
            ),
            disabled,
            action("Extract method", Some(CodeActionKind::REFACTOR_EXTRACT)),
            action("Create field", None),
            preferred,
        ])
        .into_iter()
        .map(|(group, action)| (group, action.title))
        .collect();
        assert_eq!(
            titles,
            [
                (ActionGroup::QuickFix, "Import List".to_owned()),
                (ActionGroup::QuickFix, "Create field".to_owned()),
                (ActionGroup::Refactor, "Extract method".to_owned()),
                (ActionGroup::Refactor, "Inline".to_owned()),
                (ActionGroup::Source, "Organize imports".to_owned()),
            ]
        );
        assert_eq!(
            ActionGroup::of(Some(&CodeActionKind::new("quickfixes"))),
            ActionGroup::Other
        );
    }
}
