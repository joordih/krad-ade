use gpui_kit::component::input::Textarea;
use gpui_kit::component::text::TextView;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, ease_out_quint, px, rgb, Animation, AnimationExt as _, AnyElement, ClickEvent, Context,
    Div, ElementId, FontWeight, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use krad_chat::{AskKind, Question, QuestionOption, ToolAsk};
use krad_domain::{AgentPreset, AskOutcome, AskVerdict, PermissionMode};
use std::time::Duration;

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::surface::{edit_diff, tool_icon};
use super::super::EnvironmentShell;
use super::picks::{offered, OTHER_LABEL};
use super::{AskOwner, AskQueue, Choice, NotePurpose, Waiting, OFFERED_MODES};
use crate::actions::{AskConfirm, AskDeny, ASK_KEY_CONTEXT};
use crate::components::{check_box, icon, tooltip_view, Button, ButtonVariant, IconName};
use crate::theme::{
    card_shadow, palette, Palette, ICON_SM, ICON_XS, LINE_2XS, LINE_SM, LINE_XS, RADIUS_LG,
    RADIUS_MD, RADIUS_SM, RADIUS_XL, RADIUS_XS, SPACE_0_5, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3,
    TEXT_2XS, TEXT_SM, TEXT_XS,
};

const HARNESS: AgentPreset = AgentPreset::ClaudeCode;
const CARD_PAD: f32 = 14.0;
const RULE_W: f32 = 3.0;
const PLAN_MAX_H: f32 = 320.0;
const MODE_CONTROL_H: f32 = 24.0;
const NUMBER_BADGE: f32 = 18.0;
const STEP_TAB_H: f32 = 24.0;
const DISABLED_OPACITY: f32 = 0.5;
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
const STEP_TAB_PREFIX: &str = "ask-step-tab";
const PREVIOUS_PREFIX: &str = "ask-previous";
const MODE_PREFIX: &str = "ask-mode";
const CHAT_KEY: &str = "chat";

