use gpui_kit::{Context, Window};
use krad_lsp::{ChoiceId, ServerChoice};

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::EnvironmentShell;
use super::ServerKey;

const CANCEL_LABEL: &str = "Cancel";
const MENU_ORIGIN_X_SHARE: f32 = 0.5;
const MENU_ORIGIN_Y_SHARE: f32 = 0.3;

#[derive(Clone, Debug, PartialEq)]
pub(in crate::environment_shell) struct ChoiceAnswer {
    key: ServerKey,
    id: ChoiceId,
    picked: Option<String>,
}

impl EnvironmentShell {
    pub(super) fn queue_choice(
        &mut self,
        key: &ServerKey,
        choice: ServerChoice,
        cx: &mut Context<Self>,
    ) {
        self.language_servers.pending_choice = Some((key.clone(), choice));
        cx.notify();
    }

    pub(super) fn show_pending_choice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((key, choice)) = self.language_servers.pending_choice.take() else {
            return;
        };
        let answer = |picked: Option<String>| {
            MenuAction::AnswerChoice(Box::new(ChoiceAnswer {
                key: key.clone(),
                id: choice.id.clone(),
                picked,
            }))
        };
        let mut entries = vec![MenuEntry::Header(choice.message.clone())];
        entries.extend(
            choice
                .options
                .iter()
                .map(|option| MenuEntry::item(option.clone(), None, answer(Some(option.clone())))),
        );
        entries.push(MenuEntry::Separator);
        entries.push(MenuEntry::item(CANCEL_LABEL, None, answer(None)));
        let size = window.viewport_size();
        let origin = (
            size.width.as_f32() * MENU_ORIGIN_X_SHARE,
            size.height.as_f32() * MENU_ORIGIN_Y_SHARE,
        );
        self.open_menu(origin, MenuPlacement::Below, entries, cx);
    }

    pub(in crate::environment_shell) fn answer_choice(&mut self, answer: ChoiceAnswer) {
        let Some(running) = self.language_servers.running(&answer.key) else {
            return;
        };
        if let Err(error) = running
            .server
            .client
            .answer_choice(answer.id, answer.picked.as_deref())
        {
            tracing::debug!(%error, "a language server did not take the answer to its question");
        }
    }
}
