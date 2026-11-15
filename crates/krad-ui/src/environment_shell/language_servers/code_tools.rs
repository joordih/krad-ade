use gpui_kit::{AppContext as _, Context};
use krad_domain::agents_api::{CodeAnswer, CodeRequest, ErrorCode, Response};
use krad_domain::{EnvironmentName, GroupId, TerminalId};
use krad_lsp::LspClient;
use krad_runtime::BusReply;
use std::path::PathBuf;

use super::super::EnvironmentShell;
use super::{Document, ServerKey, ServerRoot};
use krad_lsp::{ask_code as ask, confined};

const NO_SERVER: &str =
    "No language server runs for this file yet: open a file of its language in the editor";
const NO_GROUP: &str = "This agent has no group folder";
const OUTSIDE_GROUP: &str = "The change would touch files outside the agent's group folder";
const NOT_A_FILE: &str = "The path is not a file of the group";

pub(in crate::environment_shell) enum CodeCaller {
    Terminal(TerminalId),
    Group(GroupId),
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn answer_code(
        &mut self,
        environment: &str,
        caller: &CodeCaller,
        request: CodeRequest,
        reply: BusReply,
        cx: &mut Context<Self>,
    ) {
        let env = EnvironmentName::new(environment.to_owned());
        let Some(live) = self.envs.get(&env) else {
            reply
                .send(Response::error(ErrorCode::Unsupported, NO_GROUP))
                .unwrap_or_default();
            return;
        };
        let group = match caller {
            CodeCaller::Terminal(id) => live.data.group_of(*id),
            CodeCaller::Group(id) => live.data.group(*id),
        };
        let Some(root) = group.map(|group| group.root.clone()) else {
            reply
                .send(Response::error(ErrorCode::Unsupported, NO_GROUP))
                .unwrap_or_default();
            return;
        };
        let host = live.data.host.clone();
        let requested = PathBuf::from(&request.path);
        let path = if requested.is_absolute() {
            requested
        } else {
            root.join(requested)
        };
        let climbs = path
            .components()
            .any(|part| part == std::path::Component::ParentDir);
        if climbs || !path.starts_with(&root) {
            reply
                .send(Response::error(ErrorCode::NotFound, NOT_A_FILE))
                .unwrap_or_default();
            return;
        }
        let document = Document::new(&host, &path);
        let Some((key, client)) = self.code_server(&document) else {
            reply
                .send(Response::error(ErrorCode::Unsupported, NO_SERVER))
                .unwrap_or_default();
            return;
        };
        let partial = self
            .language_servers
            .running(&key)
            .is_some_and(|running| running.progress.label().is_some());
        let modifies = request.tool.modifies();
        let asked = cx.background_spawn(ask(client, request, path, root.clone()));
        cx.spawn(async move |this, cx| {
            let answered = asked.await;
            this.update(cx, |shell, cx| {
                let response = match answered {
                    Ok((_, Some(edit))) if modifies && !confined(&edit, &root) => {
                        Response::error(ErrorCode::Forbidden, OUTSIDE_GROUP)
                    }
                    Ok((lines, edit)) => {
                        if let Some(edit) = edit {
                            shell.queue_workspace_edit(host, edit, cx);
                        }
                        Response::Code(CodeAnswer { lines, partial })
                    }
                    Err(error) => Response::error(ErrorCode::Failed, error.to_string()),
                };
                reply.send(response).unwrap_or_default();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn code_server(&self, document: &Document) -> Option<(ServerKey, LspClient)> {
        if let Some(key) = self.language_servers.documents.get(document) {
            let running = self.language_servers.running(key)?;
            return Some((key.clone(), running.server.client.clone()));
        }
        let language = self.language_servers.catalog.for_path(&document.path)?;
        let plan = self.language_servers.tools.as_ref()?.plan(language)?;
        self.language_servers.slots.keys().find_map(|key| {
            let serves = key.host == document.host
                && key.plan.package == plan.package
                && match &key.root {
                    ServerRoot::Host => true,
                    ServerRoot::Folder(folder) => document.path.starts_with(folder),
                };
            let running = self.language_servers.running(key).filter(|_| serves)?;
            Some((key.clone(), running.server.client.clone()))
        })
    }
}
