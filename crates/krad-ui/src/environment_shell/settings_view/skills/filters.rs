use gpui_kit::component::input::Input;
use gpui_kit::component::Sizable as _;
use gpui_kit::{
    div, px, AnyElement, Context, ElementId, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled,
};
use krad_chat::SkillHarness;
use krad_domain::{AgentPreset, SkillKind};

use super::super::super::dialog_parts::{segment, SegmentSize};
use super::super::segmented_track;
use super::{EnvironmentShell, SkillsUi};
use crate::components::{harness_icon, IconName};
use crate::theme::{palette, Palette, SPACE_2};

const ALL_LABEL: &str = "All";
const KIND_SEGMENT: &str = "skills-kind";
const HARNESS_SEGMENT: &str = "skills-harness";
const ALL_KEY: &str = "all";

pub(super) fn harness_mark(harness: SkillHarness, pal: &Palette) -> (IconName, u32) {
    match harness {
        SkillHarness::ClaudeCode => harness_icon(AgentPreset::ClaudeCode, pal),
        SkillHarness::Codex => harness_icon(AgentPreset::Codex, pal),
        SkillHarness::Opencode => harness_icon(AgentPreset::Opencode, pal),
        SkillHarness::Shared => (IconName::Layers, pal.text_subtle),
    }
}

pub(super) const fn kind_mark(kind: SkillKind) -> IconName {
    match kind {
        SkillKind::Skill => IconName::Magic,
        SkillKind::Agent => IconName::UserGroup,
        SkillKind::Command => IconName::Terminal,
    }
}

fn segment_id(group: &str, key: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("{group}-{key}")))
}

impl EnvironmentShell {
    pub(super) fn skill_filters(ui: &SkillsUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let mut kinds = vec![segment(
            segment_id(KIND_SEGMENT, ALL_KEY),
            ALL_LABEL,
            None,
            ui.kind.is_none(),
            SegmentSize::Compact,
            pal,
            cx,
        )
        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
            this.set_skill_kind_filter(None, cx);
        }))];
        kinds.extend(SkillKind::ALL.map(|kind| {
            segment(
                segment_id(KIND_SEGMENT, kind.label()),
                kind.label(),
                Some(kind_mark(kind)),
                ui.kind == Some(kind),
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.set_skill_kind_filter(Some(kind), cx);
            }))
        }));
        let mut harnesses = vec![segment(
            segment_id(HARNESS_SEGMENT, ALL_KEY),
            ALL_LABEL,
            None,
            ui.harness.is_none(),
            SegmentSize::Compact,
            pal,
            cx,
        )
        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
            this.set_skill_harness_filter(None, cx);
        }))];
        harnesses.extend(SkillHarness::ALL.map(|harness| {
            let (mark, _) = harness_mark(harness, pal);
            segment(
                segment_id(HARNESS_SEGMENT, harness.label()),
                harness.label(),
                Some(mark),
                ui.harness == Some(harness),
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.set_skill_harness_filter(Some(harness), cx);
            }))
        }));
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .child(Input::new(&ui.search).small().cleanable(true))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(SPACE_2))
                    .child(segmented_track(pal).children(kinds))
                    .child(segmented_track(pal).flex_wrap().children(harnesses)),
            )
            .into_any_element()
    }
}
