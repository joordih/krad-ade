use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::Switch;

use super::super::super::dialog_parts::{segment, SegmentSize};
use super::super::super::EnvironmentShell;
use super::super::layout::{EDITOR_MAX_W, PAGE_PAD};
use super::parts::{chips, field, field_group, hint};
use super::steps::{ready_to_save, step_frame, step_state, Step, StepFacts, StepState, StepView};
use super::{AutomationEditor, Cheer};
use crate::components::motion::pop;
use crate::components::{chip, toggle_switch, Button, ButtonVariant, IconName};
use crate::kraddie_avatar::kraddie_avatar;
use crate::theme::{palette, Palette, LINE_XS, SPACE_1, SPACE_2, SPACE_3, TEXT_XS};

const EDITOR_ID: &str = "automation-editor";
const OWNER_PREFIX: &str = "automation-owner";
const BTN_BACK: &str = "automation-back";
const BTN_DISCARD: &str = "automation-discard";
const BTN_PRIMARY: &str = "automation-primary";
const BTN_RUN: &str = "automation-run";
const BTN_RETIRE: &str = "automation-retire";
const DUTY_ID: &str = "automation-duty";
const DUTY_KNOB: &str = "automation-duty-knob";
const CELEBRATE_PREFIX: &str = "automation-celebrate";
const OWNER_AVATAR: f32 = 20.0;
const BAR_PAD_Y: f32 = 10.0;
const BACK_LABEL: &str = "Back to the board";
const CLOSE_TOOLTIP: &str = "Close the editor";
const NAME_LABEL: &str = "Call it";
const OWNER_LABEL: &str = "Kraddie in charge";
const NO_KRADDIES: &str = "Adopt a Kraddie first; it is the one that carries this out.";
const DISCARD_LABEL: &str = "Discard";
const LAUNCH_LABEL: &str = "Launch";
const SAVE_LABEL: &str = "Save changes";
const LAUNCHED_LABEL: &str = "Launched";
const SAVED_LABEL: &str = "Saved";
const RUN_TOOLTIP: &str = "Run it now";
const RETIRE_TOOLTIP: &str = "Retire this automation for good";
const ON_DUTY_LABEL: &str = "On duty";
const RESTING_LABEL: &str = "Resting";
const READY_LABEL: &str = "Ready when you are";
const STEPS_LEFT: &str = "steps to go";
const STEP_LEFT: &str = "step to go";

fn readiness(facts: StepFacts) -> SharedString {
    if ready_to_save(facts) {
        return READY_LABEL.into();
    }
    let left = Step::ALL
        .into_iter()
        .filter(|step| step_state(*step, facts, None) != StepState::Done)
        .count();
    let unit = if left == 1 { STEP_LEFT } else { STEPS_LEFT };
    format!("{left} {unit}").into()
}

impl EnvironmentShell {
    fn step_facts(&self, editor: &AutomationEditor, cx: &Context<Self>) -> StepFacts {
        StepFacts {
            named: !editor.name.read(cx).value().trim().is_empty(),
            owned: editor
                .draft
                .kraddie
                .as_ref()
                .is_some_and(|kraddie| self.kraddies.entry(kraddie).is_some()),
            cues: editor.schedules.len() + editor.draft.triggers.len(),
            briefed: !editor.brief.read(cx).value().trim().is_empty(),
        }
    }

