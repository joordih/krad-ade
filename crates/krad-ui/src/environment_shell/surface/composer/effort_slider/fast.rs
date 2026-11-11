use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::TerminalId;

use super::super::super::super::EnvironmentShell;
use crate::components::{icon, toggle_switch, tooltip_view, IconName};
use crate::theme::{Palette, ICON_XS, LINE_XS, RADIUS_SM, SPACE_1_5, SPACE_2, TEXT_XS};

const FAST_ROW_PREFIX: &str = "effort-popover-fast";
const FAST_SWITCH_PREFIX: &str = "effort-popover-fast-switch";
const FAST_LABEL: &str = "Fast mode";
const FAST_HINT: &str = "Quicker answers on models that support it";
const TOOLTIP_FAST_ON: &str = "Turn fast mode off";
const TOOLTIP_FAST_OFF: &str = "Turn fast mode on";

impl EnvironmentShell {
    pub(super) fn fast_row(
        chat: TerminalId,
        on: bool,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tooltip = if on {
            TOOLTIP_FAST_ON
        } else {
            TOOLTIP_FAST_OFF
        };
        div()
            .id((FAST_ROW_PREFIX, chat.as_u64()))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_1_5))
            .py(px(SPACE_1_5))
            .rounded(px(RADIUS_SM))
            .cursor_pointer()
            .hover(|row| row.bg(pal.bg_hover))
            .active(|row| row.bg(pal.bg_selected))
            .tooltip(move |_, cx| tooltip_view(tooltip.into(), cx))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.switch_acp_fast(chat, !on, cx);
            }))
            .child(icon(
                IconName::Flash,
                ICON_XS,
                if on { pal.accent } else { pal.text_muted },
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(pal.text_default))
                            .child(FAST_LABEL),
                    )
                    .child(div().text_color(rgb(pal.text_subtle)).child(FAST_HINT)),
            )
            .child(toggle_switch((FAST_SWITCH_PREFIX, chat.as_u64()), on, pal))
            .into_any_element()
    }
}
