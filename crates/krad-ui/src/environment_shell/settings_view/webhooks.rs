mod draft;
mod form;

use futures::channel::oneshot;
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, IntoElement, ParentElement, Styled, Window,
};
use krad_domain::{
    AppEvent, DeliveryOutcome, EventRecord, ToastKind, Webhook, WebhookId, WebhookSigning,
};
use krad_store::delete_webhook_secret;
use std::time::SystemTime;

use super::sections::{empty_row, footnote, pane_column, settings_section};
use super::{form_row, panel_card, EnvironmentShell};
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{palette, Palette, SPACE_2};
use crate::webhook::{deliver_once, Delivery};
use form::WebhookForm;

const HOOKS_TITLE: &str = "Webhooks";
const HOOKS_HINT: &str = "Services that get the app's events, such as an agent waiting for input or a failed pipeline, as JSON over HTTPS.";
const HOOKS_EMPTY: &str = "No webhooks yet.";
const ADD_LABEL: &str = "Add webhook";
const TEST_LABEL: &str = "Send test";
const EDIT_LABEL: &str = "Edit";
const REMOVE_LABEL: &str = "Remove";
const SIGNED_DETAIL: &str = "signed";
const UNSIGNED_DETAIL: &str = "unsigned";
const EVENT_SINGULAR: &str = "event";
const EVENT_PLURAL: &str = "events";
const DETAIL_SEPARATOR: &str = " · ";
const REMOVED_TOAST: &str = "Webhook removed";
const DELIVERED_TOAST: &str = "The test event was delivered";
const REFUSED_TOAST: &str = "The webhook refused the test event";
const FAILED_TOAST: &str = "The webhook could not be reached";
const BTN_OPEN_FORM: &str = "webhook-open-form";
const TEST_PREFIX: &str = "webhook-test";
const EDIT_PREFIX: &str = "webhook-edit";
const REMOVE_PREFIX: &str = "webhook-remove";
const SIGNING_NOTE: &str = "Each request carries an X-Krad-Timestamp header with the time it was sent. With a secret, it also carries an X-Krad-Signature-256 header: sha256= and the HMAC-SHA256 of the timestamp, a dot and the body, so a receiver can turn away old or replayed requests. Secrets are kept in the system keyring, or in the config folder readable only by you. Failed deliveries are retried a few times with growing pauses.";

pub(in crate::environment_shell) struct WebhooksUi {
    form: WebhookForm,
}

fn hook_detail(hook: &Webhook) -> String {
    let count = hook.kinds.len();
    let noun = if count == 1 {
        EVENT_SINGULAR
    } else {
        EVENT_PLURAL
    };
    let signature = match hook.signing {
        WebhookSigning::Signed => SIGNED_DETAIL,
        WebhookSigning::Unsigned => UNSIGNED_DETAIL,
    };
    format!(
        "{}{DETAIL_SEPARATOR}{count} {noun}{DETAIL_SEPARATOR}{signature}",
        hook.url
    )
}

