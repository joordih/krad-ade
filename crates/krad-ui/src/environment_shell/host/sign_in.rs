use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, Entity, ParentElement, Styled,
    Subscription, Window,
};
use krad_host::mux::{PromptAsker, PromptKind, PromptSender, Secret, SignInPrompt};
use std::collections::VecDeque;
use std::sync::mpsc;

use super::super::dialog_parts::{sheet_footer, sheet_overlay, sheet_panel, sheet_title};
use super::super::EnvironmentShell;
use super::HostDialog;
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, LINE_SM, SPACE_2, SPACE_5, SPACE_6, TEXT_SM};

const DIALOG_W: f32 = 460.0;
const TITLE: &str = "Sign in to";
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

pub(in crate::environment_shell) struct PromptInbox {
    sender: PromptSender,
    arriving: mpsc::Receiver<SignInPrompt>,
    waiting: VecDeque<SignInPrompt>,
}

impl Default for PromptInbox {
    fn default() -> Self {
        let (sender, arriving) = mpsc::channel();
        Self {
            sender,
            arriving,
            waiting: VecDeque::new(),
        }
    }
}

impl PromptInbox {
    pub(in crate::environment_shell) fn sender(&self) -> PromptSender {
        self.sender.clone()
    }

    fn collect(&mut self) -> bool {
        let before = self.waiting.len();
        self.waiting.extend(self.arriving.try_iter());
        self.waiting.len() != before
    }
}

pub(in crate::environment_shell) struct SignInDialog {
    prompt: Option<SignInPrompt>,
    field: Entity<InputState>,
    _events: Subscription,
}

impl Drop for SignInDialog {
    fn drop(&mut self) {
        if let Some(prompt) = self.prompt.take() {
            prompt.answer(None);
        }
    }
}

fn dismiss(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.answer_sign_in(false, cx);
}

fn title_of(asker: &PromptAsker) -> String {
    match asker {
        PromptAsker::Host(destination) => format!("{TITLE} {destination}"),
        PromptAsker::Git => GIT_TITLE.to_owned(),
    }
}

const fn placeholder_of(asker: &PromptAsker) -> &'static str {
    match asker {
        PromptAsker::Host(_) => SECRET_PLACEHOLDER,
        PromptAsker::Git => GIT_SECRET_PLACEHOLDER,
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn prompt_sender(&self) -> PromptSender {
        self.hosts.prompts.sender()
    }

    pub(in crate::environment_shell) fn collect_sign_in_prompts(&mut self, cx: &mut Context<Self>) {
        if self.hosts.prompts.collect() {
            cx.notify();
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
        let Some(prompt) = self.hosts.prompts.waiting.pop_front() else {
            return;
        };
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
            |this: &mut Self, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.answer_sign_in(true, cx);
                }
            },
        );
        self.hosts.dialog = HostDialog::SignIn(SignInDialog {
            prompt: Some(prompt),
            field,
            _events: events,
        });
    }

    fn answer_sign_in(&mut self, given: bool, cx: &mut Context<Self>) {
        let HostDialog::SignIn(dialog) = &mut self.hosts.dialog else {
            return;
        };
        let Some(prompt) = dialog.prompt.take() else {
            return;
        };
        let answer = given.then(|| match prompt.kind {
            PromptKind::Secret => Secret::new(dialog.field.read(cx).value().to_string()),
            PromptKind::Confirm => Secret::new(TRUSTED_ANSWER.to_owned()),
        });
        prompt.answer(answer);
        self.hosts.dialog = HostDialog::Closed;
        cx.notify();
    }

    pub(in crate::environment_shell) fn sign_in_sheet(
        dialog: &SignInDialog,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let prompt = dialog.prompt.as_ref()?;
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
        let sheet = sheet_panel(DIALOG_W, pal)
            .child(sheet_title(
                title_of(&prompt.asker),
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
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.answer_sign_in(false, cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_ANSWER)
                            .label(answer_label)
                            .variant(ButtonVariant::Primary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.answer_sign_in(true, cx);
                            })),
                    ),
            );
        Some(sheet_overlay(sheet, dismiss, cx))
    }
}
