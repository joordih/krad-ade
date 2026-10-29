use gpui_kit::component::kbd::Kbd;
use gpui_kit::{
    div, px, rgb, Action, AnyView, App, AppContext, AsKeystroke as _, Context, IntoElement,
    KeyContext, ParentElement, Render, SharedString, Styled, Window,
};

use crate::actions::SHELL_KEY_CONTEXT;
use crate::theme::{LINE_XS, RADIUS_XS, SPACE_1, SPACE_2, TEXT_XS};

const TOOLTIP_BG: u32 = 0x0029_2929;
const TOOLTIP_FG: u32 = 0x00ff_ffff;
const TOOLTIP_KEYS_FG: u32 = 0x00a3_a3a3;

struct TooltipView {
    text: SharedString,
    shortcut: Option<Box<dyn Action>>,
}

impl Render for TooltipView {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let keys = self.shortcut.as_deref().and_then(|action| {
            let context = KeyContext::parse(SHELL_KEY_CONTEXT).ok()?;
            let binding =
                window.highest_precedence_binding_for_action_in_context(action, context)?;
            let keys: Vec<String> = binding
                .keystrokes()
                .iter()
                .map(|stroke| Kbd::format(stroke.as_keystroke()))
                .collect();
            Some(keys.join(" "))
        });
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_2))
            .py(px(SPACE_1))
            .rounded(px(RADIUS_XS))
            .bg(rgb(TOOLTIP_BG))
            .text_color(rgb(TOOLTIP_FG))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .child(self.text.clone())
            .children(keys.map(|keys| div().text_color(rgb(TOOLTIP_KEYS_FG)).child(keys)))
    }
}

pub(crate) fn tooltip_view(text: SharedString, cx: &mut App) -> AnyView {
    cx.new(|_| TooltipView {
        text,
        shortcut: None,
    })
    .into()
}

pub(crate) fn tooltip_with_shortcut(
    text: SharedString,
    action: &dyn Action,
    cx: &mut App,
) -> AnyView {
    let shortcut = Some(action.boxed_clone());
    cx.new(|_| TooltipView { text, shortcut }).into()
}
