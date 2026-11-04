use gpui_kit::{Context, Window};
use krad_lsp::{uri_path, ChoiceId, ServerChoice, ServerConflicts};

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::EnvironmentShell;
use super::ServerKey;

const CANCEL_LABEL: &str = "Cancel";
const CONTINUE_LABEL: &str = "Continue anyway";
const MENU_ORIGIN_X_SHARE: f32 = 0.5;
const MENU_ORIGIN_Y_SHARE: f32 = 0.3;

#[derive(Clone, Debug, PartialEq)]
pub(in crate::environment_shell) enum ChoiceAnswer {
    Pick {
        key: ServerKey,
        id: ChoiceId,
        picked: Option<String>,
    },
    Conflicts {
        key: ServerKey,
        id: ChoiceId,
        proceed: bool,
    },
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
            MenuAction::AnswerChoice(Box::new(ChoiceAnswer::Pick {
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

    pub(super) fn queue_conflicts(
        &mut self,
        key: &ServerKey,
        conflicts: ServerConflicts,
        cx: &mut Context<Self>,
    ) {
        self.language_servers.pending_conflicts = Some((key.clone(), conflicts));
        cx.notify();
    }

    pub(super) fn show_pending_conflicts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((key, asked)) = self.language_servers.pending_conflicts.take() else {
            return;
        };
        let answer = |proceed: bool| {
            MenuAction::AnswerChoice(Box::new(ChoiceAnswer::Conflicts {
                key: key.clone(),
                id: asked.id.clone(),
                proceed,
            }))
        };
        let mut entries = vec![MenuEntry::Header(asked.title.clone())];
        for conflict in &asked.conflicts {
            let opened = conflict.location.as_ref().and_then(|location| {
                uri_path(&location.uri).map(|path| (path, location.range.start))
            });
            entries.push(match opened {
                Some((path, at)) => MenuEntry::item(
                    conflict.message.clone(),
                    None,
                    MenuAction::OpenLocation(path, at),
                ),
                None => MenuEntry::Note(conflict.message.clone()),
            });
        }
        entries.push(MenuEntry::Separator);
        entries.push(MenuEntry::item(CONTINUE_LABEL, None, answer(true)));
        entries.push(MenuEntry::item(CANCEL_LABEL, None, answer(false)));
        let size = window.viewport_size();
        let origin = (
            size.width.as_f32() * MENU_ORIGIN_X_SHARE,
            size.height.as_f32() * MENU_ORIGIN_Y_SHARE,
        );
        self.open_menu(origin, MenuPlacement::Below, entries, cx);
    }

    pub(in crate::environment_shell) fn answer_choice(&mut self, answer: ChoiceAnswer) {
        let (key, sent) = match answer {
            ChoiceAnswer::Pick { key, id, picked } => {
                let Some(running) = self.language_servers.running(&key) else {
                    return;
                };
                (
                    key.clone(),
                    running.server.client.answer_choice(id, picked.as_deref()),
                )
            }
            ChoiceAnswer::Conflicts { key, id, proceed } => {
                let Some(running) = self.language_servers.running(&key) else {
                    return;
                };
                (
                    key.clone(),
                    running.server.client.answer_conflicts(id, proceed),
                )
            }
        };
        if let Err(error) = sent {
            tracing::debug!(%error, server = %key.plan.package, "a language server did not take the answer to its question");
        }
    }
}
