use gpui_kit::component::input::Textarea;
use gpui_kit::component::text::TextView;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, ease_out_quint, px, rgb, Animation, AnimationExt as _, AnyElement, ClickEvent, Context,
    Div, ElementId, FontWeight, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use krad_chat::{AskKind, Question, QuestionOption};
use krad_domain::{AskOutcome, AskVerdict, PermissionMode};
use std::time::Duration;

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::surface::{edit_diff, tool_question};
use super::super::EnvironmentShell;
use super::picks::{offered, OTHER_LABEL};
use super::{AskOwner, AskQueue, Choice, NotePurpose, Waiting, OFFERED_MODES};
use crate::actions::{AskConfirm, AskDeny, ASK_KEY_CONTEXT};
use crate::components::{check_box, icon, tooltip_view, Button, ButtonVariant, IconName};
use crate::theme::{
    palette, Palette, ICON, ICON_XS, LINE_2XS, LINE_SM, LINE_XS, RADIUS_LG, RADIUS_MD, RADIUS_SM,
    RADIUS_XS, SPACE_0_5, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3, TEXT_2XS, TEXT_SM, TEXT_XS,
};

const DETAIL_MAX_H: f32 = 160.0;
const TRAY_TUCK: f32 = 14.0;
const TRAY_BUTTON_H: f32 = 28.0;
const DIVIDER_H: f32 = 1.0;
const PRIMARY_HOVER_OPACITY: f32 = 0.9;
const PLAN_MAX_H: f32 = 320.0;
const MODE_CONTROL_H: f32 = 24.0;
const NUMBER_BADGE: f32 = 18.0;
const STEP_ENTER: Duration = Duration::from_millis(180);
const ROWS_PER_STEP: usize = 100;
const NUMBER_KEYS: std::ops::RangeInclusive<usize> = 1..=9;

const CARD_PREFIX: &str = "ask-card";
const CHOICE_PREFIX: &str = "ask-choice";
const OPTION_PREFIX: &str = "ask-option";
const OTHER_PREFIX: &str = "ask-other";
const SUBMIT_PREFIX: &str = "ask-submit";
const DENY_PREFIX: &str = "ask-deny";
const NOTE_SEND_PREFIX: &str = "ask-note-send";
const PLAN_PREFIX: &str = "ask-plan";
const STEP_PREFIX: &str = "ask-step";
const PREVIOUS_PREFIX: &str = "ask-previous";
const MODE_PREFIX: &str = "ask-mode";
const CHAT_KEY: &str = "chat";

const PLAN_QUESTION: &str = "Approve this plan?";
const DETAIL_PREFIX: &str = "ask-detail";
const COUNT_SEPARATOR: &str = " of ";
const ALLOW_LABEL: &str = "Allow once";
const ALLOW_ALWAYS_LABEL: &str = "Always allow";
const DENY_LABEL: &str = "Deny…";
const DENY_SHORT_LABEL: &str = "Deny";
const APPROVE_LABEL: &str = "Approve";
const APPROVE_AUTO_LABEL: &str = "Approve in auto mode";
const APPROVE_MANUALLY_LABEL: &str = "Approve, ask before edits";
const KEEP_PLANNING_SHORT_LABEL: &str = "Keep planning";
const ACCEPT_EDITS_LABEL: &str = "Yes, and accept edits";
const AUTO_LABEL: &str = "Yes, and use auto mode";
const MANUAL_LABEL: &str = "Yes, approve edits by hand";
const KEEP_PLANNING_LABEL: &str = "No, keep planning…";
const OTHER_PLACEHOLDER: &str = "Enter your custom answer here";
const SKIP_TOOLTIP: &str = "Skip the questions";
const SKIP_LABEL: &str = "Skip";
const PREVIOUS_LABEL: &str = "Back";
const NEXT_LABEL: &str = "Next";
const SUBMIT_LABEL: &str = "Submit";
const SEND_LABEL: &str = "Send";
const NOTE_HINT: &str = "Enter sends";
const TOOLTIP_MODE: &str = "Permission mode";
const MODE_HEADER: &str = "Permission mode";
const VERDICT_SEPARATOR: &str = " · ";
const MODE_MANUAL_LABEL: &str = "Ask before edits";
const MODE_ACCEPT_EDITS_LABEL: &str = "Accept edits";
const MODE_PLAN_LABEL: &str = "Plan";
const MODE_AUTO_LABEL: &str = "Auto";
const MODE_DONT_ASK_LABEL: &str = "Don't ask";
const MODE_BYPASS_LABEL: &str = "Bypass permissions";
const VERDICT_ALLOWED: &str = "Allowed";
const VERDICT_ALLOWED_ALWAYS: &str = "Always allowed";
const VERDICT_DENIED: &str = "Denied";
const VERDICT_ANSWERED: &str = "Answered";
const VERDICT_PLAN_APPROVED: &str = "Plan approved";
const VERDICT_PLAN_REJECTED: &str = "Plan sent back";
const VERDICT_CANCELLED: &str = "Cancelled";

