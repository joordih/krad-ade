use gpui_kit::{Context, Window};
use krad_lsp::ServerInput;
use std::collections::BTreeMap;

use super::super::name_dialogs::NameTarget;
use super::super::EnvironmentShell;
use super::ServerKey;

const TITLE_SEPARATOR: &str = ": ";

pub(super) struct PendingInput {
    key: ServerKey,
    input: ServerInput,
    answered: BTreeMap<String, String>,
    next: usize,
    asking: bool,
}

impl EnvironmentShell {
    pub(super) fn queue_input(
        &mut self,
        key: &ServerKey,
        input: ServerInput,
        cx: &mut Context<Self>,
    ) {
        if let Some(previous) = self.language_servers.pending_input.take() {
            Self::answer_input(&self.language_servers, previous, None);
        }
        self.language_servers.pending_input = Some(PendingInput {
            key: key.clone(),
            input,
            answered: BTreeMap::new(),
            next: 0,
            asking: false,
        });
        cx.notify();
    }

    pub(super) fn ask_pending_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending) = self
            .language_servers
            .pending_input
            .as_mut()
            .filter(|pending| !pending.asking)
        else {
            return;
        };
        let Some(field) = pending.input.fields.get(pending.next).cloned() else {
            return;
        };
        pending.asking = true;
        let title = format!("{}{TITLE_SEPARATOR}{}", pending.input.title, field.label);
        self.ask_name(
            NameTarget::ServerInput {
                field: field.id,
                value: field.value,
                title,
            },
            window,
            cx,
        );
    }

    pub(in crate::environment_shell) fn input_answered(
        &mut self,
        value: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(mut pending) = self.language_servers.pending_input.take() else {
            return;
        };
        let Some(value) = value else {
            Self::answer_input(&self.language_servers, pending, None);
            return;
        };
        if let Some(field) = pending.input.fields.get(pending.next) {
            pending.answered.insert(field.id.clone(), value);
        }
        pending.next += 1;
        pending.asking = false;
        if pending.next < pending.input.fields.len() {
            self.language_servers.pending_input = Some(pending);
            cx.notify();
            return;
        }
        let values = pending.answered.clone();
        Self::answer_input(&self.language_servers, pending, Some(&values));
    }

    fn answer_input(
        servers: &super::LanguageServers,
        pending: PendingInput,
        values: Option<&BTreeMap<String, String>>,
    ) {
        let Some(running) = servers.running(&pending.key) else {
            return;
        };
        if let Err(error) = running.server.client.answer_input(pending.input.id, values) {
            tracing::debug!(%error, "a language server did not take the input it asked for");
        }
    }
}
