use futures::executor::block_on;
use krad_agents::{Caller, ErrorCode, Incoming, Request, Response};
use krad_domain::agents_api::{CodeAnswer, CodeRequest};
use krad_domain::{EnvironmentName, TerminalId};
use krad_host::HostLink;
use krad_lsp::lsp_types::WorkspaceEdit;
use krad_lsp::{
    apply_text_edits, ask_code, confined, workspace_changes, DocumentChange, LspClient,
};
use std::path::{Path, PathBuf};

use super::Runtime;

const OUTSIDE_GROUP: &str = "The change would touch files outside the agent's group folder";
const NOT_A_FILE: &str = "The path is not a file of the group";
const NOT_STARTED: &str = "The code tool could not start";

struct Asked {
    client: LspClient,
    link: HostLink,
    request: CodeRequest,
    path: PathBuf,
    root: PathBuf,
    open_for_tool: Option<String>,
}

fn apply_edit(link: &HostLink, edit: WorkspaceEdit) -> Result<(), String> {
    let changes = workspace_changes(edit);
    for created in &changes.created {
        link.write(created, &[])
            .map_err(|error| error.to_string())?;
    }
    for (file, edits) in changes.edited {
        let text = link
            .read_to_string(&file)
            .map_err(|error| error.to_string())?;
        let edited = apply_text_edits(&text, edits);
        link.write(&file, edited.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    for (from, to) in &changes.renamed {
        link.rename(from, to).map_err(|error| error.to_string())?;
    }
    for deleted in &changes.deleted {
        link.remove_file(deleted)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn answered(asked: Asked) -> Response {
    let Asked {
        client,
        link,
        request,
        path,
        root,
        open_for_tool,
    } = asked;
    if let Some(language) = &open_for_tool {
        let opened = link
            .read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|text| {
                client
                    .document(DocumentChange::Opened {
                        path: path.clone(),
                        language: language.clone(),
                        text,
                    })
                    .map_err(|error| error.to_string())
            });
        if let Err(error) = opened {
            return Response::error(ErrorCode::Failed, error);
        }
    }
    let modifies = request.tool.modifies();
    let response = match block_on(ask_code(
        client.clone(),
        request,
        path.clone(),
        root.clone(),
    )) {
        Ok((_, Some(edit))) if modifies && !confined(&edit, &root) => {
            Response::error(ErrorCode::Forbidden, OUTSIDE_GROUP)
        }
        Ok((lines, edit)) => match edit.map_or(Ok(()), |edit| apply_edit(&link, edit)) {
            Ok(()) => Response::Code(CodeAnswer {
                lines,
                partial: false,
            }),
            Err(error) => Response::error(ErrorCode::Failed, error),
        },
        Err(error) => Response::error(ErrorCode::Failed, error.to_string()),
    };
    if open_for_tool.is_some() {
        if let Err(error) = client.document(DocumentChange::Closed { path }) {
            tracing::debug!(%error, "a language server did not hear a code tool's close");
        }
    }
    response
}

fn resolved(root: &Path, requested: &str) -> Option<PathBuf> {
    let requested = PathBuf::from(requested);
    let path = if requested.is_absolute() {
        requested
    } else {
        root.join(requested)
    };
    let climbs = path
        .components()
        .any(|part| part == std::path::Component::ParentDir);
    (!climbs && path.starts_with(root)).then_some(path)
}

impl Runtime {
    pub(super) fn answer_code_requests(&mut self, incoming: Incoming) -> Option<Incoming> {
        let (Request::Code(request), Caller::Terminal(terminal)) =
            (&incoming.request, &incoming.from)
        else {
            return Some(incoming);
        };
        let environment = EnvironmentName::new(terminal.environment.clone());
        let Some(root) = self
            .environments
            .data(&environment)
            .and_then(|data| data.group_of(TerminalId(terminal.terminal)))
            .map(|group| group.root.clone())
        else {
            return Some(incoming);
        };
        let Some(path) = resolved(&root, &request.path) else {
            if incoming
                .reply
                .send(Response::error(ErrorCode::NotFound, NOT_A_FILE))
                .is_err()
            {
                tracing::trace!("a code tool caller left before its answer");
            }
            return None;
        };
        let Some((client, host, language)) = self.code_server(&environment, &path) else {
            return Some(incoming);
        };
        let open = self
            .languages
            .documents
            .values()
            .any(|document| document.path == path);
        let Incoming { request, reply, .. } = incoming;
        let Request::Code(request) = request else {
            return None;
        };
        let asked = Asked {
            client,
            link: self.hosts.link(&host),
            request,
            path,
            root,
            open_for_tool: (!open).then_some(language),
        };
        let started = self.jobs.run_reporting(move |_| {
            if reply.send(answered(asked)).is_err() {
                tracing::trace!("a code tool caller left before its answer");
            }
        });
        if let Err(error) = started {
            tracing::warn!(%error, "{NOT_STARTED}");
        }
        None
    }

    fn code_server(
        &self,
        environment: &EnvironmentName,
        path: &Path,
    ) -> Option<(LspClient, krad_domain::Host, String)> {
        let tools = self.languages.tools()?;
        let language = tools.catalog().for_path(path)?;
        let plan = tools.plan(language)?;
        self.languages.servers.iter().find_map(|(key, server)| {
            let serves = key.environment == *environment
                && key.package == plan.package
                && path.starts_with(&key.root);
            let running = server.running.as_ref().filter(|_| serves)?;
            Some((
                running.client.clone(),
                key.host.clone(),
                language.lsp_id().to_owned(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_path_stays_inside_the_group() {
        let root = Path::new("/home/dev/app");
        assert_eq!(
            resolved(root, "src/main.rs"),
            Some(root.join("src/main.rs"))
        );
        assert_eq!(resolved(root, "/etc/passwd"), None);
        assert_eq!(
            resolved(root, "/home/dev/app/lib.rs"),
            Some(root.join("lib.rs"))
        );
    }
}