impl Choice {
    const fn label(self) -> &'static str {
        match self {
            Self::Allow => ALLOW_LABEL,
            Self::AllowAlways => ALLOW_ALWAYS_LABEL,
            Self::Deny => DENY_LABEL,
            Self::ApproveAcceptingEdits => ACCEPT_EDITS_LABEL,
            Self::ApproveAuto => AUTO_LABEL,
            Self::ApproveManually => MANUAL_LABEL,
            Self::KeepPlanning => KEEP_PLANNING_LABEL,
        }
    }

    const fn tray_label(self) -> &'static str {
        match self {
            Self::Allow => ALLOW_LABEL,
            Self::AllowAlways => ALLOW_ALWAYS_LABEL,
            Self::Deny => DENY_SHORT_LABEL,
            Self::ApproveAcceptingEdits => APPROVE_LABEL,
            Self::ApproveAuto => APPROVE_AUTO_LABEL,
            Self::ApproveManually => APPROVE_MANUALLY_LABEL,
            Self::KeepPlanning => KEEP_PLANNING_SHORT_LABEL,
        }
    }
}

pub(in crate::environment_shell) const fn mode_label(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Manual => MODE_MANUAL_LABEL,
        PermissionMode::AcceptEdits => MODE_ACCEPT_EDITS_LABEL,
        PermissionMode::Plan => MODE_PLAN_LABEL,
        PermissionMode::Auto => MODE_AUTO_LABEL,
        PermissionMode::DontAsk => MODE_DONT_ASK_LABEL,
        PermissionMode::BypassPermissions => MODE_BYPASS_LABEL,
    }
}

const fn verdict_label(verdict: AskVerdict) -> &'static str {
    match verdict {
        AskVerdict::Allowed => VERDICT_ALLOWED,
        AskVerdict::AllowedAlways => VERDICT_ALLOWED_ALWAYS,
        AskVerdict::Denied => VERDICT_DENIED,
        AskVerdict::Answered => VERDICT_ANSWERED,
        AskVerdict::PlanApproved(_) => VERDICT_PLAN_APPROVED,
        AskVerdict::PlanRejected => VERDICT_PLAN_REJECTED,
        AskVerdict::Cancelled => VERDICT_CANCELLED,
    }
}

pub(in crate::environment_shell) fn ask_line(outcome: &AskOutcome, pal: &Palette) -> AnyElement {
    let mut text = verdict_label(outcome.verdict).to_owned();
    if let AskVerdict::PlanApproved(mode) = outcome.verdict {
        text.push_str(VERDICT_SEPARATOR);
        text.push_str(mode_label(mode));
    }
    if !outcome.subject.is_empty() {
        text.push_str(VERDICT_SEPARATOR);
        text.push_str(&outcome.subject);
    }
    let glyph = match outcome.verdict {
        AskVerdict::Allowed
        | AskVerdict::AllowedAlways
        | AskVerdict::Answered
        | AskVerdict::PlanApproved(_) => IconName::CheckCircle,
        AskVerdict::Denied | AskVerdict::PlanRejected | AskVerdict::Cancelled => {
            IconName::CancelCircle
        }
    };
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_1_5))
        .min_w_0()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_muted))
        .child(icon(glyph, ICON_XS, pal.text_muted))
        .child(div().truncate().child(text))
        .into_any_element()
}

fn owned_id(prefix: &str, owner: AskOwner, index: usize) -> ElementId {
    let owner = match owner {
        AskOwner::Chat => CHAT_KEY.to_owned(),
        AskOwner::Surface(id) => id.as_u64().to_string(),
    };
    ElementId::Name(SharedString::from(format!("{prefix}-{owner}-{index}")))
}

