use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement, Styled,
    Window,
};
use krad_domain::{EventKind, Webhook, WebhookSigning};
use krad_store::write_webhook_secret;
use std::collections::BTreeSet;

use super::super::super::dialog_parts::{sheet_group, sheet_padded};
use super::super::sections::{card_field, settings_section};
use super::super::{form_row, hint, EnvironmentShell};
use super::draft::{Draft, DraftTarget, ReadyWebhook};
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, SPACE_2};

const ADD_TITLE: &str = "Add a webhook";
const EDIT_TITLE: &str = "Edit the webhook";
const FORM_HINT: &str = "The app posts each chosen event as JSON to this address.";
const NAME_LABEL: &str = "Name";
const URL_LABEL: &str = "Address";
const SECRET_LABEL: &str = "Secret";
const NAME_PLACEHOLDER: &str = "Deploy bot";
const URL_PLACEHOLDER: &str = "https://hooks.example.com/krad";
const SECRET_PLACEHOLDER: &str = "Signs each request; optional";
const KEEP_SECRET_PLACEHOLDER: &str = "Leave blank to keep the saved secret";
const EVENTS_TITLE: &str = "Events";
const SAVE_LABEL: &str = "Save";
const CANCEL_LABEL: &str = "Cancel";
const SAVED_TOAST: &str = "Webhook saved";
const BTN_SAVE: &str = "webhook-save";
const BTN_CANCEL: &str = "webhook-cancel";
const KIND_SWITCH_PREFIX: &str = "webhook-kind";

pub(in crate::environment_shell) struct WebhookForm {
    name: Entity<InputState>,
    url: Entity<InputState>,
    secret: Entity<InputState>,
    kinds: BTreeSet<EventKind>,
    editing: Option<Webhook>,
    error: Option<String>,
    pub(super) open: bool,
}

fn field(
    placeholder: &str,
    masked: bool,
    window: &mut Window,
    cx: &mut Context<EnvironmentShell>,
) -> Entity<InputState> {
    let placeholder = placeholder.to_owned();
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder)
            .masked(masked)
    })
}

fn kind_index(kind: EventKind) -> u64 {
    let index = EventKind::SELECTABLE
        .iter()
        .position(|known| *known == kind)
        .unwrap_or_default();
    u64::try_from(index).unwrap_or(u64::MAX)
}

impl WebhookForm {
    pub(in crate::environment_shell) fn new(
        window: &mut Window,
        cx: &mut Context<EnvironmentShell>,
    ) -> Self {
        Self {
            name: field(NAME_PLACEHOLDER, false, window, cx),
            url: field(URL_PLACEHOLDER, false, window, cx),
            secret: field(SECRET_PLACEHOLDER, true, window, cx),
            kinds: BTreeSet::new(),
            editing: None,
            error: None,
            open: false,
        }
    }

    fn draft(&self, cx: &Context<EnvironmentShell>) -> Draft {
        let text = |field: &Entity<InputState>| field.read(cx).value().to_string();
        Draft {
            editing: self.editing.clone(),
            name: text(&self.name),
            url: text(&self.url),
            secret: text(&self.secret),
            kinds: self.kinds.clone(),
        }
    }
}

impl EnvironmentShell {
    fn webhook_form(&mut self) -> Option<&mut WebhookForm> {
        self.settings_ui.as_mut().map(|ui| &mut ui.webhooks.form)
    }

    pub(super) fn open_webhook_form(
        &mut self,
        editing: Option<Webhook>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.webhook_form() else {
            return;
        };
        let name = editing
            .as_ref()
            .map_or_else(String::new, |hook| hook.name.as_str().to_owned());
        let url = editing
            .as_ref()
            .map_or_else(String::new, |hook| hook.url.as_str().to_owned());
        let signed = editing
            .as_ref()
            .is_some_and(|hook| hook.signing == WebhookSigning::Signed);
        let secret_hint = if signed {
            KEEP_SECRET_PLACEHOLDER
        } else {
            SECRET_PLACEHOLDER
        };
        form.kinds = editing
            .as_ref()
            .map_or_else(BTreeSet::new, |hook| hook.kinds.clone());
        form.editing = editing;
        form.error = None;
        form.open = true;
        let fields = [(form.name.clone(), name), (form.url.clone(), url)];
        let secret = form.secret.clone();
        for (field, value) in fields {
            field.update(cx, |field, cx| field.set_value(value, window, cx));
        }
        secret.update(cx, |field, cx| {
            field.set_value("", window, cx);
            field.set_placeholder(secret_hint, window, cx);
        });
        cx.notify();
    }

