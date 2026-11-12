use gpui_kit::{
    div, px, rgb, AnyElement, Context, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Styled,
};
use krad_domain::TerminalId;

use super::super::super::super::EnvironmentShell;
use super::popover::{HIGH_END, LOW_END};
use super::EffortPlacement;
use crate::theme::{palette, LINE_XS, SPACE_2, TEXT_XS};

impl EnvironmentShell {
    pub(in crate::environment_shell) fn embedded_effort(
        &self,
        chat: TerminalId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let popover = self
            .effort_popover
            .as_ref()
            .filter(|popover| popover.placement == EffortPlacement::InPicker)
            .filter(|popover| popover.state.chat == chat)?;
        let (facts, track, was) = self.effort_track_of(chat, cx.reduce_motion())?;
        let pal = palette(cx);
        let help = popover.state.help.then(|| Self::effort_help(&facts, pal));
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(SPACE_2))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, _| this.release_effort()),
                )
                .child(Self::effort_header(
                    &facts,
                    &track,
                    was,
                    popover.state.help,
                    pal,
                    cx,
                ))
                .children(help)
                .child(Self::effort_track(&track, &facts, pal, cx))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_subtle))
                        .child(LOW_END)
                        .child(HIGH_END),
                )
                .children(facts.fast.map(|on| Self::fast_row(chat, on, pal, cx)))
                .into_any_element(),
        )
    }
}