const fn test_toast(outcome: DeliveryOutcome) -> &'static str {
    match outcome {
        DeliveryOutcome::Delivered => DELIVERED_TOAST,
        DeliveryOutcome::Refused => REFUSED_TOAST,
        DeliveryOutcome::Failed => FAILED_TOAST,
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn webhooks_ui(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> WebhooksUi {
        WebhooksUi {
            form: WebhookForm::new(window, cx),
        }
    }

    fn queue_secret_change<T: Send + 'static>(
        &mut self,
        change: impl FnOnce() -> T + Send + 'static,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<T> {
        let (done, heard) = oneshot::channel();
        let previous = self.app_events.webhook_secrets.take();
        let task = cx.background_spawn(async move {
            if let Some(previous) = previous {
                previous.await;
            }
            done.send(change()).unwrap_or_default();
        });
        self.app_events.webhook_secrets = Some(task);
        heard
    }

    fn remove_webhook(&mut self, id: WebhookId, cx: &mut Context<Self>) {
        if self.settings.webhooks.remove(id).is_none() {
            return;
        }
        self.save_settings(cx);
        if let Some(sender) = &self.app_events.webhooks {
            sender.changed(id, None);
        }
        cx.notify();
        let removed = self.queue_secret_change(move || delete_webhook_secret(id), cx);
        cx.spawn(async move |this, cx| {
            let Ok(removed) = removed.await else {
                return;
            };
            this.update(cx, |shell, cx| match removed {
                Ok(()) => shell.toast(ToastKind::Success, REMOVED_TOAST, cx),
                Err(error) => shell.toast(ToastKind::Error, &error.to_string(), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn test_webhook(&mut self, id: WebhookId, cx: &mut Context<Self>) {
        let Some(hook) = self.settings.webhooks.find(id).cloned() else {
            return;
        };
        let record = EventRecord::new(SystemTime::now(), self.active.clone(), None, AppEvent::Ping);
        let delivery = Delivery { hook, record };
        let sent = cx.background_spawn(async move { deliver_once(&delivery) });
        cx.spawn(async move |this, cx| {
            let outcome = sent.await;
            this.update(cx, |shell, cx| {
                shell.toast(ToastKind::Info, test_toast(outcome), cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn webhook_row(hook: &Webhook, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let id = hook.id;
        let edited = hook.clone();
        let button = |prefix: &'static str, label: &'static str| {
            Button::new((prefix, id.0))
                .label(label)
                .small()
                .variant(ButtonVariant::Secondary)
        };
        let controls =
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_2))
                .child(button(TEST_PREFIX, TEST_LABEL).on_click(
                    cx.listener(move |this: &mut Self, _, _, cx| this.test_webhook(id, cx)),
                ))
                .child(button(EDIT_PREFIX, EDIT_LABEL).on_click(cx.listener(
                    move |this: &mut Self, _, window, cx| {
                        this.open_webhook_form(Some(edited.clone()), window, cx);
                    },
                )))
                .child(button(REMOVE_PREFIX, REMOVE_LABEL).on_click(
                    cx.listener(move |this: &mut Self, _, _, cx| this.remove_webhook(id, cx)),
                ))
                .into_any_element();
        form_row(hook.name.as_str(), Some(&hook_detail(hook)), controls, pal)
    }

    fn webhooks_card(&self, form_open: bool, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let rows: Vec<AnyElement> = if self.settings.webhooks.hooks.is_empty() {
            vec![empty_row(HOOKS_EMPTY, pal)]
        } else {
            self.settings
                .webhooks
                .hooks
                .iter()
                .map(|hook| Self::webhook_row(hook, pal, cx))
                .collect()
        };
        let add = (!form_open).then(|| {
            div()
                .flex()
                .flex_row()
                .justify_end()
                .child(
                    Button::new(BTN_OPEN_FORM)
                        .label(ADD_LABEL)
                        .icon(IconName::Add)
                        .small()
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                            this.open_webhook_form(None, window, cx);
                        })),
                )
                .into_any_element()
        });
        settings_section(
            HOOKS_TITLE,
            Some(HOOKS_HINT),
            std::iter::once(panel_card(rows, pal)).chain(add),
            pal,
        )
    }

    pub(super) fn webhooks_pane(&self, ui: &WebhooksUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let open = ui.form.open;
        let mut sections = vec![self.webhooks_card(open, cx)];
        if open {
            sections.push(Self::webhook_form_section(&ui.form, cx));
        }
        sections.push(footnote(SIGNING_NOTE, pal));
        pane_column(sections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::EventKind;

    #[test]
    fn a_row_tells_the_address_the_event_count_and_the_signature() {
        let hook = Webhook {
            id: WebhookId(1),
            name: "ci".parse().unwrap_or_else(|_| unreachable!("a name")),
            url: "https://hooks.example.test/x"
                .parse()
                .unwrap_or_else(|_| unreachable!("a url")),
            kinds: [EventKind::CiFailed].into_iter().collect(),
            signing: WebhookSigning::Signed,
        };
        assert_eq!(
            hook_detail(&hook),
            "https://hooks.example.test/x · 1 event · signed"
        );
        assert_eq!(test_toast(DeliveryOutcome::Refused), REFUSED_TOAST);
    }
}
