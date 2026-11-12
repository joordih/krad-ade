use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement, Styled,
    Window,
};
use krad_domain::{ClaudeOauthToken, CloudRunnerId};

use super::super::super::dialog_parts::sheet_padded;
use super::super::sections::card_field;
use super::super::{form_row, hint, EnvironmentShell};
use crate::components::{Button, ButtonVariant};
use crate::theme::{Palette, SPACE_2};

const TOKEN_LABEL: &str = "Long-lived Claude Code token";
const TOKEN_KEPT: &str = "Kept in this computer's keyring; new Claude Code chats on this runner use it instead of the runner's own sign-in";
const TOKEN_ABSENT: &str = "Claude Code uses the runner's own sign-in";
const USE_LABEL: &str = "Use a long-lived token";
const FORGET_LABEL: &str = "Forget token";
const FIELD_LABEL: &str = "Token";
const FIELD_PLACEHOLDER: &str = "Paste the token claude setup-token printed";
const FORM_HINT: &str = "A terminal runs claude setup-token on this computer. Approve it in the browser, then paste the token it prints. It reaches the runner only on the standard input of each chat, never on a command line.";
const SAVE_LABEL: &str = "Keep token";
const CANCEL_LABEL: &str = "Cancel";
const USE_PREFIX: &str = "cloud-runner-token-use";
const FORGET_PREFIX: &str = "cloud-runner-token-forget";
const SAVE_PREFIX: &str = "cloud-runner-token-save";
const CANCEL_PREFIX: &str = "cloud-runner-token-cancel";

pub(in crate::environment_shell) struct TokenForm {
    runner: Option<CloudRunnerId>,
    field: Entity<InputState>,
    error: Option<String>,
}

impl TokenForm {
    pub(in crate::environment_shell) fn new(
        window: &mut Window,
        cx: &mut Context<EnvironmentShell>,
    ) -> Self {
        Self {
            runner: None,
            field: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(FIELD_PLACEHOLDER)
                    .masked(true)
            }),
            error: None,
        }
    }
}

impl EnvironmentShell {
    fn token_form(&mut self) -> Option<&mut TokenForm> {
        self.settings_ui
            .as_mut()
            .map(|ui| &mut ui.cloud_runners.token)
    }

    fn open_token_form(
        &mut self,
        runner: CloudRunnerId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.token_form() else {
            return;
        };
        form.runner = Some(runner);
        form.error = None;
        let field = form.field.clone();
        field.update(cx, |field, cx| field.set_value("", window, cx));
        self.start_claude_setup_token(cx);
        cx.notify();
    }

    fn close_token_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.token_form() else {
            return;
        };
        form.runner = None;
        form.error = None;
        let field = form.field.clone();
        field.update(cx, |field, cx| field.set_value("", window, cx));
        cx.notify();
    }

    fn submit_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((runner, text)) = self.settings_ui.as_ref().and_then(|ui| {
            let form = &ui.cloud_runners.token;
            Some((form.runner?, form.field.read(cx).value().to_string()))
        }) else {
            return;
        };
        match text.parse::<ClaudeOauthToken>() {
            Ok(token) => {
                self.close_token_form(window, cx);
                Self::save_claude_token(runner, token, cx);
            }
            Err(error) => {
                if let Some(form) = self.token_form() {
                    form.error = Some(error.to_string());
                }
                cx.notify();
            }
        }
    }

    pub(super) fn runner_token_rows(
        &self,
        runner: CloudRunnerId,
        form: &TokenForm,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let kept = self.runner_claude_token(runner).is_some();
        let button = |prefix: &'static str, label: &'static str| {
            Button::new((prefix, runner.0))
                .label(label)
                .small()
                .variant(ButtonVariant::Secondary)
        };
        let control = if kept {
            button(FORGET_PREFIX, FORGET_LABEL)
                .on_click(cx.listener(move |_: &mut Self, _, _, cx| {
                    Self::forget_claude_token(runner, cx);
                }))
                .into_any_element()
        } else {
            button(USE_PREFIX, USE_LABEL)
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    this.open_token_form(runner, window, cx);
                }))
                .into_any_element()
        };
        let detail = if kept { TOKEN_KEPT } else { TOKEN_ABSENT };
        let mut rows = vec![form_row(TOKEN_LABEL, Some(detail), control, pal)];
        if form.runner == Some(runner) {
            rows.push(sheet_padded(hint(FORM_HINT, pal)).into_any_element());
            rows.push(card_field(
                FIELD_LABEL,
                Input::new(&form.field).into_any_element(),
                pal,
            ));
            rows.extend(
                form.error
                    .as_deref()
                    .map(|error| sheet_padded(hint(error, pal)).into_any_element()),
            );
            rows.push(sheet_padded(Self::token_form_buttons(runner, cx)).into_any_element());
        }
        rows
    }

    fn token_form_buttons(runner: CloudRunnerId, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .gap(px(SPACE_2))
            .child(div().flex_1())
            .child(
                Button::new((CANCEL_PREFIX, runner.0))
                    .label(CANCEL_LABEL)
                    .small()
                    .variant(ButtonVariant::Ghost)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.close_token_form(window, cx);
                    })),
            )
            .child(
                Button::new((SAVE_PREFIX, runner.0))
                    .label(SAVE_LABEL)
                    .small()
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.submit_token(window, cx);
                    })),
            )
            .into_any_element()
    }
}
