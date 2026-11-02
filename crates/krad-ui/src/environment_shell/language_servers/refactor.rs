use gpui_kit::component::input::{CodeActionProvider, EditorState, Rope, RopeExt as _};
use gpui_kit::http_client::Result;
use gpui_kit::{App, AppContext as _, Context, Entity, SharedString, Task, Window};
use krad_lsp::lsp_types::request::{
    CodeActionRequest, CodeActionResolveRequest, ExecuteCommand, Formatting, Rename,
};
use krad_lsp::lsp_types::{
    CodeAction, CodeActionContext, CodeActionOrCommand, CodeActionParams, CodeActionTriggerKind,
    Command, DocumentFormattingParams, ExecuteCommandParams, FormattingOptions,
    PartialResultParams, Position, Range, RenameParams, TextDocumentIdentifier,
    TextDocumentPositionParams, WorkDoneProgressParams, WorkspaceEdit,
};
use krad_lsp::{file_uri, LspClient};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::super::name_dialogs::NameTarget;
use super::super::EnvironmentShell;
use super::providers::ServerDocument;
use crate::actions::{Reformat, RenameSymbol};

const PROVIDER_ID: &str = "krad-language-server";
const TAB_SIZE: u32 = 4;
const NOTHING_TO_RENAME_TEXT: &str = "Nothing to rename here";
const RENAME_FAILED_TEXT: &str = "Rename failed";
const FORMAT_FAILED_TEXT: &str = "Formatting failed";

fn as_action(item: CodeActionOrCommand) -> CodeAction {
    match item {
        CodeActionOrCommand::CodeAction(action) => action,
        CodeActionOrCommand::Command(command) => CodeAction {
            title: command.title.clone(),
            command: Some(command),
            ..CodeAction::default()
        },
    }
}

fn overlaps(range: &Range, with: &Range) -> bool {
    let key = |at: &Position| (at.line, at.character);
    key(&range.start) <= key(&with.end) && key(&with.start) <= key(&range.end)
}

async fn run_command(client: &LspClient, command: Command) -> Result<()> {
    let params = ExecuteCommandParams {
        command: command.command,
        arguments: command.arguments.unwrap_or_default(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    client.request::<ExecuteCommand>(&params).await?;
    Ok(())
}

impl CodeActionProvider for ServerDocument {
    fn id(&self) -> SharedString {
        PROVIDER_ID.into()
    }

    fn code_actions(
        &self,
        state: Entity<EditorState>,
        range: std::ops::Range<usize>,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Vec<CodeAction>>> {
        let text: &Rope = state.read(cx).text();
        let range = Range::new(
            text.offset_to_position(range.start),
            text.offset_to_position(range.end),
        );
        let diagnostics = self
            .shell
            .upgrade()
            .map_or_else(Vec::new, |shell| {
                shell.read(cx).language_servers.diagnostics.of(&self.path)
            })
            .into_iter()
            .filter(|diagnostic| overlaps(&diagnostic.range, &range))
            .collect();
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier::new(self.uri.clone()),
            range,
            context: CodeActionContext {
                diagnostics,
                only: None,
                trigger_kind: Some(CodeActionTriggerKind::INVOKED),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let answer = self.client.request::<CodeActionRequest>(&params);
        cx.background_spawn(async move {
            Ok(answer
                .await?
                .unwrap_or_default()
                .into_iter()
                .map(as_action)
                .collect())
        })
    }

    fn perform_code_action(
        &self,
        _: Entity<EditorState>,
        action: CodeAction,
        _: bool,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let client = self.client.clone();
        let shell = self.shell.clone();
        cx.spawn(async move |cx| {
            let action = if action.edit.is_none() && action.data.is_some() {
                client.request::<CodeActionResolveRequest>(&action).await?
            } else {
                action
            };
            if let Some(edit) = action.edit {
                shell.update(cx, |shell, cx| shell.queue_workspace_edit(edit, cx))?;
            }
            if let Some(command) = action.command {
                run_command(&client, command).await?;
            }
            Ok(())
        })
    }
}

impl EnvironmentShell {
    fn shown_document(&self, cx: &App) -> Option<(PathBuf, Entity<EditorState>, Position)> {
        let live = self.envs.get(&self.active)?;
        let (id, path) = live.shown_file()?;
        let state = live.editors.get(&id)?.state.clone()?;
        let at = state.read(cx).cursor_position();
        Some((path, state, at))
    }

    pub(in crate::environment_shell) fn on_rename_symbol(
        &mut self,
        _: &RenameSymbol,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((path, state, at)) = self.shown_document(cx) else {
            return;
        };
        let word = {
            let editor = state.read(cx);
            editor.text().word_at(editor.cursor())
        };
        if word.is_empty() || self.language_servers.client_of(&path).is_none() {
            self.toast(NOTHING_TO_RENAME_TEXT, cx);
            return;
        }
        self.ask_name(NameTarget::Symbol { path, at, word }, window, cx);
    }

    pub(in crate::environment_shell) fn rename_symbol(
        &mut self,
        path: &Path,
        at: Position,
        new_name: String,
        cx: &mut Context<Self>,
    ) {
        let (Some(client), Ok(uri)) = (self.language_servers.client_of(path), file_uri(path))
        else {
            return;
        };
        let params = RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier::new(uri),
                position: at,
            },
            new_name,
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let answer = client.request::<Rename>(&params);
        cx.spawn(async move |this, cx| {
            let renamed = answer.await;
            this.update(cx, |shell, cx| match renamed {
                Ok(Some(edit)) => shell.queue_workspace_edit(edit, cx),
                Ok(None) => shell.toast(NOTHING_TO_RENAME_TEXT, cx),
                Err(error) => shell.toast(&format!("{RENAME_FAILED_TEXT}: {error}"), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn on_reformat(
        &mut self,
        _: &Reformat,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((path, _, _)) = self.shown_document(cx) else {
            return;
        };
        let (Some(client), Ok(uri)) = (self.language_servers.client_of(&path), file_uri(&path))
        else {
            return;
        };
        let params = DocumentFormattingParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            options: FormattingOptions {
                tab_size: TAB_SIZE,
                insert_spaces: true,
                ..FormattingOptions::default()
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let answer = client.request::<Formatting>(&params);
        cx.spawn(async move |this, cx| {
            let formatted = answer.await;
            this.update(cx, |shell, cx| match formatted {
                Ok(Some(edits)) => shell.queue_workspace_edit(
                    WorkspaceEdit {
                        changes: Some(HashMap::from([(uri, edits)])),
                        ..WorkspaceEdit::default()
                    },
                    cx,
                ),
                Ok(None) => {}
                Err(error) => shell.toast(&format!("{FORMAT_FAILED_TEXT}: {error}"), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_command_becomes_an_action_with_its_title() {
        let command = Command::new("Run test".to_owned(), "run".to_owned(), None);
        let action = as_action(CodeActionOrCommand::Command(command));
        assert_eq!(action.title, "Run test");
        assert!(action.command.is_some());
    }

    #[test]
    fn ranges_overlap_when_they_share_a_position() {
        let line = |from, to| Range::new(Position::new(2, from), Position::new(2, to));
        assert!(overlaps(&line(0, 5), &line(5, 9)));
        assert!(!overlaps(&line(0, 4), &line(5, 9)));
    }
}
