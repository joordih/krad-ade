use gpui_kit::{
    div, px, rgb, AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};

use super::super::EnvironmentShell;
use super::state::HostTone;
use crate::components::{tooltip_view, Button, ButtonVariant, IconName};
use crate::theme::{palette, Palette, LINE_XS, SPACE_1, SPACE_1_5, SPACE_2, TEXT_XS};

const DOT: f32 = 8.0;
const BADGE_ID: &str = "sidebar-host-badge";
const BTN_RECONNECT: &str = "sidebar-host-reconnect";
const RECONNECT_LABEL: &str = "Reconnect";
const SEPARATOR: &str = " · ";

const fn tone_color(tone: HostTone, pal: &Palette) -> u32 {
    match tone {
        HostTone::Ready => pal.success_fg,
        HostTone::Pending => pal.warning_fg,
        HostTone::Off => pal.text_muted,
        HostTone::Broken => pal.error_fg,
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn host_badge(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let live = self.envs.get(&self.active)?;
        let destination = live.data.host.destination()?.to_string();
        let pal = palette(cx);
        let state = &live.host;
        let tooltip: SharedString = match state.detail() {
            Some(detail) => format!(
                "{destination}{SEPARATOR}{}{SEPARATOR}{detail}",
                state.label()
            ),
            None => format!("{destination}{SEPARATOR}{}", state.label()),
        }
        .into();
        let reconnect = state.can_reconnect().then(|| {
            let env = self.active.clone();
            Button::new(BTN_RECONNECT)
                .label(RECONNECT_LABEL)
                .icon(IconName::Refresh)
                .variant(ButtonVariant::Secondary)
                .small()
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.reconnect_to_host(&env, cx);
                }))
        });
        Some(
            div()
                .id(BADGE_ID)
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_1_5))
                .px(px(SPACE_2))
                .py(px(SPACE_1))
                .tooltip(move |_, cx| tooltip_view(tooltip.clone(), cx))
                .child(
                    div()
                        .flex_none()
                        .size(px(DOT))
                        .rounded_full()
                        .bg(rgb(tone_color(state.tone(), pal))),
                )
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_default))
                        .child(destination),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(state.label()),
                )
                .children(reconnect)
                .into_any_element(),
        )
    }
}