const PERMISSION_VERB: &str = "wants to use";
const PLAN_VERB: &str = "has a plan";
const COUNT_SEPARATOR: &str = " of ";
const ALLOW_LABEL: &str = "Allow";
const ALLOW_ALWAYS_LABEL: &str = "Allow, don't ask again";
const DENY_LABEL: &str = "Deny…";
const ACCEPT_EDITS_LABEL: &str = "Yes, and accept edits";
const AUTO_LABEL: &str = "Yes, and use auto mode";
const MANUAL_LABEL: &str = "Yes, approve edits by hand";
const KEEP_PLANNING_LABEL: &str = "No, keep planning…";
const OTHER_PLACEHOLDER: &str = "Enter your custom answer here";
const SKIP_TOOLTIP: &str = "Skip the questions";
const STEP_LABEL: &str = "Step";
const PREVIOUS_LABEL: &str = "Previous";
const NEXT_LABEL: &str = "Next";
const SUBMIT_LABEL: &str = "Submit";
const SEND_LABEL: &str = "Send";
const KEYS_HINT: &str = "Number keys pick, Enter confirms, Esc stops";
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
    pub(in crate::environment_shell) fn ask_panel(
        &self,
        owner: AskOwner,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let queue = self.ask_queue(owner)?;
        let waiting = queue.front()?;
        let pal = palette(cx);
        let noting = queue.note.is_some();
        let asks_questions = matches!(waiting.ask.kind, AskKind::Questions(_));
        let body = Self::ask_body(owner, &waiting.ask, queue, cx);
        let note = Self::note_box(owner, queue, cx);
        let buttons = Self::ask_buttons(owner, waiting, queue, cx);
        Some(
            Self::ask_card(owner, queue, asks_questions, cx)
                .child(body)
                .children(note)
                .child(buttons)
                .when(!noting, |card| {
                    card.child(
                        div()
                            .text_size(px(TEXT_XS))
                            .line_height(px(LINE_XS))
                            .text_color(rgb(pal.text_subtle))
                            .child(KEYS_HINT),
                    )
                })
                .into_any_element(),
        )
    }

    fn ask_card(
        owner: AskOwner,
        queue: &AskQueue,
        asks_questions: bool,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<Div> {
        let pal = palette(cx);
        let card = div()
            .id(owned_id(CARD_PREFIX, owner, 0))
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(SPACE_2))
            .p(px(CARD_PAD))
            .when(asks_questions, |card| {
                card.rounded(px(RADIUS_XL))
                    .bg(rgb(pal.bg_surface))
                    .border_1()
                    .border_color(pal.divider)
                    .shadow(card_shadow(pal))
            })
            .when(!asks_questions, |card| {
                card.rounded(px(RADIUS_MD))
                    .bg(rgb(pal.bg_panel))
                    .border_l(px(RULE_W))
                    .border_color(rgb(pal.attention))
            })
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_strong))
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
            return card;
        };
        card.track_focus(&focus)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx);
            })
    }

    fn ask_body(
        owner: AskOwner,
        ask: &ToolAsk,
        queue: &AskQueue,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let column = div().flex().flex_col().gap(px(SPACE_1_5));
        match &ask.kind {
            AskKind::Permission {
                tool,
                summary,
                title,
                edit,
            } => {
                let mono = cx.theme().mono_font_family.clone();
                column
                    .child(heading(
                        tool_icon(tool),
                        PERMISSION_VERB,
                        Some(tool),
                        queue,
                        pal,
                    ))
                    .children(title.clone().map(|title| div().child(title)))
                    .when(!summary.is_empty(), |body| {
                        body.child(
                            div()
                                .font_family(mono.clone())
                                .text_size(px(TEXT_XS))
                                .text_color(rgb(pal.text_default))
                                .child(summary.clone()),
                        )
                    })
                    .children(edit.as_ref().map(|edit| edit_diff(edit, &mono, pal)))
                    .into_any_element()
            }
            AskKind::Plan { plan } => column
                .child(heading(IconName::Note, PLAN_VERB, None, queue, pal))
                .child(
                    div()
                        .id(owned_id(PLAN_PREFIX, owner, 0))
                        .max_h(px(PLAN_MAX_H))
                        .overflow_y_scroll()
                        .child(
                            TextView::markdown(owned_id(PLAN_PREFIX, owner, 1), plan.clone())
                                .selectable(true),
                        ),
                )
                .into_any_element(),
            AskKind::Questions(questions) => Self::question_step(owner, questions, queue, cx),
        }
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

    fn ask_buttons(
        owner: AskOwner,
        waiting: &Waiting,
        queue: &AskQueue,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = div().flex().flex_row().flex_wrap().gap(px(SPACE_2));
        if let AskKind::Questions(questions) = &waiting.ask.kind {
            return Self::question_footer(owner, questions, queue, cx);
        }
        let choices = Choice::of_waiting(waiting);
        row.children(choices.into_iter().enumerate().map(|(index, choice)| {
            let number = index + 1;
            Button::new(owned_id(CHOICE_PREFIX, owner, index))
                .label(format!("{number}  {}", choice.label()))
                .small()
                .variant(if index == 0 {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Secondary
                })
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    this.choose(owner, choice, window, cx);
                }))
        }))
        .into_any_element()
    }

    fn question_step(
        owner: AskOwner,
        questions: &[Question],
        queue: &AskQueue,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let step = queue.picks.step(questions.len());
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
        let shown = Animation::new(STEP_ENTER).with_easing(ease_out_quint());
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pt(px(SPACE_0_5))
                            .font_weight(FontWeight::MEDIUM)
                            .child(question.text.clone()),
                    )
                    .when(queue.waiting.len() > 1, |line| {
                        line.child(
                            div()
                                .flex_none()
                                .pt(px(SPACE_0_5))
                                .text_size(px(TEXT_XS))
                                .text_color(rgb(pal.text_muted))
                                .child(format!("1{COUNT_SEPARATOR}{}", queue.waiting.len())),
                        )
                    })
                    .child(
                        Button::new(owned_id(DENY_PREFIX, owner, 0))
                            .icon(IconName::Close)
                            .small()
                            .tooltip(SKIP_TOOLTIP)
                            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                                this.open_note(owner, NotePurpose::DenyReason, window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_1_5))
                    .children(rows)
                    .with_animation(owned_id(STEP_PREFIX, owner, step), shown, Styled::opacity),
            )
            .into_any_element()
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

    fn question_footer(
        owner: AskOwner,
        questions: &[Question],
        queue: &AskQueue,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let count = questions.len();
        let step = queue.picks.step(count);
        let several = count > 1;
        let last = step + 1 >= count;
        let steps = questions.iter().enumerate().map(|(index, question)| {
            let current = index == step;
            let name = question
                .header
                .clone()
                .unwrap_or_else(|| format!("{STEP_LABEL} {}", index + 1));
            div()
                .id(owned_id(STEP_TAB_PREFIX, owner, index))
                .flex()
                .items_center()
                .h(px(STEP_TAB_H))
                .px(px(SPACE_2))
                .rounded(px(RADIUS_SM))
                .cursor_pointer()
                .text_size(px(TEXT_XS))
                .text_color(rgb(if current { pal.accent } else { pal.text_muted }))
                .when(current, |tab| tab.bg(pal.bg_selected))
                .when(!current, |tab| {
                    tab.hover(|style| style.text_color(rgb(pal.text_default)))
                })
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.show_question(owner, index, cx);
                }))
                .child(name)
        });
        let previous = Button::new(owned_id(PREVIOUS_PREFIX, owner, 0))
            .label(PREVIOUS_LABEL)
            .small()
            .variant(ButtonVariant::Secondary)
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.show_question(owner, step.saturating_sub(1), cx);
            }));
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1))
            .when(several, |footer| footer.children(steps))
            .child(div().flex_1())
            .when(several, |footer| {
                footer.child(
                    div()
                        .when(step == 0, |back| back.opacity(DISABLED_OPACITY).occlude())
                        .child(previous),
                )
            })
            .child(
                div().pl(px(SPACE_1)).child(
                    Button::new(owned_id(SUBMIT_PREFIX, owner, 0))
                        .label(if last { SUBMIT_LABEL } else { NEXT_LABEL })
                        .small()
                        .variant(ButtonVariant::Primary)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.next_question(owner, cx);
                        })),
                ),
            )
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
        .px(px(SPACE_3))
        .py(px(SPACE_2))
        .rounded(px(RADIUS_LG))
        .border_1()
        .cursor_pointer()
        .when(row.on, |option| {
            option.border_color(rgb(pal.accent)).bg(pal.bg_selected)
        })
        .when(!row.on, |option| {
            option
                .border_color(pal.divider)
                .hover(|style| style.bg(pal.bg_hover))
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

fn heading(
    glyph: IconName,
    verb: &str,
    tool: Option<&String>,
    queue: &AskQueue,
    pal: &Palette,
) -> Div {
    let count = queue.waiting.len();
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_1_5))
        .child(icon(glyph, ICON_SM, pal.attention_text))
        .child(
            div()
                .text_color(rgb(pal.text_muted))
                .child(format!("{} {verb}", HARNESS.display_name())),
        )
        .children(tool.map(|tool| div().font_weight(FontWeight::MEDIUM).child(tool.clone())))
        .child(div().flex_1())
        .when(count > 1, |line| {
            line.child(
                div()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(format!("1{COUNT_SEPARATOR}{count}")),
            )
        })
}
