use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{JavaEngine, ServerLaunch};
use krad_extensions::{trusted_manifest_folders, BundledManifest, LanguageTools};
use krad_host::HostLink;
use krad_lsp::lsp_types::request::{
    Completion, Formatting, GotoDefinition, HoverRequest, References, Rename, SignatureHelpRequest,
};
use krad_lsp::lsp_types::{
    CompletionParams, DocumentFormattingParams, FormattingOptions, GotoDefinitionParams,
    HoverParams, PartialResultParams, ReferenceContext, ReferenceParams, RenameParams,
    SignatureHelpParams, TextDocumentIdentifier, TextDocumentPositionParams,
    WorkDoneProgressParams,
};
use krad_lsp::{
    file_uri, DocumentChange, LanguageServer, LspClient, LspError, ServerEvent, ServerSettings,
};
use krad_protocol::{
    Answer, BufferId, ErrorCode, EventKind, FileDiagnostics, LanguageAnswer, LanguageAsk,
    LanguageCommand, Reply, RequestId, ServerState, TextPosition,
};
use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use super::{refused, Runtime};
use crate::jobs::JobDone;
use crate::languages::convert;
use crate::languages::{group_root, Document, Server, ServerKey, Tools};

const NO_SERVER: &str = "no language server runs for this buffer";
const NO_PLAN: &str = "no language server is known for this buffer";
const NOT_A_PATH: &str = "the buffer's path is not a file path";
const UNKNOWN_VERB: &str = "this runtime does not know that language command";
const TAB_SIZE: u32 = 4;

pub(crate) enum Located {
    Launch(ServerLaunch),
    NotInstalled,
    NeedsServerRuntime(String),
    Failed(String),
}

pub(crate) enum LanguageJob {
    ToolsLoaded(Result<LanguageTools, String>),
    Located {
        key: ServerKey,
        located: Located,
    },
    Started {
        key: ServerKey,
        started: Result<LanguageServer, String>,
    },
    Event {
        key: ServerKey,
        event: ServerEvent,
    },
}

impl fmt::Debug for LanguageJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::ToolsLoaded(_) => "ToolsLoaded",
            Self::Located { .. } => "Located",
            Self::Started { .. } => "Started",
            Self::Event { .. } => "Event",
        };
        formatter.write_str(name)
    }
}

fn platform_of(link: &HostLink) -> Option<krad_domain::HostPlatform> {
    match link {
        HostLink::Local => None,
        HostLink::Remote(ssh) => ssh.platform(),
    }
}

fn position_params(uri: krad_lsp::lsp_types::Uri, at: TextPosition) -> TextDocumentPositionParams {
    TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri },
        position: convert::position(at),
    }
}

