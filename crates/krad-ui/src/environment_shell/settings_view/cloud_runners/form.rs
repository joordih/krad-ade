use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement, Styled,
    Window,
};

use super::super::super::dialog_parts::{field_label, sheet_group, sheet_padded};
use super::super::super::host::ssh_config_suggestions;
use super::super::sections::{card_field, settings_section};
use super::super::{hint, EnvironmentShell};
use super::draft::Draft;
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, Palette, SPACE_2};

const FORM_TITLE: &str = "Add a cloud runner";
const FORM_HINT: &str = "The app connects with your SSH setup, so keys, agents and aliases from ~/.ssh/config work as they do in a terminal.";
const NAME_LABEL: &str = "Name";
const DESTINATION_LABEL: &str = "SSH destination";
const NAME_PLACEHOLDER: &str = "Build box";
const DESTINATION_PLACEHOLDER: &str = "user@host, host:port or an alias from ~/.ssh/config";
const SUGGESTIONS_LABEL: &str = "From ~/.ssh/config";
const SAVE_LABEL: &str = "Add";
const CANCEL_LABEL: &str = "Cancel";
const BTN_SAVE: &str = "cloud-runner-save";
const BTN_CANCEL: &str = "cloud-runner-cancel";
const SUGGESTION_PREFIX: &str = "cloud-runner-suggestion";
const SUGGESTIONS_SHOWN: usize = 8;

pub(in crate::environment_shell) struct RunnerForm {
    name: Entity<InputState>,
    destination: Entity<InputState>,
    suggestions: Vec<String>,
    error: Option<String>,
    pub(super) open: bool,
}

fn field(
    placeholder: &str,
    window: &mut Window,
    cx: &mut Context<EnvironmentShell>,
) -> Entity<InputState> {
    let placeholder = placeholder.to_owned();
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

impl RunnerForm {
    pub(in crate::environment_shell) fn new(
        window: &mut Window,
        cx: &mut Context<EnvironmentShell>,
    ) -> Self {
        Self {
            name: field(NAME_PLACEHOLDER, window, cx),
            destination: field(DESTINATION_PLACEHOLDER, window, cx),
            suggestions: Vec::new(),
            error: None,
            open: false,
        }
    }

    fn draft(&self, cx: &Context<EnvironmentShell>) -> Draft {
        let text = |field: &Entity<InputState>| field.read(cx).value().to_string();
        Draft {
            name: text(&self.name),
            destination: text(&self.destination),
        }
    }
}

impl EnvironmentShell {
    fn runner_form(&mut self) -> Option<&mut RunnerForm> {
        self.settings_ui
            .as_mut()
            .map(|ui| &mut ui.cloud_runners.form)
    }

    pub(super) fn open_runner_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.runner_form() else {
            return;
        };
        form.open = true;
        form.error = None;
        let fields = [form.name.clone(), form.destination.clone()];
        for field in fields {
            field.update(cx, |field, cx| field.set_value("", window, cx));
        }
        let read = cx.background_spawn(async move { ssh_config_suggestions() });
        cx.spawn(async move |this, cx| {
            let suggestions = read.await;
            this.update(cx, |shell, cx| {
                if let Some(form) = shell.runner_form() {
                    form.suggestions = suggestions;
                    cx.notify();
                }
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    fn close_runner_form(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.runner_form() {
            form.open = false;
            form.error = None;
        }
        cx.notify();
    }

    fn pick_runner_suggestion(&mut self, alias: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.runner_form() else {
            return;
        };
        let destination = form.destination.clone();
        let name = form.name.clone();
        destination.update(cx, |field, cx| {
            field.set_value(alias.to_owned(), window, cx);
        });
        if name.read(cx).value().trim().is_empty() {
            name.update(cx, |field, cx| {
                field.set_value(alias.to_owned(), window, cx);
            });
        }
        cx.notify();
    }

    fn submit_runner(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self
            .settings_ui
            .as_ref()
            .map(|ui| ui.cloud_runners.form.draft(cx))
        else {
            return;
        };
        match draft.ready() {
            Ok((name, destination)) => {
                self.close_runner_form(cx);
                self.add_cloud_runner(name, destination, cx);
            }
            Err(error) => {
                if let Some(form) = self.runner_form() {
                    form.error = Some(error.to_string());
                }
                cx.notify();
            }
        }
    }

    fn runner_suggestions(
        form: &RunnerForm,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if form.suggestions.is_empty() {
            return None;
        }
        let chips = form
            .suggestions
            .iter()
            .take(SUGGESTIONS_SHOWN)
            .enumerate()
            .map(|(index, alias)| {
                let picked = alias.clone();
                Button::new((SUGGESTION_PREFIX, index))
                    .label(alias.clone())
                    .small()
                    .variant(ButtonVariant::Ghost)
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        this.pick_runner_suggestion(&picked, window, cx);
                    }))
            });
        Some(
            sheet_padded(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_2))
                    .child(field_label(SUGGESTIONS_LABEL, pal))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap(px(SPACE_2))
                            .children(chips),
                    ),
            )
            .into_any_element(),
        )
    }

    pub(super) fn runner_form_section(form: &RunnerForm, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let mut fields = vec![
            card_field(NAME_LABEL, Input::new(&form.name).into_any_element(), pal),
            card_field(
                DESTINATION_LABEL,
                Input::new(&form.destination).into_any_element(),
                pal,
            ),
        ];
        fields.extend(Self::runner_suggestions(form, pal, cx));
        fields.extend(
            form.error
                .as_deref()
                .map(|error| sheet_padded(hint(error, pal)).into_any_element()),
        );
        fields.push(sheet_padded(Self::runner_form_buttons(cx)).into_any_element());
        settings_section(FORM_TITLE, Some(FORM_HINT), [sheet_group(fields, pal)], pal)
    }

    fn runner_form_buttons(cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .gap(px(SPACE_2))
            .child(div().flex_1())
            .child(
                Button::new(BTN_CANCEL)
                    .label(CANCEL_LABEL)
                    .small()
                    .variant(ButtonVariant::Ghost)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.close_runner_form(cx);
                    })),
            )
            .child(
                Button::new(BTN_SAVE)
                    .label(SAVE_LABEL)
                    .small()
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.submit_runner(cx);
                    })),
            )
            .into_any_element()
    }
}
