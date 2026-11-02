mod chat_panel;

use gpui_kit::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{SessionPanes, WorkMode};
use std::borrow::Cow;

use super::dialog_parts::{segment, SegmentSize};
use super::settings_view::segmented_track;
use super::{EnvironmentShell, LiveEnv};
use crate::actions::ToggleWorkMode;
use crate::components::{tooltip_with_shortcut, IconName};
use crate::theme::palette;

const SEGMENT_PREFIX: &str = "work-mode";
const SWITCH_TOOLTIP: &str = "Switch work mode";

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
                let (glyph, label) = mode_identity(mode);
                segment(
                    (SEGMENT_PREFIX, mode as u64),
                    label,
                    Some(glyph),
                    self.work_mode == mode,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.set_work_mode(mode, cx);
                }))
                .into_any_element()
            })
            .collect();
        segmented_track(pal)
            .id(SEGMENT_PREFIX)
            .flex_none()
            .occlude()
            .tooltip(|_, cx| tooltip_with_shortcut(SWITCH_TOOLTIP.into(), &ToggleWorkMode, cx))
            .children(segments)
            .into_any_element()
    }
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
}
