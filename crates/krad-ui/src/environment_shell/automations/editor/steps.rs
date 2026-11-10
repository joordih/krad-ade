use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Div, ElementId, FontWeight, IntoElement, ParentElement, Styled,
};
use krad_domain::{AutomationError, DraftError};

use super::super::AutomationMenu;
use super::Flaw;
use crate::components::motion::{pop, shaking};
use crate::components::{icon, IconName};
use crate::theme::{
    tinted, Palette, ICON_XS, LINE_SM, LINE_XS, RADIUS_LG, SPACE_1, SPACE_2, SPACE_3, SPACE_5,
    TEXT_2XS, TEXT_SM, TEXT_XS,
};

const MARKER: f32 = 24.0;
const CONNECTOR_W: f32 = 2.0;
const FLAW_FILL_ALPHA: u32 = 0x14;
const CONNECTOR_ALPHA: u32 = 0x50;
const MARKER_PREFIX: &str = "automation-step-marker";
const SHAKE_PREFIX: &str = "automation-step-shake";
const STEP_SEPARATOR: &str = " · ";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Step {
    Crew,
    Cue,
    Brief,
    Workspace,
    Missed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StepState {
    Done,
    Pending,
    Flawed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct StepFacts {
    pub(super) named: bool,
    pub(super) owned: bool,
    pub(super) cues: usize,
    pub(super) briefed: bool,
}

impl Step {
    pub(super) const ALL: [Self; 5] = [
        Self::Crew,
        Self::Cue,
        Self::Brief,
        Self::Workspace,
        Self::Missed,
    ];

    pub(super) const fn numeral(self) -> &'static str {
        match self {
            Self::Crew => "1",
            Self::Cue => "2",
            Self::Brief => "3",
            Self::Workspace => "4",
            Self::Missed => "5",
        }
    }

    pub(super) const fn tag(self) -> &'static str {
        match self {
            Self::Crew => "Crew",
            Self::Cue => "Cue",
            Self::Brief => "Brief",
            Self::Workspace => "Workspace",
            Self::Missed => "Missed beats",
        }
    }

    pub(super) const fn question(self) -> &'static str {
        match self {
            Self::Crew => "Who takes it on?",
            Self::Cue => "What wakes it up?",
            Self::Brief => "What is the job?",
            Self::Workspace => "Where does it work?",
            Self::Missed => "And if the app was closed?",
        }
    }

    const fn index(self) -> u64 {
        match self {
            Self::Crew => 0,
            Self::Cue => 1,
            Self::Brief => 2,
            Self::Workspace => 3,
            Self::Missed => 4,
        }
    }

    pub(super) const fn settled_by(self, choice: &AutomationMenu) -> bool {
        match self {
            Self::Cue => matches!(
                choice,
                AutomationMenu::AddTrigger(_) | AutomationMenu::AddSchedule(_)
            ),
            Self::Brief => matches!(
                choice,
                AutomationMenu::Harness(_)
                    | AutomationMenu::Effort(_)
                    | AutomationMenu::Permission(_)
            ),
            Self::Workspace => matches!(choice, AutomationMenu::Group(_)),
            Self::Crew | Self::Missed => false,
        }
    }
}

#[must_use]
pub(super) const fn flawed_step(error: DraftError) -> Step {
    match error {
        DraftError::Name(_) | DraftError::NoKraddie => Step::Crew,
        DraftError::NoTrigger
        | DraftError::Automation(
            AutomationError::RepeatedTrigger | AutomationError::TooManyTriggers,
        ) => Step::Cue,
        DraftError::NoInstructions
        | DraftError::Automation(
            AutomationError::InstructionsTooLong | AutomationError::NotAnAgent,
        ) => Step::Brief,
        DraftError::Automation(AutomationError::GroupNeedsThisRepo) => Step::Workspace,
    }
}

#[must_use]
pub(super) fn step_state(step: Step, facts: StepFacts, flawed: Option<Step>) -> StepState {
    if flawed == Some(step) {
        return StepState::Flawed;
    }
    let done = match step {
        Step::Crew => facts.named && facts.owned,
        Step::Cue => facts.cues > 0,
        Step::Brief => facts.briefed,
        Step::Workspace | Step::Missed => true,
    };
    if done {
        StepState::Done
    } else {
        StepState::Pending
    }
}

#[must_use]
pub(super) fn ready_to_save(facts: StepFacts) -> bool {
    Step::ALL
        .into_iter()
        .all(|step| step_state(step, facts, None) == StepState::Done)
}

fn marker(step: Step, state: StepState, pal: &Palette) -> AnyElement {
    let base = div()
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(MARKER))
        .rounded_full()
        .text_size(px(TEXT_2XS))
        .font_weight(FontWeight::SEMIBOLD);
    match state {
        StepState::Done => pop(
            ElementId::NamedInteger(MARKER_PREFIX.into(), step.index()),
            base.bg(rgb(pal.accent))
                .child(icon(IconName::Tick, ICON_XS, pal.on_accent)),
        ),
        StepState::Pending => base
            .border_1()
            .border_color(pal.border)
            .text_color(rgb(pal.text_muted))
            .child(step.numeral())
            .into_any_element(),
        StepState::Flawed => base
            .bg(rgb(pal.error_fg))
            .text_color(rgb(pal.on_accent))
            .child(step.numeral())
            .into_any_element(),
    }
}

