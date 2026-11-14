use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, Entity, ParentElement, Styled,
    Subscription, Window,
};
use krad_client::RuntimeClient;
use krad_domain::{PromptAsker, PromptKind};
use krad_protocol::{Command, HostCommand, HostPrompt, PromptAnswer};
use std::sync::Arc;

use super::super::dialog_parts::{sheet_footer, sheet_overlay, sheet_panel, sheet_title, Dismiss};
use super::super::EnvironmentShell;
use super::HostDialog;
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, LINE_SM, SPACE_2, SPACE_5, SPACE_6, TEXT_SM};

const DIALOG_W: f32 = 460.0;
const TITLE: &str = "Sign in to";
const UNKNOWN_TITLE: &str = "Sign in";
const GIT_TITLE: &str = "Git asks you to sign in";
const SECRET_PLACEHOLDER: &str = "Type it here; it never leaves this computer except to ssh";
const GIT_SECRET_PLACEHOLDER: &str = "Type it here; only git gets it and nothing keeps it";
const SIGN_IN_LABEL: &str = "Sign in";
const TRUST_LABEL: &str = "Trust and connect";
const CANCEL_LABEL: &str = "Cancel";
const TRUSTED_ANSWER: &str = "yes";
const BTN_CLOSE: &str = "sign-in-close";
const BTN_CANCEL: &str = "sign-in-cancel";
const BTN_ANSWER: &str = "sign-in-answer";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum SignInPlace {
    Shell,
    Settings,
}

pub(in crate::environment_shell) struct PendingPrompt {
    prompt: Option<HostPrompt>,
    runtime: Arc<dyn RuntimeClient>,
}

impl PendingPrompt {
    pub(in crate::environment_shell) fn new(
        prompt: HostPrompt,
        runtime: Arc<dyn RuntimeClient>,
    ) -> Self {
        Self {
            prompt: Some(prompt),
            runtime,
        }
    }

    pub(in crate::environment_shell) fn answer(&mut self, typed: Option<String>) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        let answer = typed.map(|typed| answer_of(prompt.kind, typed));
        self.runtime.post(Command::Host(HostCommand::AnswerPrompt {
            prompt: prompt.prompt,
            answer,
        }));
    }
}

impl Drop for PendingPrompt {
    fn drop(&mut self) {
        self.answer(None);
    }
}

fn answer_of(kind: PromptKind, typed: String) -> PromptAnswer {
    match kind {
        PromptKind::Secret => PromptAnswer::new(typed),
        PromptKind::Confirm => PromptAnswer::new(TRUSTED_ANSWER.to_owned()),
    }
}

pub(in crate::environment_shell) struct SignInDialog {
    pending: PendingPrompt,
    field: Entity<InputState>,
    _events: Subscription,
}

fn dismiss_in_shell(
    this: &mut EnvironmentShell,
    _: &mut Window,
    cx: &mut Context<EnvironmentShell>,
) {
    this.answer_sign_in(SignInPlace::Shell, false, cx);
}

fn dismiss_in_settings(
    this: &mut EnvironmentShell,
    _: &mut Window,
    cx: &mut Context<EnvironmentShell>,
) {
    this.answer_sign_in(SignInPlace::Settings, false, cx);
}

const fn dismiss_of(place: SignInPlace) -> Dismiss {
    match place {
        SignInPlace::Shell => dismiss_in_shell,
        SignInPlace::Settings => dismiss_in_settings,
    }
}

const fn placeholder_of(asker: &PromptAsker) -> &'static str {
    match asker {
        PromptAsker::Git => GIT_SECRET_PLACEHOLDER,
        PromptAsker::Host { .. } | PromptAsker::Runner { .. } | PromptAsker::Unknown => {
            SECRET_PLACEHOLDER
        }
    }
}

impl EnvironmentShell {
    fn sign_in_title(&self, asker: &PromptAsker) -> String {
        match asker {
            PromptAsker::Host { destination } => format!("{TITLE} {destination}"),
            PromptAsker::Runner { runner } => format!("{TITLE} {}", self.runner_name(*runner)),
            PromptAsker::Git => GIT_TITLE.to_owned(),
            PromptAsker::Unknown => UNKNOWN_TITLE.to_owned(),
        }
    }

