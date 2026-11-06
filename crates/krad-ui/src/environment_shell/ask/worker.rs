use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement,
    Styled, Window,
};
use krad_chat::AskKind;
use krad_domain::{TerminalId, ToolKind};
use std::borrow::Cow;

use super::super::surface::{names_its_tool, tool_request};
use super::super::EnvironmentShell;
use super::view::owned_id;
use super::{subject, tray_button, AskQueue, Choice};
use crate::components::{icon, IconName};
use crate::theme::{palette, ICON_SM, LINE_XS, SPACE_1, SPACE_1_5, TEXT_XS};

const CHOICE_PREFIX: &str = "worker-ask-choice";
const ANSWER_IN_SESSION_LABEL: &str = "Answer in session";
const PLAN_REQUEST: &str = "Approve the plan";
const PLAN_SEPARATOR: &str = ": ";
const QUESTION_MARK: &str = "?";
const NO_MARK: &str = "";
const MORE_PREFIX: &str = "+";
const MORE_SUFFIX: &str = " more";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardButton {
    Answer(Choice),
    Note(Choice),
    InSession,
}

impl From<Choice> for CardButton {
    fn from(choice: Choice) -> Self {
        if choice.note().is_some() {
            Self::Note(choice)
        } else {
            Self::Answer(choice)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CardChoice {
    button: CardButton,
    label: String,
    rejects: bool,
}

impl CardChoice {
    fn of(choice: Choice, kind: &AskKind) -> Self {
        Self {
            button: choice.into(),
            label: choice.label(kind).to_owned(),
            rejects: choice.rejects(kind),
        }
    }

    fn in_session() -> Self {
        Self {
            button: CardButton::InSession,
            label: ANSWER_IN_SESSION_LABEL.to_owned(),
            rejects: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Phrase {
    request: Cow<'static, str>,
    code: Option<String>,
    mark: &'static str,
}

impl Phrase {
    fn of(kind: &AskKind) -> Self {
        match kind {
            AskKind::Permission {
                tool,
                kind: tool_kind,
                summary,
                ..
            } => Self::tool(tool, *tool_kind, summary),
            AskKind::AcpPermission { tool, .. } => Self::tool(&tool.name, tool.kind, &tool.summary),
            AskKind::Questions(_) => Self {
                request: Cow::Owned(subject(kind)),
                code: None,
                mark: NO_MARK,
            },
            AskKind::Plan { .. } => {
                let heading = subject(kind);
                let request = if heading.is_empty() {
                    Cow::Borrowed(PLAN_REQUEST)
                } else {
                    Cow::Owned(format!("{PLAN_REQUEST}{PLAN_SEPARATOR}{heading}"))
                };
                Self {
                    request,
                    code: None,
                    mark: NO_MARK,
                }
            }
        }
    }

    fn tool(name: &str, kind: ToolKind, summary: &str) -> Self {
        let named = (!names_its_tool(kind)).then_some(name.trim());
        let code = Some(summary.trim())
            .filter(|summary| !summary.is_empty())
            .or(named)
            .filter(|code| !code.is_empty())
            .map(str::to_owned);
        Self {
            request: tool_request(name, kind),
            code,
            mark: QUESTION_MARK,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct WorkerAsk {
    worker: TerminalId,
    phrase: Phrase,
    choices: Vec<CardChoice>,
    more: usize,
}

impl WorkerAsk {
    fn of(worker: TerminalId, queue: &AskQueue) -> Option<Self> {
        let waiting = queue.front()?;
        let kind = &waiting.ask.kind;
        let mut choices: Vec<CardChoice> = match kind {
            AskKind::Permission { .. } | AskKind::AcpPermission { .. } => {
                Choice::of_waiting(waiting)
                    .into_iter()
                    .map(|choice| CardChoice::of(choice, kind))
                    .collect()
            }
            AskKind::Questions(_) | AskKind::Plan { .. } => Vec::new(),
        };
        if choices.is_empty() {
            choices.push(CardChoice::in_session());
        }
        Some(Self {
            worker,
            phrase: Phrase::of(kind),
            choices,
            more: queue.waiting.len().saturating_sub(1),
        })
    }

    fn more_label(&self) -> Option<String> {
        (self.more > 0).then(|| format!("{MORE_PREFIX}{}{MORE_SUFFIX}", self.more))
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn worker_ask(&self, worker: TerminalId) -> Option<WorkerAsk> {
        WorkerAsk::of(worker, self.ask_queue(worker)?)
    }

    pub(in crate::environment_shell) fn worker_ask_row(
        ask: &WorkerAsk,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let worker = ask.worker;
        let phrase = div()
            .flex()
            .flex_row()
            .items_center()
            .flex_1()
            .min_w_0()
            .text_color(rgb(pal.text_default))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(ask.phrase.request.to_string()),
            )
            .children(ask.phrase.code.clone().map(|code| {
                div()
                    .min_w_0()
                    .truncate()
                    .ml(px(SPACE_1))
                    .font_family(mono)
                    .text_color(rgb(pal.text_strong))
                    .child(code)
            }))
            .child(div().flex_none().child(ask.phrase.mark));
        let more = ask.more_label().map(|label| {
            div()
                .flex_none()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_muted))
                .child(label)
        });
        let buttons = ask.choices.iter().enumerate().rev().map(|(index, choice)| {
            let button = choice.button;
            tray_button(owned_id(CHOICE_PREFIX, worker, index), index == 0, pal)
                .when(choice.rejects, |element| {
                    element.text_color(rgb(pal.error_fg))
                })
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    cx.stop_propagation();
                    this.answer_from_card(worker, button, window, cx);
                }))
                .child(choice.label.clone())
                .into_any_element()
        });
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .child(icon(IconName::Lock, ICON_SM, pal.text_muted))
            .child(phrase)
            .children(more)
            .children(buttons)
            .into_any_element()
    }

    fn answer_from_card(
        &mut self,
        worker: TerminalId,
        button: CardButton,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match button {
            CardButton::Answer(choice) => self.choose(worker, choice, window, cx),
            CardButton::Note(choice) => {
                self.go_to_worker(worker, cx);
                self.choose(worker, choice, window, cx);
            }
            CardButton::InSession => self.go_to_worker(worker, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{AskReply, Waiting};
    use super::*;
    use krad_chat::{Question, ToolAsk};
    use krad_domain::{AcpPermissionKind, AcpPermissionOption, AcpPermissionOptionId, ToolCall};

    const WORKER: TerminalId = TerminalId(7);

    fn waiting(kind: AskKind) -> Waiting {
        let mut ask = ToolAsk::held(String::new(), <_>::default(), Vec::new());
        ask.kind = kind;
        Waiting::held(ask, AskReply::Keys, String::new(), None)
    }

    fn front(kinds: impl IntoIterator<Item = AskKind>) -> Option<WorkerAsk> {
        let mut queue = AskQueue::default();
        kinds.into_iter().for_each(|kind| queue.push(waiting(kind)));
        WorkerAsk::of(WORKER, &queue)
    }

    fn edit(path: &str) -> AskKind {
        AskKind::Permission {
            tool: "Edit".to_owned(),
            kind: ToolKind::Edit,
            summary: path.to_owned(),
            title: None,
            edit: None,
        }
    }

    fn buttons(ask: Option<&WorkerAsk>) -> Vec<CardButton> {
        ask.map_or_default(|ask| ask.choices.iter().map(|choice| choice.button).collect())
    }

    #[test]
    fn a_permission_reads_as_its_tool_and_path_and_offers_the_tray_choices() {
        let ask = front([edit("src/app/page.tsx")]);
        assert_eq!(
            ask.as_ref().map(|ask| &ask.phrase),
            Some(&Phrase {
                request: Cow::Borrowed("Allow edit to"),
                code: Some("src/app/page.tsx".to_owned()),
                mark: QUESTION_MARK,
            })
        );
        assert_eq!(
            buttons(ask.as_ref()),
            [
                CardButton::Answer(Choice::Allow),
                CardButton::Note(Choice::Deny)
            ],
            "denying takes a note, so it opens the Worker's own tray"
        );
        assert_eq!(ask.and_then(|ask| ask.more_label()), None);
    }

    #[test]
    fn acp_options_keep_the_agents_own_names_allowing_first() {
        let option = |id: &str, kind| AcpPermissionOption {
            id: AcpPermissionOptionId(id.to_owned()),
            name: id.to_owned(),
            kind,
        };
        let ask = front([AskKind::AcpPermission {
            tool: ToolCall {
                id: None,
                name: "cargo test".to_owned(),
                kind: ToolKind::Shell,
                summary: String::new(),
                edits: Vec::new(),
            },
            options: vec![
                option("Reject", AcpPermissionKind::RejectOnce),
                option("Allow", AcpPermissionKind::AllowOnce),
            ],
        }]);
        assert_eq!(
            ask.as_ref()
                .map(|ask| (ask.phrase.request.as_ref(), ask.phrase.code.as_deref())),
            Some(("Allow running", Some("cargo test")))
        );
        assert_eq!(
            buttons(ask.as_ref()),
            [
                CardButton::Answer(Choice::AcpOption(1)),
                CardButton::Answer(Choice::AcpOption(0))
            ]
        );
        let shown: Vec<(&str, bool)> = ask.as_ref().map_or_default(|ask| {
            ask.choices
                .iter()
                .map(|choice| (choice.label.as_str(), choice.rejects))
                .collect()
        });
        assert_eq!(shown, [("Allow", false), ("Reject", true)]);
    }

    #[test]
    fn asks_that_need_typing_send_the_user_to_the_workers_session() {
        let question = AskKind::Questions(vec![Question {
            text: "Which database?".to_owned(),
            header: None,
            options: Vec::new(),
            multi_select: false,
        }]);
        let plan = AskKind::Plan {
            plan: "# Add the notes file".to_owned(),
        };
        let asked = front([question]);
        assert_eq!(
            asked.as_ref().map(|ask| ask.phrase.request.as_ref()),
            Some("Which database?")
        );
        assert_eq!(buttons(asked.as_ref()), [CardButton::InSession]);
        let planned = front([plan]);
        assert_eq!(
            planned.as_ref().map(|ask| ask.phrase.request.as_ref()),
            Some("Approve the plan: Add the notes file")
        );
        assert_eq!(buttons(planned.as_ref()), [CardButton::InSession]);
    }

    #[test]
    fn queued_asks_behind_the_front_one_are_counted() {
        let ask = front([edit("a.rs"), edit("b.rs"), edit("c.rs")]);
        assert_eq!(
            ask.as_ref().and_then(|ask| ask.phrase.code.as_deref()),
            Some("a.rs")
        );
        assert_eq!(
            ask.and_then(|ask| ask.more_label()).as_deref(),
            Some("+2 more")
        );
        assert_eq!(front([]), None);
    }

    #[test]
    fn the_card_answers_the_worker_that_asked() {
        assert_eq!(front([edit("a.rs")]).map(|ask| ask.worker), Some(WORKER));
    }
}