fn heading(step: Step, rail: bool, pal: &Palette) -> Div {
    let tag = if rail {
        step.tag().to_uppercase()
    } else {
        format!(
            "{}{STEP_SEPARATOR}{}",
            step.numeral(),
            step.tag().to_uppercase()
        )
    };
    div()
        .flex()
        .flex_col()
        .gap(px(SPACE_1))
        .child(
            div()
                .text_size(px(TEXT_2XS))
                .line_height(px(LINE_XS))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(pal.text_muted))
                .child(tag),
        )
        .child(
            div()
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(pal.text_strong))
                .child(step.question()),
        )
}

pub(super) struct StepView<'a> {
    pub(super) step: Step,
    pub(super) state: StepState,
    pub(super) rail: bool,
    pub(super) last: bool,
    pub(super) flaw: Option<&'a Flaw>,
    pub(super) body: AnyElement,
}

pub(super) fn step_frame(view: StepView<'_>, pal: &Palette) -> AnyElement {
    let StepView {
        step,
        state,
        rail,
        last,
        flaw,
        body,
    } = view;
    let flaw = flaw.filter(|flaw| flaw.step == step);
    let content = div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(px(SPACE_2))
        .pb(px(SPACE_5))
        .child(heading(step, rail, pal))
        .child(body)
        .when_some(flaw, |content, flaw| {
            content.child(
                div()
                    .px(px(SPACE_3))
                    .py(px(SPACE_2))
                    .rounded(px(RADIUS_LG))
                    .bg(tinted(pal.error_fg, FLAW_FILL_ALPHA))
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.error_fg))
                    .child(flaw.message.clone()),
            )
        });
    let content = match flaw {
        Some(flaw) => shaking(
            ElementId::NamedInteger(SHAKE_PREFIX.into(), flaw.attempt),
            content,
        ),
        None => content.into_any_element(),
    };
    if !rail {
        return content;
    }
    let connector_color = if state == StepState::Done {
        tinted(pal.accent, CONNECTOR_ALPHA)
    } else {
        tinted(pal.text_disabled, CONNECTOR_ALPHA)
    };
    div()
        .flex()
        .flex_row()
        .gap(px(SPACE_3))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .flex_none()
                .w(px(MARKER))
                .child(marker(step, state, pal))
                .when(!last, |column| {
                    column.child(
                        div()
                            .flex_1()
                            .w(px(CONNECTOR_W))
                            .my(px(SPACE_1))
                            .rounded_full()
                            .bg(connector_color),
                    )
                }),
        )
        .child(content)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AutomationNameError, Trigger};

    fn complete() -> StepFacts {
        StepFacts {
            named: true,
            owned: true,
            cues: 1,
            briefed: true,
        }
    }

    #[test]
    fn steps_complete_from_what_the_draft_holds() {
        assert!(ready_to_save(complete()));
        let empty = StepFacts::default();
        let states: Vec<StepState> = Step::ALL
            .into_iter()
            .map(|step| step_state(step, empty, None))
            .collect();
        assert_eq!(
            states,
            [
                StepState::Pending,
                StepState::Pending,
                StepState::Pending,
                StepState::Done,
                StepState::Done
            ]
        );
        assert!(!ready_to_save(StepFacts {
            cues: 0,
            ..complete()
        }));
    }

    #[test]
    fn a_flaw_marks_its_step_even_when_the_rest_looks_done() {
        assert_eq!(
            step_state(Step::Workspace, complete(), Some(Step::Workspace)),
            StepState::Flawed
        );
        assert_eq!(
            step_state(Step::Cue, complete(), Some(Step::Workspace)),
            StepState::Done
        );
    }

    #[test]
    fn every_draft_error_points_at_the_step_that_fixes_it() {
        assert_eq!(
            flawed_step(DraftError::Name(AutomationNameError::Empty)),
            Step::Crew
        );
        assert_eq!(flawed_step(DraftError::NoKraddie), Step::Crew);
        assert_eq!(flawed_step(DraftError::NoTrigger), Step::Cue);
        assert_eq!(flawed_step(DraftError::NoInstructions), Step::Brief);
        assert_eq!(
            flawed_step(DraftError::Automation(AutomationError::NotAnAgent)),
            Step::Brief
        );
        assert_eq!(
            flawed_step(DraftError::Automation(AutomationError::GroupNeedsThisRepo)),
            Step::Workspace
        );
        assert_eq!(
            flawed_step(DraftError::Automation(AutomationError::TooManyTriggers)),
            Step::Cue
        );
    }

    #[test]
    fn picking_a_cue_clears_a_cue_flaw_only() {
        let cue = AutomationMenu::AddTrigger(Trigger::Manual);
        assert!(Step::Cue.settled_by(&cue));
        assert!(!Step::Brief.settled_by(&cue));
        assert!(Step::Workspace.settled_by(&AutomationMenu::Group(None)));
    }
}
