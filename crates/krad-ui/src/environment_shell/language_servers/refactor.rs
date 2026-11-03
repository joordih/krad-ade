use gpui_kit::component::input::{CodeActionProvider, EditorState, Rope, RopeExt as _};
use gpui_kit::http_client::Result;
use gpui_kit::{App, Context, Entity, SharedString, Task, Window};
use krad_domain::Host;
use krad_lsp::lsp_types::request::{
    CodeActionRequest, CodeActionResolveRequest, ExecuteCommand, Formatting, Rename,
};
use krad_lsp::lsp_types::{
    CodeAction, CodeActionContext, CodeActionOrCommand, CodeActionParams, CodeActionTriggerKind,
    Command, DocumentFormattingParams, ExecuteCommandParams, FormattingOptions,
    PartialResultParams, Position, Range, RenameParams, TextDocumentIdentifier,
    TextDocumentPositionParams, WorkDoneProgressParams, WorkspaceEdit,
};
use krad_lsp::{file_uri, uri_path, LspClient};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::super::name_dialogs::NameTarget;
use super::super::EnvironmentShell;
use super::providers::ServerDocument;
use super::Document;
use crate::actions::{Reformat, RenameSymbol};

const PROVIDER_ID: &str = "krad-language-server";
const START_RENAME_COMMAND: &str = "krad.startRename";
const LINE_KEY: &str = "line";
const CHARACTER_KEY: &str = "character";
const TAB_SIZE: u32 = 4;
const NOTHING_TO_RENAME_TEXT: &str = "Nothing to rename here";
const RENAME_FAILED_TEXT: &str = "Rename failed";
const FORMAT_FAILED_TEXT: &str = "Formatting failed";
const CODE_ACTION_FAILED_TEXT: &str = "The code action failed";

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

fn rename_target(command: &Command) -> Option<(PathBuf, Position)> {
    let arguments = command.arguments.as_deref()?;
    let uri = arguments.first()?.as_str()?.parse().ok()?;
    let position = arguments.get(1)?;
    let line = u32::try_from(position.get(LINE_KEY)?.as_u64()?).ok()?;
    let character = u32::try_from(position.get(CHARACTER_KEY)?.as_u64()?).ok()?;
    let at = Position::new(line, character);
    Some((uri_path(&uri)?, at))
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
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Vec<CodeAction>>> {
        let at = range.start;
        let text: &Rope = state.read(cx).text();
        let range = Range::new(
            text.offset_to_position(range.start),
            text.offset_to_position(range.end),
        );
        let diagnostics = self
            .shell
            .upgrade()
            .map_or_else(Vec::new, |shell| {
                shell
                    .read(cx)
                    .language_servers
                    .diagnostics
                    .of(&self.document)
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
        let (shell, document) = (self.shell.clone(), self.document.clone());
        window.spawn(cx, async move |cx| {
            let actions: Vec<CodeAction> = answer
                .await?
                .unwrap_or_default()
                .into_iter()
                .map(as_action)
                .collect();
            shell.update_in(cx, |shell, window, cx| {
                shell.show_code_actions(&document, at, actions, window, cx);
            })?;
            Ok(Vec::new())
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
        let host = self.document.host.clone();
        cx.spawn(async move |cx| {
            let action = if action.edit.is_none() && action.data.is_some() {
                client.request::<CodeActionResolveRequest>(&action).await?
            } else {
                action
            };
            if let Some(edit) = action.edit {
                let edit_host = host.clone();
                shell.update(cx, |shell, cx| {
                    shell.queue_workspace_edit(edit_host, edit, cx);
                })?;
            }
            if let Some(command) = action.command {
                if command.command == START_RENAME_COMMAND {
                    if let Some((path, at)) = rename_target(&command) {
                        shell
                            .update(cx, |shell, cx| shell.queue_start_rename(host, path, at, cx))?;
                    }
                } else {
                    run_command(&client, command).await?;
                }
            }
            Ok(())
        })
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn run_code_action(
        &self,
        document: &Document,
        state: Entity<EditorState>,
        action: CodeAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(running) = self
            .language_servers
            .documents
            .get(document)
            .and_then(|key| self.language_servers.running(key))
        else {
            return;
        };
        let Some(server) = ServerDocument::new(
            running.server.client.clone(),
            document,
            None,
            &running.capabilities,
            cx.entity().downgrade(),
        ) else {
            return;
        };
        let performing = server.perform_code_action(state, action, true, window, cx);
        cx.spawn(async move |this, cx| {
            if let Err(error) = performing.await {
                this.update(cx, |shell, _| {
                    shell.push_toast(format!("{CODE_ACTION_FAILED_TEXT}: {error}"));
                })
                .unwrap_or_default();
            }
        })
        .detach();
    }

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
        if word.is_empty()
            || self
                .language_servers
                .client_of(&self.active_document(&path))
                .is_none()
        {
            self.toast(NOTHING_TO_RENAME_TEXT, cx);
            return;
        }
        self.ask_name(NameTarget::Symbol { path, at, word }, window, cx);
    }

    fn queue_start_rename(
        &mut self,
        host: Host,
        path: PathBuf,
        at: Position,
        cx: &mut Context<Self>,
    ) {
        self.language_servers.pending_rename = Some((host, path, at));
        cx.notify();
    }

    pub(super) fn start_rename_at(
        &mut self,
        document: &super::Document,
        at: Position,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.open_state(document) else {
            return;
        };
        let word = {
            let editor = state.read(cx);
            let text = editor.text();
            text.word_at(text.position_to_offset(&at))
        };
        if word.is_empty() {
            return;
        }
        self.ask_name(
            NameTarget::Symbol {
                path: document.path.clone(),
                at,
                word,
            },
            window,
            cx,
        );
    }

    pub(in crate::environment_shell) fn rename_symbol(
        &mut self,
        path: &Path,
        at: Position,
        new_name: String,
        cx: &mut Context<Self>,
    ) {
        let document = self.active_document(path);
        let (Some(client), Ok(uri)) = (self.language_servers.client_of(&document), file_uri(path))
        else {
            return;
        };
        let host = document.host;
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
                Ok(Some(edit)) => shell.queue_workspace_edit(host, edit, cx),
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
        let document = self.active_document(&path);
        let (Some(client), Ok(uri)) = (self.language_servers.client_of(&document), file_uri(&path))
        else {
            return;
        };
        let host = document.host;
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
                    host,
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
