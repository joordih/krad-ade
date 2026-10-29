use gpui_kit::component::input::{Enter, Escape, IndentInline, MoveDown, MoveUp};
use gpui_kit::{Context, Div, InteractiveElement, Window};

use super::super::suggest::Composer;
use super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn suggestion_keys(
        field: Div,
        composer: Composer,
        cx: &mut Context<Self>,
    ) -> Div {
        field
            .capture_action(cx.listener(move |this: &mut Self, _: &MoveUp, _, cx| {
                if this.suggesting_in(composer) {
                    this.move_suggestion(false, cx);
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(move |this: &mut Self, _: &MoveDown, _, cx| {
                if this.suggesting_in(composer) {
                    this.move_suggestion(true, cx);
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(move |this: &mut Self, _: &Enter, window, cx| {
                this.accept_highlighted(composer, window, cx);
            }))
            .capture_action(
                cx.listener(move |this: &mut Self, _: &IndentInline, window, cx| {
                    this.accept_highlighted(composer, window, cx);
                }),
            )
            .capture_action(cx.listener(move |this: &mut Self, _: &Escape, _, cx| {
                if this.suggesting_in(composer) {
                    this.close_suggest(cx);
                    cx.stop_propagation();
                }
            }))
    }

    fn suggesting_in(&self, composer: Composer) -> bool {
        self.chat
            .suggest
            .as_ref()
            .is_some_and(|suggest| suggest.composer == composer)
    }

    fn accept_highlighted(
        &mut self,
        composer: Composer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.suggesting_in(composer) {
            return;
        }
        let Some(selected) = self.chat.suggest.as_ref().map(|suggest| suggest.selected) else {
            return;
        };
        if self.suggestions().is_none_or(|rows| rows.is_empty()) {
            return;
        }
        self.accept_suggestion(selected, window, cx);
        cx.stop_propagation();
    }
}