fn number_key(event: &KeyDownEvent) -> Option<usize> {
    let stroke = &event.keystroke;
    if stroke.modifiers.modified() {
        return None;
    }
    stroke
        .key
        .parse::<usize>()
        .ok()
        .filter(|number| NUMBER_KEYS.contains(number))
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn ask_tray(
        &self,
        owner: AskOwner,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let queue = self.ask_queue(owner)?;
        let waiting = queue.front()?;
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let (question, detail) = match &waiting.ask.kind {
            AskKind::Questions(questions) => {
                return Some(Self::question_tray(owner, questions, queue, cx));
            }
            AskKind::Permission {
                tool,
                summary,
                title,
                edit,
            } => {
                let detail = match edit {
                    Some(edit) => edit_diff(edit, &mono, pal),
                    None => div()
                        .id(owned_id(DETAIL_PREFIX, owner, 0))
                        .max_h(px(DETAIL_MAX_H))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(SPACE_1))
                        .child(
                            div()
                                .font_family(mono)
                                .text_color(rgb(pal.text_strong))
                                .child(summary.clone()),
                        )
                        .children(title.clone().map(|title| {
                            div()
                                .text_size(px(TEXT_XS))
                                .line_height(px(LINE_XS))
                                .text_color(rgb(pal.text_muted))
                                .child(title)
                        }))
                        .into_any_element(),
                };
                (tool_question(tool), detail)
            }
            AskKind::Plan { plan } => (
                PLAN_QUESTION.into(),
                div()
                    .id(owned_id(PLAN_PREFIX, owner, 0))
                    .max_h(px(PLAN_MAX_H))
                    .overflow_y_scroll()
                    .child(
                        TextView::markdown(owned_id(PLAN_PREFIX, owner, 1), plan.clone())
                            .selectable(true),
                    )
                    .into_any_element(),
            ),
        };
        let count = queue.waiting.len();
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(icon(IconName::Lock, ICON, pal.text_muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(pal.text_default))
                    .child(question),
            )
            .when(count > 1, |line| {
                line.child(
                    div()
                        .flex_none()
                        .text_size(px(TEXT_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(format!("1{COUNT_SEPARATOR}{count}")),
                )
            })
            .children(Self::tray_buttons(owner, waiting, cx));
        let note = Self::note_box(owner, queue, cx);
        Some(
            Self::ask_keys(owner, queue, tray(pal), cx)
                .child(header)
                .child(div().h(px(DIVIDER_H)).bg(pal.divider))
                .child(detail)
                .children(note)
                .into_any_element(),
        )
    }

    fn ask_keys(
        owner: AskOwner,
        queue: &AskQueue,
        shell: Div,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<Div> {
        let shell = shell
            .id(owned_id(CARD_PREFIX, owner, 0))
            .key_context(ASK_KEY_CONTEXT)
            .on_action(
                cx.listener(move |this: &mut Self, _: &AskConfirm, window, cx| {
                    this.confirm_ask(owner, window, cx);
                }),
            )
            .on_action(cx.listener(move |this: &mut Self, _: &AskDeny, _, cx| {
                this.deny_and_interrupt(owner, cx);
            }))
            .on_key_down(
                cx.listener(move |this: &mut Self, event: &KeyDownEvent, window, cx| {
                    let noting = this
                        .ask_queue(owner)
                        .is_some_and(|queue| queue.note.is_some());
                    if let (false, Some(number)) = (noting, number_key(event)) {
                        this.choose_number(owner, number, window, cx);
                        cx.stop_propagation();
                    }
                }),
            );
        let Some(focus) = queue.focus.clone() else {
            return shell;
        };
        shell
            .track_focus(&focus)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                if !focus.contains_focused(window, cx) {
                    window.focus(&focus, cx);
                }
            })
    }

    fn tray_buttons(owner: AskOwner, waiting: &Waiting, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let pal = palette(cx);
        let choices = Choice::of_waiting(waiting);
        let last = choices.len().saturating_sub(1);
        choices
            .into_iter()
            .enumerate()
            .rev()
            .map(|(index, choice)| {
                let key = format!("{} ({})", choice.label(), index + 1);
                tray_button(owned_id(CHOICE_PREFIX, owner, index), index == 0, pal)
                    .when(index == last && last > 0, |button| button.ml(px(SPACE_1)))
                    .tooltip(move |_, cx| tooltip_view(key.clone().into(), cx))
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        this.choose(owner, choice, window, cx);
                    }))
                    .child(choice.tray_label())
                    .into_any_element()
            })
            .collect()
    }

    fn question_tray(
        owner: AskOwner,
        questions: &[Question],
        queue: &AskQueue,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let count = questions.len();
        let step = queue.picks.step(count);
        let Some(question) = questions.get(step) else {
            return div().into_any_element();
        };
        let mut rows: Vec<AnyElement> = offered(question)
            .enumerate()
            .map(|(index, option)| {
                let row = OptionRow {
                    owner,
                    step,
                    index,
                    multi: question.multi_select,
                    on: queue.picks.is_picked(&question.text, &option.label),
                };
                Self::option_row(row, &question.text, option, cx)
            })
            .collect();
        let own = OptionRow {
            owner,
            step,
            index: rows.len(),
            multi: question.multi_select,
            on: queue.picks.own_answer(question).is_some(),
        };
        rows.push(Self::own_answer_row(own, question, queue, cx));
        let last = step + 1 >= count;
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(icon(IconName::Chat, ICON, pal.text_muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(rgb(pal.text_default))
                    .child(question.text.clone()),
            )
            .when(count > 1, |line| {
                line.child(
                    div()
                        .flex_none()
                        .text_size(px(TEXT_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(format!("{}{COUNT_SEPARATOR}{count}", step + 1)),
                )
            })
            .child(
                tray_button(owned_id(DENY_PREFIX, owner, 0), false, pal)
                    .tooltip(|_, cx| tooltip_view(SKIP_TOOLTIP.into(), cx))
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        this.open_note(owner, NotePurpose::DenyReason, window, cx);
                    }))
                    .child(SKIP_LABEL),
            )
            .when(step > 0, |line| {
                line.child(
                    tray_button(owned_id(PREVIOUS_PREFIX, owner, 0), false, pal)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.show_question(owner, step.saturating_sub(1), cx);
                        }))
                        .child(PREVIOUS_LABEL),
                )
            })
            .child(
                tray_button(owned_id(SUBMIT_PREFIX, owner, 0), true, pal)
                    .ml(px(SPACE_1))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.next_question(owner, cx);
                    }))
                    .child(if last { SUBMIT_LABEL } else { NEXT_LABEL }),
            );
        let shown = Animation::new(STEP_ENTER).with_easing(ease_out_quint());
        let note = Self::note_box(owner, queue, cx);
        Self::ask_keys(owner, queue, tray(pal), cx)
            .child(header)
            .child(div().h(px(DIVIDER_H)).bg(pal.divider))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_0_5))
                    .children(rows)
                    .with_animation(owned_id(STEP_PREFIX, owner, step), shown, Styled::opacity),
            )
            .children(note)
            .into_any_element()
    }

    fn note_box(owner: AskOwner, queue: &AskQueue, cx: &mut Context<Self>) -> Option<AnyElement> {
        let note = queue.note.as_ref()?;
        let pal = palette(cx);
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(SPACE_1))
                .p(px(SPACE_2))
                .rounded(px(RADIUS_XS))
                .bg(rgb(pal.bg_surface))
                .child(Textarea::new(&note.input).appearance(false))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .text_size(px(TEXT_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(div().flex_1().child(NOTE_HINT))
                        .child(
                            Button::new(owned_id(NOTE_SEND_PREFIX, owner, 0))
                                .label(SEND_LABEL)
                                .small()
                                .variant(ButtonVariant::Primary)
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.submit_note(owner, cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    fn option_row(
        row: OptionRow,
        question: &str,
        option: &QuestionOption,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let owner = row.owner;
        let question = question.to_owned();
        let label = option.label.clone();
        option_shell(row, OPTION_PREFIX, pal)
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.pick_option(owner, &question, &label, cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(option.label.clone())
                    .children(option.description.clone().map(|description| {
                        div()
                            .text_size(px(TEXT_XS))
                            .line_height(px(LINE_XS))
                            .text_color(rgb(pal.text_muted))
                            .child(description)
                    })),
            )
            .child(option_mark(row, pal))
            .into_any_element()
    }

    fn own_answer_row(
        row: OptionRow,
        question: &Question,
        queue: &AskQueue,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let owner = row.owner;
        let own = queue.picks.own_answer(question).map(str::to_owned);
        let asked = question.text.clone();
        option_shell(row, OTHER_PREFIX, pal)
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                let purpose = NotePurpose::OtherAnswer {
                    question: asked.clone(),
                };
                this.open_note(owner, purpose, window, cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(OTHER_LABEL)
                    .child(
                        div()
                            .text_size(px(TEXT_XS))
                            .line_height(px(LINE_XS))
                            .text_color(rgb(if own.is_some() {
                                pal.text_default
                            } else {
                                pal.text_muted
                            }))
                            .child(own.unwrap_or_else(|| OTHER_PLACEHOLDER.to_owned())),
                    ),
            )
            .child(option_mark(row, pal))
            .into_any_element()
    }

    pub(in crate::environment_shell) fn mode_control(
        &self,
        owner: AskOwner,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let current = self.permission_mode(owner);
        let label = mode_label(current.unwrap_or(PermissionMode::Manual));
        let entries: Vec<MenuEntry> = std::iter::once(MenuEntry::Header(MODE_HEADER.to_owned()))
            .chain(OFFERED_MODES.into_iter().map(|mode| {
                MenuEntry::checked(
                    mode_label(mode).to_owned(),
                    current == Some(mode),
                    MenuAction::SetPermissionMode(owner, mode),
                )
            }))
            .collect();
        let entries = std::rc::Rc::new(entries);
        div()
            .id(owned_id(MODE_PREFIX, owner, 0))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(MODE_CONTROL_H))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_MD))
            .cursor_pointer()
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .text_color(rgb(pal.text_default))
            .hover(|style| style.bg(pal.bg_hover))
            .tooltip(|_, cx| tooltip_view(TOOLTIP_MODE.into(), cx))
            .on_click(
                cx.listener(move |this: &mut Self, event: &ClickEvent, _, cx| {
                    let position = event.position();
                    this.open_menu(
                        (position.x.as_f32(), position.y.as_f32()),
                        MenuPlacement::Above,
                        entries.to_vec(),
                        cx,
                    );
                }),
            )
            .child(icon(IconName::Lock, ICON_XS, pal.text_muted))
            .child(label)
            .child(icon(IconName::ChevronDown, ICON_XS, pal.text_subtle))
            .into_any_element()
    }
}