async fn answer(
    client: LspClient,
    path: PathBuf,
    ask: LanguageAsk,
) -> Result<LanguageAnswer, LspError> {
    let uri = file_uri(&path)?;
    Ok(match ask {
        LanguageAsk::Completion { at } => {
            let params = CompletionParams {
                text_document_position: position_params(uri, at),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
                context: None,
            };
            LanguageAnswer::Completions {
                items: convert::completions(client.request::<Completion>(&params).await?),
            }
        }
        LanguageAsk::Hover { at } => {
            let params = HoverParams {
                text_document_position_params: position_params(uri, at),
                work_done_progress_params: WorkDoneProgressParams::default(),
            };
            LanguageAnswer::Hover {
                markdown: convert::hover(client.request::<HoverRequest>(&params).await?),
            }
        }
        LanguageAsk::Definition { at } => {
            let params = GotoDefinitionParams {
                text_document_position_params: position_params(uri, at),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            };
            LanguageAnswer::Locations {
                locations: convert::definitions(client.request::<GotoDefinition>(&params).await?),
            }
        }
        LanguageAsk::References { at } => {
            let params = ReferenceParams {
                text_document_position: position_params(uri, at),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
                context: ReferenceContext {
                    include_declaration: true,
                },
            };
            LanguageAnswer::Locations {
                locations: convert::references(client.request::<References>(&params).await?),
            }
        }
        LanguageAsk::SignatureHelp { at } => {
            let params = SignatureHelpParams {
                context: None,
                text_document_position_params: position_params(uri, at),
                work_done_progress_params: WorkDoneProgressParams::default(),
            };
            let (signatures, active) =
                convert::signatures(client.request::<SignatureHelpRequest>(&params).await?);
            LanguageAnswer::Signatures { signatures, active }
        }
        LanguageAsk::Rename { at, new_name } => {
            let params = RenameParams {
                text_document_position: position_params(uri, at),
                new_name,
                work_done_progress_params: WorkDoneProgressParams::default(),
            };
            LanguageAnswer::Edits {
                files: convert::workspace_edits(client.request::<Rename>(&params).await?),
            }
        }
        LanguageAsk::Format => {
            let params = DocumentFormattingParams {
                text_document: TextDocumentIdentifier { uri },
                options: FormattingOptions {
                    tab_size: TAB_SIZE,
                    insert_spaces: true,
                    ..FormattingOptions::default()
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
            };
            LanguageAnswer::Edits {
                files: convert::file_edits(path, client.request::<Formatting>(&params).await?),
            }
        }
        LanguageAsk::InstallServer | LanguageAsk::Unknown => LanguageAnswer::Unknown,
    })
}

impl Runtime {
    fn load_language_tools(&mut self) {
        self.languages.tools = Tools::Loading;
        let extensions = self.settings.current.extensions.clone();
        let bundled = match self.settings.current.java_engine {
            JavaEngine::IntelliJ => vec![BundledManifest::IntellijJava],
            JavaEngine::Jdtls => Vec::new(),
        };
        let started = self.jobs.run(move || {
            let loaded = krad_store::config_dir()
                .ok_or_else(|| "there is no config folder".to_owned())
                .and_then(|config| {
                    let folders = trusted_manifest_folders(&config, &extensions);
                    LanguageTools::load_with(&config, &folders, &bundled)
                        .map_err(|error| error.to_string())
                });
            JobDone::Language(LanguageJob::ToolsLoaded(loaded))
        });
        if let Err(error) = started {
            tracing::warn!(%error, "the language tools were not loaded");
            self.languages.tools = Tools::Failed;
        }
    }

    pub(super) fn attach_language(&mut self, buffer: BufferId) {
        let tools = match &self.languages.tools {
            Tools::Ready(tools) => Arc::clone(tools),
            Tools::Failed => return,
            Tools::Loading => return self.languages.waiting.push(buffer),
            Tools::Unloaded => {
                self.languages.waiting.push(buffer);
                return self.load_language_tools();
            }
        };
        let Some((environment, path, text)) = self.editor.document(buffer) else {
            return;
        };
        let Some(language) = tools.catalog().for_path(&path).cloned() else {
            return;
        };
        let Some(plan) = tools.plan(&language) else {
            return;
        };
        if tools.server_toolchain(&plan).is_some() {
            tracing::debug!(package = %plan.package, "a toolchain server stays with the desktop for now");
            return;
        }
        let Some(host) = self.environments.host_of(&environment) else {
            return;
        };
        let roots: Vec<PathBuf> = self
            .environments
            .data(&environment)
            .map_or_else(Vec::new, |data| {
                data.groups.iter().map(|group| group.root.clone()).collect()
            });
        let Some(root) = group_root(&roots, &path).or_else(|| path.parent().map(PathBuf::from))
        else {
            return;
        };
        let key = ServerKey {
            environment,
            host,
            root,
            package: plan.package.clone(),
        };
        self.languages.documents.insert(
            buffer,
            Document {
                key: key.clone(),
                path,
                language: language.lsp_id().to_owned(),
            },
        );
        let fresh = !self.languages.servers.contains_key(&key);
        let server = self
            .languages
            .servers
            .entry(key.clone())
            .or_insert_with(|| Server {
                plan,
                state: ServerState::Locating,
                running: None,
                documents: BTreeSet::new(),
            });
        server.documents.insert(buffer);
        if server.running.is_some() {
            return self.open_in_server(buffer, &text);
        }
        if fresh {
            self.languages.announce(&key, &mut self.outbox);
            self.locate_server(&key, tools, false);
        }
    }

    fn locate_server(&mut self, key: &ServerKey, tools: Arc<LanguageTools>, install: bool) {
        let Some(plan) = self
            .languages
            .servers
            .get(key)
            .map(|server| server.plan.clone())
        else {
            return;
        };
        let link = self.hosts.link(&key.host);
        let chosen = self.settings.current.server_runtimes.clone();
        let located_key = key.clone();
        let started = self.jobs.run(move || {
            let platform = platform_of(&link);
            let located = if install {
                match tools.install_on(&link, platform, &plan, &located_key.root, &chosen) {
                    Ok(launch) => Located::Launch(launch),
                    Err(error) => Located::Failed(error.to_string()),
                }
            } else if let Some(missing) =
                tools.missing_server_runtime_on(&link, platform, &plan, &chosen)
            {
                Located::NeedsServerRuntime(missing.name)
            } else {
                tools
                    .locate_on(&link, platform, &plan, &located_key.root, &chosen)
                    .map_or(Located::NotInstalled, Located::Launch)
            };
            JobDone::Language(LanguageJob::Located {
                key: located_key,
                located,
            })
        });
        if let Err(error) = started {
            let reason = error.to_string();
            self.languages
                .show(key, ServerState::Failed { reason }, &mut self.outbox);
        }
    }

    fn start_server(&mut self, key: &ServerKey, launch: ServerLaunch) {
        let (Some(tools), Some(plan)) = (
            self.languages.tools(),
            self.languages
                .servers
                .get(key)
                .map(|server| server.plan.clone()),
        ) else {
            return;
        };
        self.languages
            .show(key, ServerState::Starting, &mut self.outbox);
        let config = tools.config(&plan);
        let (sections, options) = (config.sections(), config.options.clone());
        let link = self.hosts.link(&key.host);
        let started_key = key.clone();
        let started = self.jobs.run_reporting(move |steps| {
            let root = started_key.root.clone();
            let (server, mut events) =
                match LanguageServer::start(&link, &launch, &root, ServerSettings::new(sections)) {
                    Ok(started) => started,
                    Err(error) => {
                        return steps.report(JobDone::Language(LanguageJob::Started {
                            key: started_key,
                            started: Err(error.to_string()),
                        }));
                    }
                };
            let initialized = block_on(LanguageServer::initialize(
                server.client.clone(),
                &link,
                std::slice::from_ref(&root),
                options,
            ));
            if let Err(error) = initialized {
                return steps.report(JobDone::Language(LanguageJob::Started {
                    key: started_key,
                    started: Err(error.to_string()),
                }));
            }
            steps.report(JobDone::Language(LanguageJob::Started {
                key: started_key.clone(),
                started: Ok(server),
            }));
            while let Some(event) = block_on(events.next()) {
                steps.report(JobDone::Language(LanguageJob::Event {
                    key: started_key.clone(),
                    event,
                }));
            }
        });
        if let Err(error) = started {
            let reason = error.to_string();
            self.languages
                .show(key, ServerState::Failed { reason }, &mut self.outbox);
        }
    }

    fn open_in_server(&self, buffer: BufferId, text: &str) {
        let Some((client, document)) = self.languages.client_of(buffer) else {
            return;
        };
        let opened = client.document(DocumentChange::Opened {
            path: document.path.clone(),
            language: document.language.clone(),
            text: text.to_owned(),
        });
        if let Err(error) = opened {
            tracing::debug!(%error, "a language server did not take an opened buffer");
        }
    }

    pub(super) fn language_buffer_edited(&self, buffer: BufferId) {
        let (Some((client, document)), Some((_, _, text))) = (
            self.languages.client_of(buffer),
            self.editor.document(buffer),
        ) else {
            return;
        };
        let edited = client.document(DocumentChange::Edited {
            path: document.path.clone(),
            text,
        });
        if let Err(error) = edited {
            tracing::debug!(%error, "a language server did not take an edit");
        }
    }

    pub(super) fn language_buffer_saved(&self, buffer: BufferId) {
        let Some((client, document)) = self.languages.client_of(buffer) else {
            return;
        };
        let path = document.path.clone();
        if let Err(error) = client.document(DocumentChange::Saved { path }) {
            tracing::debug!(%error, "a language server did not hear a save");
        }
    }

    pub(super) fn language_buffer_closed(&mut self, buffer: BufferId) {
        if let Some((client, document)) = self.languages.client_of(buffer) {
            let path = document.path.clone();
            if let Err(error) = client.document(DocumentChange::Closed { path }) {
                tracing::debug!(%error, "a language server did not hear a close");
            }
        }
        if let Some(document) = self.languages.documents.remove(&buffer) {
            if let Some(server) = self.languages.servers.get_mut(&document.key) {
                server.documents.remove(&buffer);
            }
        }
    }

    pub(super) fn language_job_done(&mut self, done: LanguageJob) {
        match done {
            LanguageJob::ToolsLoaded(loaded) => {
                self.languages.tools = match loaded {
                    Ok(tools) => Tools::Ready(Arc::new(tools)),
                    Err(error) => {
                        tracing::warn!(%error, "the language tools could not be loaded");
                        Tools::Failed
                    }
                };
                for buffer in std::mem::take(&mut self.languages.waiting) {
                    self.attach_language(buffer);
                }
            }
            LanguageJob::Located { key, located } => match located {
                Located::Launch(launch) => self.start_server(&key, launch),
                Located::NotInstalled => {
                    self.languages
                        .show(&key, ServerState::NotInstalled, &mut self.outbox);
                }
                Located::NeedsServerRuntime(name) => self.languages.show(
                    &key,
                    ServerState::NeedsServerRuntime { name },
                    &mut self.outbox,
                ),
                Located::Failed(reason) => {
                    self.languages
                        .show(&key, ServerState::Failed { reason }, &mut self.outbox);
                }
            },
            LanguageJob::Started { key, started } => self.server_started(&key, started),
            LanguageJob::Event { key, event } => self.server_event(&key, event),
        }
    }

    fn server_started(&mut self, key: &ServerKey, started: Result<LanguageServer, String>) {
        let server = match started {
            Ok(server) => server,
            Err(reason) => {
                return self
                    .languages
                    .show(key, ServerState::Failed { reason }, &mut self.outbox);
            }
        };
        let Some(slot) = self.languages.servers.get_mut(key) else {
            return;
        };
        slot.running = Some(server);
        let documents: Vec<BufferId> = slot.documents.iter().copied().collect();
        self.languages
            .show(key, ServerState::Running, &mut self.outbox);
        for buffer in documents {
            if let Some((_, _, text)) = self.editor.document(buffer) {
                self.open_in_server(buffer, &text);
            }
        }
    }

    fn server_event(&mut self, key: &ServerKey, event: ServerEvent) {
        match event {
            ServerEvent::Diagnostics(params) => {
                let Some((path, diagnostics)) = convert::diagnostics(params) else {
                    return;
                };
                self.outbox
                    .push(EventKind::Diagnostics(Box::new(FileDiagnostics {
                        environment: key.environment.clone(),
                        path,
                        diagnostics,
                    })));
            }
            ServerEvent::Stopped => {
                if let Some(server) = self.languages.servers.get_mut(key) {
                    server.running = None;
                }
                self.languages
                    .show(key, ServerState::Stopped, &mut self.outbox);
            }
            ServerEvent::ApplyEdit(_)
            | ServerEvent::DiagnosticsRefresh
            | ServerEvent::Progress(_)
            | ServerEvent::Choice(_)
            | ServerEvent::Input(_)
            | ServerEvent::Conflicts(_) => {
                tracing::debug!("a language server event is not shown by the runtime yet");
            }
        }
    }

    pub(super) fn execute_language(&mut self, id: RequestId, command: LanguageCommand) -> Reply {
        let LanguageCommand { buffer, ask } = command;
        match ask {
            LanguageAsk::Unknown => return refused(ErrorCode::Unsupported, UNKNOWN_VERB),
            LanguageAsk::InstallServer => return self.install_server(buffer),
            _ => {}
        }
        let Some((client, document)) = self.languages.client_of(buffer) else {
            return refused(ErrorCode::NotFound, NO_SERVER);
        };
        if file_uri(&document.path).is_err() {
            return refused(ErrorCode::Invalid, NOT_A_PATH);
        }
        let path = document.path.clone();
        let started = self.jobs.run(move || {
            let answer = match block_on(answer(client, path, ask)) {
                Ok(answer) => Answer::Language { answer },
                Err(error) => Answer::Failed {
                    code: ErrorCode::Failed,
                    message: error.to_string(),
                },
            };
            JobDone::Answered {
                request: id,
                answer: Box::new(answer),
            }
        });
        match started {
            Ok(()) => Reply::Accepted,
            Err(error) => refused(ErrorCode::Unavailable, &error.to_string()),
        }
    }

    fn install_server(&mut self, buffer: BufferId) -> Reply {
        let (Some(tools), Some(key)) = (
            self.languages.tools(),
            self.languages
                .documents
                .get(&buffer)
                .map(|document| document.key.clone()),
        ) else {
            return refused(ErrorCode::NotFound, NO_PLAN);
        };
        self.languages
            .show(&key, ServerState::Installing, &mut self.outbox);
        self.locate_server(&key, tools, true);
        Reply::Accepted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::io::{pipe, BufRead, BufReader, Write};
    use std::thread;

    const HEADER: &str = "Content-Length: ";

    fn read_request(input: &mut impl BufRead) -> Option<Value> {
        let mut length = 0;
        loop {
            let mut line = String::new();
            input.read_line(&mut line).ok()?;
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(size) = line.strip_prefix(HEADER) {
                length = size.parse().ok()?;
            }
        }
        let mut body = vec![0; length];
        input.read_exact(&mut body).ok()?;
        serde_json::from_slice(&body).ok()
    }

    fn reply(output: &mut impl Write, id: &Value, result: &Value) -> std::io::Result<()> {
        let body = json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string();
        write!(output, "{HEADER}{}\r\n\r\n{body}", body.len())?;
        output.flush()
    }

    fn fake_server() -> Result<LspClient, Box<dyn std::error::Error>> {
        let (client_reads, mut server_writes) = pipe()?;
        let (server_reads, client_writes) = pipe()?;
        let (client, _events) = LspClient::over(
            BufReader::new(client_reads),
            client_writes,
            ServerSettings::new(json!({})),
        )?;
        thread::spawn(move || {
            let mut from_client = BufReader::new(server_reads);
            while let Some(request) = read_request(&mut from_client) {
                let Some(id) = request.get("id").cloned() else {
                    continue;
                };
                let result = match request["method"].as_str() {
                    Some("textDocument/hover") => {
                        json!({ "contents": { "kind": "markdown", "value": "`fn main()`" } })
                    }
                    Some("textDocument/definition") => json!([{
                        "uri": "file:///home/dev/app/src/lib.rs",
                        "range": { "start": { "line": 3, "character": 7 }, "end": { "line": 3, "character": 11 } }
                    }]),
                    _ => Value::Null,
                };
                if reply(&mut server_writes, &id, &result).is_err() {
                    return;
                }
            }
        });
        Ok(client)
    }

    #[test]
    fn hover_and_definition_reach_the_server_and_come_back_as_answers(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = fake_server()?;
        let path = PathBuf::from("/home/dev/app/src/main.rs");
        let at = TextPosition {
            line: 0,
            character: 4,
        };
        let hovered = block_on(answer(
            client.clone(),
            path.clone(),
            LanguageAsk::Hover { at },
        ))?;
        assert_eq!(
            hovered,
            LanguageAnswer::Hover {
                markdown: Some("`fn main()`".to_owned())
            }
        );
        let LanguageAnswer::Locations { locations } =
            block_on(answer(client, path, LanguageAsk::Definition { at }))?
        else {
            return Err("not locations".into());
        };
        assert_eq!(locations[0].path, PathBuf::from("/home/dev/app/src/lib.rs"));
        assert_eq!(locations[0].range.start.character, 7);
        Ok(())
    }
}