    fn close_webhook_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.webhook_form() else {
            return;
        };
        form.open = false;
        form.editing = None;
        form.error = None;
        form.kinds.clear();
        let secret = form.secret.clone();
        secret.update(cx, |field, cx| field.set_value("", window, cx));
        cx.notify();
    }

    fn toggle_webhook_kind(&mut self, kind: EventKind, cx: &mut Context<Self>) {
        if let Some(form) = self.webhook_form() {
            if !form.kinds.remove(&kind) {
                form.kinds.insert(kind);
            }
        }
        cx.notify();
    }

    fn submit_webhook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self
            .settings_ui
            .as_ref()
            .map(|ui| ui.webhooks.form.draft(cx))
        else {
            return;
        };
        let webhooks = &mut self.settings.webhooks;
        match draft.ready(|| webhooks.issue_id()) {
            Ok(ready) => {
                self.close_webhook_form(window, cx);
                self.save_webhook(ready, cx);
            }
            Err(error) => {
                if let Some(form) = self.webhook_form() {
                    form.error = Some(error.to_string());
                }
                cx.notify();
            }
        }
    }

    fn save_webhook(&mut self, ready: ReadyWebhook, cx: &mut Context<Self>) {
        let ReadyWebhook {
            hook,
            new_secret,
            target,
        } = ready;
        let id = hook.id;
        let written = self.queue_secret_change(
            move || new_secret.map_or(Ok(()), |secret| write_webhook_secret(id, &secret)),
            cx,
        );
        cx.spawn(async move |this, cx| {
            let Ok(written) = written.await else {
                return;
            };
            this.update(cx, |shell, cx| {
                if let Err(error) = written {
                    return shell.toast(&error.to_string(), cx);
                }
                let removed_meanwhile =
                    target == DraftTarget::Existing && shell.settings.webhooks.find(id).is_none();
                if removed_meanwhile {
                    return;
                }
                shell.settings.webhooks.save(hook.clone());
                shell.save_settings(cx);
                if let Some(sender) = &shell.app_events.webhooks {
                    sender.changed(id, Some(hook));
                }
                shell.toast(SAVED_TOAST, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn webhook_form_section(form: &WebhookForm, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let title = if form.editing.is_some() {
            EDIT_TITLE
        } else {
            ADD_TITLE
        };
        let mut fields = vec![
            card_field(NAME_LABEL, Input::new(&form.name).into_any_element(), pal),
            card_field(URL_LABEL, Input::new(&form.url).into_any_element(), pal),
            card_field(
                SECRET_LABEL,
                Input::new(&form.secret).mask_toggle().into_any_element(),
                pal,
            ),
        ];
        fields.extend(
            form.error
                .as_deref()
                .map(|error| sheet_padded(hint(error, pal)).into_any_element()),
        );
        fields.push(sheet_padded(Self::webhook_form_buttons(cx)).into_any_element());
        let kinds: Vec<AnyElement> = EventKind::SELECTABLE
            .into_iter()
            .map(|kind| {
                let switch = Switch::new((KIND_SWITCH_PREFIX, kind_index(kind)))
                    .checked(form.kinds.contains(&kind))
                    .on_click(cx.listener(move |this: &mut Self, _: &bool, _, cx| {
                        this.toggle_webhook_kind(kind, cx);
                    }))
                    .into_any_element();
                form_row(kind.label(), None, switch, pal)
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .child(settings_section(
                title,
                Some(FORM_HINT),
                [sheet_group(fields, pal)],
                pal,
            ))
            .child(settings_section(
                EVENTS_TITLE,
                None,
                [sheet_group(kinds, pal)],
                pal,
            ))
            .into_any_element()
    }

    fn webhook_form_buttons(cx: &mut Context<Self>) -> AnyElement {
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
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.close_webhook_form(window, cx);
                    })),
            )
            .child(
                Button::new(BTN_SAVE)
                    .label(SAVE_LABEL)
                    .small()
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.submit_webhook(window, cx);
                    })),
            )
            .into_any_element()
    }
}