#[derive(Clone, Copy)]
struct OptionRow {
    owner: AskOwner,
    step: usize,
    index: usize,
    multi: bool,
    on: bool,
}

fn option_shell(row: OptionRow, prefix: &str, pal: &Palette) -> gpui_kit::Stateful<Div> {
    div()
        .id(owned_id(
            prefix,
            row.owner,
            row.step * ROWS_PER_STEP + row.index,
        ))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_3))
        .px(px(SPACE_2))
        .py(px(SPACE_1_5))
        .rounded(px(RADIUS_SM))
        .cursor_pointer()
        .when(row.on, |option| option.bg(pal.bg_selected))
        .when(!row.on, |option| {
            option.hover(|style| style.bg(pal.bg_hover))
        })
}

fn option_mark(row: OptionRow, pal: &Palette) -> Div {
    if row.multi {
        return check_box(row.on, pal);
    }
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(NUMBER_BADGE))
        .rounded(px(RADIUS_XS / 2.0))
        .text_size(px(TEXT_2XS))
        .line_height(px(LINE_2XS))
        .when(row.on, |badge| {
            badge.bg(rgb(pal.accent)).text_color(rgb(pal.on_accent))
        })
        .when(!row.on, |badge| {
            badge
                .bg(rgb(pal.bg_panel))
                .text_color(rgb(pal.text_default))
        })
        .child((row.index + 1).to_string())
}

pub(in crate::environment_shell) fn tray(pal: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .gap(px(SPACE_2))
        .mb(px(-TRAY_TUCK))
        .px(px(SPACE_3))
        .pt(px(SPACE_2))
        .pb(px(TRAY_TUCK + SPACE_2))
        .rounded_t(px(RADIUS_LG))
        .bg(rgb(pal.bg_panel))
        .border_1()
        .border_color(pal.divider)
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .text_color(rgb(pal.text_strong))
}

pub(in crate::environment_shell) fn tray_button(
    id: ElementId,
    primary: bool,
    pal: &Palette,
) -> gpui_kit::Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .h(px(TRAY_BUTTON_H))
        .px(px(if primary { SPACE_3 } else { SPACE_2 }))
        .rounded(px(RADIUS_MD))
        .cursor_pointer()
        .when(primary, |button| {
            button
                .bg(rgb(pal.text_strong))
                .text_color(rgb(pal.bg_surface))
                .font_weight(FontWeight::MEDIUM)
                .hover(|style| style.opacity(PRIMARY_HOVER_OPACITY))
        })
        .when(!primary, |button| {
            button
                .text_color(rgb(pal.text_muted))
                .hover(|style| style.text_color(rgb(pal.text_strong)))
        })
}