    pub(in crate::environment_shell) fn sign_in_dialog(
        &self,
        prompt: HostPrompt,
        place: SignInPlace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> SignInDialog {
        let masked = prompt.kind == PromptKind::Secret;
        let placeholder = placeholder_of(&prompt.asker);
        let field = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .masked(masked)
        });
        let focus_target = field.clone();
        cx.defer_in(window, move |_, window, cx| {
            focus_target.update(cx, |field, cx| field.focus(window, cx));
        });
        let events = cx.subscribe_in(
            &field,
            window,
            move |this: &mut Self, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.answer_sign_in(place, true, cx);
                }
            },
        );
        SignInDialog {
            pending: PendingPrompt::new(prompt, Arc::clone(&self.runtime.client)),
            field,
            _events: events,
        }
    }

    pub(in crate::environment_shell) fn show_waiting_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.hosts.dialog, HostDialog::Closed) {
            return;
        }
        let Some(prompt) = self
            .runtime
            .view
            .update(cx, |model, _| model.hosts.next_prompt())
        else {
            return;
        };
        let dialog = self.sign_in_dialog(prompt, SignInPlace::Shell, window, cx);
        self.hosts.dialog = HostDialog::SignIn(dialog);
    }

    fn sign_in_dialog_at(&mut self, place: SignInPlace) -> Option<&mut SignInDialog> {
        match place {
            SignInPlace::Shell => match &mut self.hosts.dialog {
                HostDialog::SignIn(dialog) => Some(dialog),
                _ => None,
            },
            SignInPlace::Settings => self.runner_sign_in_dialog(),
        }
    }

    fn answer_sign_in(&mut self, place: SignInPlace, given: bool, cx: &mut Context<Self>) {
        let Some(dialog) = self.sign_in_dialog_at(place) else {
            return;
        };
        let typed = given.then(|| dialog.field.read(cx).value().to_string());
        dialog.pending.answer(typed);
        match place {
            SignInPlace::Shell => self.hosts.dialog = HostDialog::Closed,
            SignInPlace::Settings => self.close_runner_sign_in(),
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn sign_in_sheet(
        &self,
        dialog: &SignInDialog,
        place: SignInPlace,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let prompt = dialog.pending.prompt.as_ref()?;
        let pal = palette(cx);
        let asks_secret = prompt.kind == PromptKind::Secret;
        let body = div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .px(px(SPACE_6))
            .pb(px(SPACE_5))
            .child(
                div()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_default))
                    .child(prompt.text.trim().to_owned()),
            )
            .when(asks_secret, |body| body.child(Input::new(&dialog.field)));
        let answer_label = if asks_secret {
            SIGN_IN_LABEL
        } else {
            TRUST_LABEL
        };
        let dismiss = dismiss_of(place);
        let sheet = sheet_panel(DIALOG_W, pal)
            .child(sheet_title(
                self.sign_in_title(&prompt.asker),
                None,
                BTN_CLOSE,
                dismiss,
                cx,
            ))
            .child(body)
            .child(
                sheet_footer(pal)
                    .justify_end()
                    .child(
                        Button::new(BTN_CANCEL)
                            .label(CANCEL_LABEL)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.answer_sign_in(place, false, cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_ANSWER)
                            .label(answer_label)
                            .variant(ButtonVariant::Primary)
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.answer_sign_in(place, true, cx);
                            })),
                    ),
            );
        Some(sheet_overlay(sheet, dismiss, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_client::RecordedClient;
    use krad_domain::CloudRunnerId;
    use krad_protocol::PromptId;

    const PASSWORD: &str = "correct horse";

    fn runner_prompt(kind: PromptKind) -> HostPrompt {
        HostPrompt {
            prompt: PromptId(4),
            asker: PromptAsker::Runner {
                runner: CloudRunnerId(2),
            },
            kind,
            text: "me@gpu's password: ".to_owned(),
        }
    }

    fn answered(command: &Command) -> Option<(PromptId, Option<String>)> {
        match command {
            Command::Host(HostCommand::AnswerPrompt { prompt, answer }) => {
                Some((*prompt, answer.clone().map(PromptAnswer::into_text)))
            }
            _ => None,
        }
    }

    #[test]
    fn submitting_answers_the_prompt_once_and_closing_unanswered_cancels_it() {
        let client = RecordedClient::new(Vec::new());
        let mut pending = PendingPrompt::new(runner_prompt(PromptKind::Secret), client.clone());
        pending.answer(Some(PASSWORD.to_owned()));
        pending.answer(Some(PASSWORD.to_owned()));
        drop(pending);
        drop(PendingPrompt::new(
            runner_prompt(PromptKind::Secret),
            client.clone(),
        ));
        let sent: Vec<_> = client.sent().iter().filter_map(answered).collect();
        assert_eq!(
            sent,
            [
                (PromptId(4), Some(PASSWORD.to_owned())),
                (PromptId(4), None)
            ]
        );
    }

    #[test]
    fn trusting_a_host_key_answers_yes_whatever_was_typed() {
        let client = RecordedClient::new(Vec::new());
        let mut pending = PendingPrompt::new(runner_prompt(PromptKind::Confirm), client.clone());
        pending.answer(Some(String::new()));
        let sent: Vec<_> = client.sent().iter().filter_map(answered).collect();
        assert_eq!(sent, [(PromptId(4), Some(TRUSTED_ANSWER.to_owned()))]);
    }

    #[test]
    fn an_answer_never_prints_its_secret() {
        let answer = answer_of(PromptKind::Secret, PASSWORD.to_owned());
        assert!(!format!("{answer:?}").contains(PASSWORD));
    }
}