    pub(in crate::environment_shell) fn automation_editor(
        &self,
        rail: bool,
        beside: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(editor) = self.automations.editor.as_deref() else {
            return div().into_any_element();
        };
        let pal = palette(cx);
        let facts = self.step_facts(editor, cx);
        let flawed = editor.flaw.as_ref().map(|flaw| flaw.step);
        let steps: Vec<AnyElement> = Step::ALL
            .into_iter()
            .map(|step| {
                let body = match step {
                    Step::Crew => self.crew_step(editor, pal, cx),
                    Step::Cue => self.cue_step(editor, pal, cx),
                    Step::Brief => self.brief_step(editor, pal, cx),
                    Step::Workspace => self.workspace_step(editor, pal, cx),
                    Step::Missed => Self::missed_step(editor, pal, cx),
                };
                step_frame(
                    StepView {
                        step,
                        state: step_state(step, facts, flawed),
                        rail,
                        last: step == Step::Missed,
                        flaw: editor.flaw.as_ref(),
                        body,
                    },
                    pal,
                )
            })
            .collect();
        let top = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(
                if beside {
                    Button::new(BTN_BACK)
                        .icon(IconName::Close)
                        .small()
                        .tooltip(CLOSE_TOOLTIP)
                } else {
                    Button::new(BTN_BACK)
                        .label(BACK_LABEL)
                        .icon(IconName::ChevronLeft)
                        .small()
                }
                .on_click(
                    cx.listener(|this: &mut Self, _, _, cx| this.close_automation_editor(cx)),
                ),
            )
            .child(div().flex_1())
            .child(chip(readiness(facts), pal));
        let column = div()
            .flex()
            .flex_col()
            .w_full()
            .max_w(px(EDITOR_MAX_W))
            .mx_auto()
            .gap(px(SPACE_3))
            .child(top)
            .children(steps)
            .children(self.automation_logbook(editor, pal));
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(
                div()
                    .id(EDITOR_ID)
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(PAGE_PAD))
                    .child(column),
            )
            .child(Self::action_bar(editor, pal, cx))
            .into_any_element()
    }

    fn crew_step(
        &self,
        editor: &AutomationEditor,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let frame = self.kraddies.frame();
        let owners: Vec<AnyElement> = self
            .kraddies
            .entries
            .iter()
            .enumerate()
            .map(|(index, kraddie)| {
                let id = kraddie.kraddie.id();
                let selected = editor.draft.kraddie.as_ref() == Some(&id);
                let sprite = kraddie.sprite(self.kraddie_mood(kraddie));
                self.kraddies.note_drawn(sprite);
                segment(
                    (OWNER_PREFIX, index),
                    SharedString::from(kraddie.kraddie.name.to_string()),
                    None,
                    selected,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .pl(px(SPACE_1))
                .child(kraddie_avatar(sprite, OWNER_AVATAR, frame))
                .flex_row_reverse()
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    let id = id.clone();
                    this.edit_automation_draft(cx, |editor| {
                        editor.draft.kraddie = Some(id);
                        if editor
                            .flaw
                            .as_ref()
                            .is_some_and(|flaw| flaw.step == Step::Crew)
                        {
                            editor.flaw = None;
                        }
                    });
                }))
                .into_any_element()
            })
            .collect();
        let empty = owners.is_empty();
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_3))
            .child(
                field_group()
                    .child(field(NAME_LABEL, pal))
                    .child(Input::new(&editor.name)),
            )
            .child(
                field_group()
                    .child(field(OWNER_LABEL, pal))
                    .child(chips().children(owners))
                    .when(empty, |group| group.child(hint(NO_KRADDIES, pal))),
            )
            .into_any_element()
    }

    fn action_bar(editor: &AutomationEditor, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let on = editor.draft.switch.is_on();
        let duty = div()
            .id(DUTY_ID)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .cursor_pointer()
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.edit_automation_draft(cx, |editor| {
                    let next = match editor.draft.switch {
                        Switch::On => Switch::Off,
                        Switch::Off => Switch::On,
                    };
                    editor.set_switch(next);
                });
            }))
            .child(toggle_switch(DUTY_KNOB, on, pal))
            .child(
                div()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(if on { pal.text_strong } else { pal.text_muted }))
                    .child(if on { ON_DUTY_LABEL } else { RESTING_LABEL }),
            );
        let editing = editor.editing.is_some();
        let run = editor.editing.clone().map(|key| {
            Button::new(BTN_RUN)
                .icon(IconName::Play)
                .small()
                .tooltip(RUN_TOOLTIP)
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.run_automation_now(&key, cx);
                }))
        });
        let retire = editing.then(|| {
            Button::new(BTN_RETIRE)
                .icon(IconName::Minus)
                .small()
                .tooltip(RETIRE_TOOLTIP)
                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                    this.retire_edited_automation(cx);
                }))
        });
        let (label, glyph) = match (editor.celebrating, editing) {
            (Some((_, Cheer::Launched)), _) => (LAUNCHED_LABEL, Some(IconName::Tick)),
            (Some((_, Cheer::Saved)), _) => (SAVED_LABEL, Some(IconName::Tick)),
            (None, true) => (SAVE_LABEL, None),
            (None, false) => (LAUNCH_LABEL, Some(IconName::Send)),
        };
        let mut primary = Button::new(BTN_PRIMARY)
            .label(label)
            .variant(ButtonVariant::Primary)
            .on_click(cx.listener(|this: &mut Self, _, _, cx| this.save_automation_editor(cx)));
        if let Some(glyph) = glyph {
            primary = primary.icon(glyph);
        }
        let primary = match editor.celebrating {
            Some((celebration, _)) => pop(
                ElementId::NamedInteger(CELEBRATE_PREFIX.into(), celebration),
                div().relative().child(primary),
            ),
            None => primary.into_any_element(),
        };
        div()
            .flex()
            .flex_none()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(PAGE_PAD))
            .py(px(BAR_PAD_Y))
            .border_t_1()
            .border_color(pal.divider)
            .bg(rgb(pal.bg_surface))
            .child(duty)
            .children(run)
            .children(retire)
            .child(div().flex_1())
            .child(
                Button::new(BTN_DISCARD)
                    .label(DISCARD_LABEL)
                    .variant(ButtonVariant::Secondary)
                    .on_click(
                        cx.listener(|this: &mut Self, _, _, cx| this.close_automation_editor(cx)),
                    ),
            )
            .child(primary)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_readiness_chip_counts_what_is_left() {
        assert_eq!(readiness(StepFacts::default()).as_ref(), "3 steps to go");
        let almost = StepFacts {
            named: true,
            owned: true,
            cues: 1,
            briefed: false,
        };
        assert_eq!(readiness(almost).as_ref(), "1 step to go");
        let done = StepFacts {
            briefed: true,
            ..almost
        };
        assert_eq!(readiness(done).as_ref(), READY_LABEL);
    }
}
