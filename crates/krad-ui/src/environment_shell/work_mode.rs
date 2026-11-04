mod chat_panel;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, px, rgb, AnimationExt as _, AnyElement, Context, FontWeight, InteractiveElement,
    IntoElement, ParentElement, SpringAnimation, Stateful, StatefulInteractiveElement, Styled,
    Window,
};
use krad_domain::{SessionPanes, WorkMode};
use std::borrow::Cow;

use super::settings_view::segmented_track;
use super::{EnvironmentShell, LiveEnv};
use crate::actions::ToggleWorkMode;
use crate::components::{icon, tooltip_with_shortcut, IconName};
use crate::theme::{
    palette, row_fill, row_shadow, Palette, CAPSULE_PAD, ICON_SM, LINE_XS, MOTION_SPRING,
    SPACE_0_5, SPACE_1_5, TAB_H, TEXT_XS,
};

const SEGMENT_PREFIX: &str = "work-mode";
const PILL_ID: &str = "work-mode-pill";
const SWITCH_TOOLTIP: &str = "Switch work mode";
const SEGMENT_W: f32 = 80.0;

const fn mode_identity(mode: WorkMode) -> (IconName, &'static str) {
    match mode {
        WorkMode::Ade => (IconName::Layers, "ADE"),
        WorkMode::Editor => (IconName::File, "Editor"),
    }
}

impl EnvironmentShell {
    pub(super) fn editing(&self) -> bool {
        self.work_mode == WorkMode::Editor
    }

    pub(super) fn shown_panes<'a>(&self, live: &'a LiveEnv) -> Option<Cow<'a, SessionPanes>> {
        let panes = &live.data.current_session()?.panes;
        if !self.editing() {
            return Some(Cow::Borrowed(panes));
        }
        panes
            .keeping(|id| live.data.is_file_tab(id))
            .map(Cow::Owned)
    }

    fn set_work_mode(&mut self, mode: WorkMode, cx: &mut Context<Self>) {
        if self.work_mode == mode {
            return;
        }
        self.work_mode = mode;
        cx.notify();
        self.store();
    }

    pub(super) fn toggle_work_mode(&mut self, cx: &mut Context<Self>) {
        self.set_work_mode(self.work_mode.toggled(), cx);
    }

    pub(super) fn on_toggle_work_mode(
        &mut self,
        _: &ToggleWorkMode,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_work_mode(cx);
    }

    pub(super) fn work_mode_switch(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let segments: Vec<AnyElement> = WorkMode::ALL
            .into_iter()
            .map(|mode| {
                mode_segment(mode, self.work_mode == mode, pal)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.set_work_mode(mode, cx);
                    }))
                    .into_any_element()
            })
            .collect();
        segmented_track(pal)
            .id(SEGMENT_PREFIX)
            .relative()
            .flex_none()
            .occlude()
            .tooltip(|_, cx| tooltip_with_shortcut(SWITCH_TOOLTIP.into(), &ToggleWorkMode, cx))
            .child(mode_pill(self.work_mode, pal, cx))
            .children(segments)
            .into_any_element()
    }
}

fn pill_left(mode: WorkMode) -> f32 {
    let index = WorkMode::ALL
        .iter()
        .position(|candidate| *candidate == mode)
        .unwrap_or_default();
    let steps = u16::try_from(index).map_or(0.0, f32::from);
    CAPSULE_PAD + steps * (SEGMENT_W + SPACE_0_5)
}

fn mode_pill(mode: WorkMode, pal: &Palette, cx: &gpui_kit::App) -> AnyElement {
    div()
        .absolute()
        .top(px(CAPSULE_PAD))
        .w(px(SEGMENT_W))
        .h(px(TAB_H))
        .rounded_full()
        .bg(row_fill(pal, cx))
        .border_1()
        .border_color(pal.divider)
        .shadow(row_shadow(pal, cx))
        .with_spring(
            PILL_ID,
            SpringAnimation::new(MOTION_SPRING).to(px(pill_left(mode))),
            Styled::left,
        )
        .into_any_element()
}

fn mode_segment(mode: WorkMode, chosen: bool, pal: &Palette) -> Stateful<gpui_kit::Div> {
    let (glyph, label) = mode_identity(mode);
    div()
        .id((SEGMENT_PREFIX, mode as u64))
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(SPACE_1_5))
        .w(px(SEGMENT_W))
        .h(px(TAB_H))
        .rounded_full()
        .cursor_pointer()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(if chosen {
            pal.text_strong
        } else {
            pal.text_default
        }))
        .when(chosen, |segment| segment.font_weight(FontWeight::MEDIUM))
        .when(!chosen, |segment| {
            segment.hover(|style| style.bg(pal.bg_hover))
        })
        .child(icon(glyph, ICON_SM, pal.text_default))
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_work_mode_has_its_own_label_and_icon() {
        let [ade, editor] = WorkMode::ALL.map(mode_identity);
        assert_ne!(ade.0, editor.0);
        assert_ne!(ade.1, editor.1);
    }

    #[test]
    fn the_pill_slides_one_segment_per_work_mode() {
        let [ade, editor] = WorkMode::ALL.map(pill_left);
        assert!((ade - CAPSULE_PAD).abs() < f32::EPSILON);
        assert!((editor - ade - (SEGMENT_W + SPACE_0_5)).abs() < f32::EPSILON);
    }
}
