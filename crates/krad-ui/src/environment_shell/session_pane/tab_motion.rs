use gpui_kit::{
    div, px, AnimationExt as _, AnyElement, IntoElement, ParentElement, SpringAnimation, Styled,
};
use krad_domain::TerminalId;

use super::tab_look::TAB_MAX_W;
use super::TAB_MIN_W;
use crate::theme::MOTION_SPRING;

const ENTER_PREFIX: &str = "session-tab-enter";
const HIDDEN: f32 = 0.0;
const SETTLED: f32 = 1.0;

pub(in crate::environment_shell) fn entering_tab(id: TerminalId, tab: AnyElement) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .flex_1()
        .max_w(px(TAB_MAX_W))
        .child(tab)
        .with_spring(
            (ENTER_PREFIX, id.as_u64()),
            SpringAnimation::new(MOTION_SPRING).to(SETTLED).from(HIDDEN),
            |mut wrapper, shown| {
                let shown = shown.clamp(HIDDEN, SETTLED);
                wrapper.style().flex_grow = Some(shown);
                let wrapper = wrapper.opacity(shown).min_w(px(TAB_MIN_W * shown));
                if shown < SETTLED {
                    wrapper.overflow_hidden()
                } else {
                    wrapper
                }
            },
        )
        .into_any_element()
}
