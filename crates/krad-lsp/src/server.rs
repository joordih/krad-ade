use futures::channel::mpsc;
use lsp_types::notification::{Exit, Initialized};
use lsp_types::request::{Initialize, Shutdown};
use lsp_types::{
    ClientCapabilities, CodeActionCapabilityResolveSupport, CodeActionClientCapabilities,
    CodeActionKind, CodeActionKindLiteralSupport, CodeActionLiteralSupport,
    CompletionClientCapabilities, DiagnosticClientCapabilities,
    DiagnosticWorkspaceClientCapabilities, GotoCapability, HoverClientCapabilities,
    InitializeParams, InitializedParams, MarkupKind, PublishDiagnosticsClientCapabilities,
    ReferenceClientCapabilities, RenameClientCapabilities, ServerCapabilities,
    TextDocumentClientCapabilities, TextDocumentSyncClientCapabilities, WindowClientCapabilities,
    WorkspaceClientCapabilities, WorkspaceEditClientCapabilities,
};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use krad_host::{HostCommand, HostLink};

use crate::client::{LspClient, ServerEvent, ServerSettings};
use crate::error::LspError;
use crate::uri::workspace_folder;
use krad_domain::ServerLaunch;
use serde_json::Value;

pub struct LanguageServer {
    pub client: LspClient,
    child: Child,
}

impl LanguageServer {
    /// # Errors
    ///
    /// Returns [`LspError::Spawn`] when the server cannot be started.
    pub fn start(
        link: &HostLink,
        launch: &ServerLaunch,
        root: &Path,
        settings: ServerSettings,
    ) -> Result<(Self, mpsc::Receiver<ServerEvent>), LspError> {
        let mut child = server_command(link, launch, root)?
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(LspError::Spawn)?;
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(LspError::Stopped);
        };
        let (client, events) = LspClient::over(BufReader::new(output), input, settings)?;
        Ok((Self { client, child }, events))
    }

    /// # Errors
    ///
    /// Returns [`LspError`] when the server refuses to start on the folder.
    pub async fn initialize(
        client: LspClient,
        link: &HostLink,
        folders: &[PathBuf],
        options: Option<Value>,
    ) -> Result<ServerCapabilities, LspError> {
        let folders = folders
            .iter()
            .map(|folder| workspace_folder(folder))
            .collect::<Result<Vec<_>, _>>()?;
        let params = initialize_params(link, folders, options);
        let result = client.request::<Initialize>(&params).await?;
        client.notify::<Initialized>(&InitializedParams {})?;
        Ok(result.capabilities)
    }

    pub fn stop(&mut self) {
        let shutdown = self.client.request::<Shutdown>(&());
        drop(shutdown);
        self.client.notify::<Exit>(&()).unwrap_or_default();
    }
}

impl Drop for LanguageServer {
    fn drop(&mut self) {
        self.stop();
        self.child.kill().unwrap_or_default();
    }
}

#[allow(deprecated)]
fn initialize_params(
    link: &HostLink,
    folders: Vec<lsp_types::WorkspaceFolder>,
    options: Option<Value>,
) -> InitializeParams {
    InitializeParams {
        process_id: link.is_local().then(std::process::id),
        root_uri: folders.first().map(|folder| folder.uri.clone()),
        workspace_folders: Some(folders),
        capabilities: client_capabilities(),
        initialization_options: options,
        ..InitializeParams::default()
    }
}

fn server_command(
    link: &HostLink,
    launch: &ServerLaunch,
    root: &Path,
) -> Result<Command, LspError> {
    if link.is_local() {
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .envs(launch.env.iter().map(|(name, value)| (name, value)))
            .current_dir(root);
        return Ok(command);
    }
    let mut remote = HostCommand::new(&launch.program);
    remote.args(&launch.args).current_dir(root).in_login_shell();
    let vars: Vec<(String, String)> = launch
        .env
        .iter()
        .map(|(name, value)| (name.clone(), value.to_string_lossy().into_owned()))
        .collect();
    remote.path_and_env(&vars);
    remote
        .build(link)
        .map_err(|error| LspError::Spawn(error.into()))
}

const EDIT_PROPERTY: &str = "edit";

fn client_capabilities() -> ClientCapabilities {
    let markup = vec![MarkupKind::Markdown, MarkupKind::PlainText];
    ClientCapabilities {
        text_document: Some(TextDocumentClientCapabilities {
            synchronization: Some(TextDocumentSyncClientCapabilities {
                did_save: Some(true),
                ..TextDocumentSyncClientCapabilities::default()
            }),
            completion: Some(CompletionClientCapabilities::default()),
            hover: Some(HoverClientCapabilities {
                content_format: Some(markup),
                ..HoverClientCapabilities::default()
            }),
            definition: Some(GotoCapability {
                link_support: Some(true),
                ..GotoCapability::default()
            }),
            references: Some(ReferenceClientCapabilities::default()),
            rename: Some(RenameClientCapabilities {
                prepare_support: Some(true),
                ..RenameClientCapabilities::default()
            }),
            code_action: Some(CodeActionClientCapabilities {
                code_action_literal_support: Some(CodeActionLiteralSupport {
                    code_action_kind: CodeActionKindLiteralSupport {
                        value_set: [
                            CodeActionKind::QUICKFIX,
                            CodeActionKind::REFACTOR,
                            CodeActionKind::REFACTOR_EXTRACT,
                            CodeActionKind::REFACTOR_INLINE,
                            CodeActionKind::REFACTOR_REWRITE,
                            CodeActionKind::SOURCE,
                            CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                        ]
                        .iter()
                        .map(|kind| kind.as_str().to_owned())
                        .collect(),
                    },
                }),
                data_support: Some(true),
                resolve_support: Some(CodeActionCapabilityResolveSupport {
                    properties: vec![EDIT_PROPERTY.to_owned()],
                }),
                ..CodeActionClientCapabilities::default()
            }),
            publish_diagnostics: Some(PublishDiagnosticsClientCapabilities::default()),
            diagnostic: Some(DiagnosticClientCapabilities::default()),
            ..TextDocumentClientCapabilities::default()
        }),
        window: Some(WindowClientCapabilities {
            work_done_progress: Some(true),
            ..WindowClientCapabilities::default()
        }),
        workspace: Some(WorkspaceClientCapabilities {
            apply_edit: Some(true),
            workspace_folders: Some(true),
            diagnostic: Some(DiagnosticWorkspaceClientCapabilities {
                refresh_support: Some(true),
            }),
            configuration: Some(true),
            workspace_edit: Some(WorkspaceEditClientCapabilities {
                document_changes: Some(true),
                ..WorkspaceEditClientCapabilities::default()
            }),
            ..WorkspaceClientCapabilities::default()
        }),
        ..ClientCapabilities::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(deprecated)]
    fn the_first_workspace_folder_is_also_the_root_older_servers_read() -> Result<(), LspError> {
        let folder = workspace_folder(Path::new("/work/loader"))?;
        let params = initialize_params(&HostLink::Local, vec![folder.clone()], None);
        assert_eq!(params.root_uri, Some(folder.uri));
        assert_eq!(
            params.workspace_folders.map(|folders| folders.len()),
            Some(1)
        );
        Ok(())
    }
}
