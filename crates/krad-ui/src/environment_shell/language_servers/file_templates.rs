use gpui_kit::{AppContext as _, Context, Task};
use krad_domain::{Host, LanguageId};
use krad_lsp::lsp_types::Position;
use krad_lsp::{
    file_uri, offers_file_templates, uri_path, CreateFromTemplateParams, CreateFromTemplateRequest,
    FileTemplate, FileTemplatesParams, FileTemplatesRequest, TemplateCreation,
};
use std::path::{Path, PathBuf};

use super::super::EnvironmentShell;
use super::edits::{apply_text_edits, workspace_changes};
use super::{ServerKey, ServerRoot};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) struct TemplateOutput {
    pub(in crate::environment_shell) folders: Vec<PathBuf>,
    pub(in crate::environment_shell) files: Vec<(PathBuf, String)>,
    pub(in crate::environment_shell) open: Option<(PathBuf, Position)>,
}

pub(in crate::environment_shell) struct ServerTemplates {
    pub(in crate::environment_shell) server: ServerKey,
    pub(in crate::environment_shell) languages: Vec<LanguageId>,
    pub(in crate::environment_shell) templates: Vec<FileTemplate>,
}

fn output_of(creation: TemplateCreation) -> TemplateOutput {
    let changes = workspace_changes(creation.edit);
    let files = changes
        .edited
        .into_iter()
        .filter(|(path, _)| changes.created.contains(path))
        .map(|(path, edits)| {
            let text = apply_text_edits("", edits);
            (path, text)
        })
        .collect();
    TemplateOutput {
        folders: creation.folders.iter().filter_map(uri_path).collect(),
        files,
        open: creation
            .open
            .and_then(|location| Some((uri_path(&location.uri)?, location.range.start))),
    }
}

impl EnvironmentShell {
    fn serves_folder(&self, key: &ServerKey, folder: &Path) -> bool {
        match &key.root {
            ServerRoot::Folder(root) => folder.starts_with(root),
            ServerRoot::Host => self
                .language_servers
                .folders
                .get(key)
                .is_some_and(|roots| roots.iter().any(|root| folder.starts_with(root))),
        }
    }

    pub(in crate::environment_shell) fn server_templates(
        &self,
        host: &Host,
        folder: &Path,
        cx: &mut Context<Self>,
    ) -> Vec<Task<Option<ServerTemplates>>> {
        let Ok(uri) = file_uri(folder) else {
            return Vec::new();
        };
        let servers = &self.language_servers;
        servers
            .slots
            .keys()
            .filter(|key| key.host == *host && self.serves_folder(key, folder))
            .filter_map(|key| {
                let running = servers.running(key)?;
                offers_file_templates(&running.capabilities).then_some((key, running))
            })
            .map(|(key, running)| {
                let languages = servers
                    .catalog
                    .iter()
                    .filter(|language| {
                        language.server.as_deref() == Some(key.plan.package.as_str())
                    })
                    .map(|language| language.id.clone())
                    .collect();
                let asked =
                    running
                        .server
                        .client
                        .request::<FileTemplatesRequest>(&FileTemplatesParams {
                            folder: uri.clone(),
                        });
                let server = key.clone();
                cx.background_spawn(async move {
                    let templates = asked.await.ok()?;
                    Some(ServerTemplates {
                        server,
                        languages,
                        templates,
                    })
                })
            })
            .collect()
    }

    pub(in crate::environment_shell) fn create_from_server_template(
        &self,
        server: &ServerKey,
        folder: &Path,
        template: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) -> Option<Task<Option<TemplateOutput>>> {
        let running = self.language_servers.running(server)?;
        let params = CreateFromTemplateParams {
            folder: file_uri(folder).ok()?,
            template: template.to_owned(),
            name: name.to_owned(),
        };
        let asked = running
            .server
            .client
            .request::<CreateFromTemplateRequest>(&params);
        Some(cx.background_spawn(async move { asked.await.ok().flatten().map(output_of) }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_lsp::lsp_types::{
        CreateFile, DocumentChangeOperation, DocumentChanges, Location, OneOf,
        OptionalVersionedTextDocumentIdentifier, Range, ResourceOp, TextDocumentEdit, TextEdit,
        WorkspaceEdit,
    };

    #[test]
    fn a_created_file_takes_the_text_its_edit_inserts_and_opens_where_the_server_says(
    ) -> Result<(), krad_lsp::LspError> {
        let uri = file_uri(Path::new("/work/src/main/java/dev/Main.java"))?;
        let start = Position::new(0, 0);
        let edit = WorkspaceEdit {
            document_changes: Some(DocumentChanges::Operations(vec![
                DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
                    uri: uri.clone(),
                    options: None,
                    annotation_id: None,
                })),
                DocumentChangeOperation::Edit(TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier {
                        uri: uri.clone(),
                        version: None,
                    },
                    edits: vec![OneOf::Left(TextEdit::new(
                        Range::new(start, start),
                        "class Main {}\n".to_owned(),
                    ))],
                }),
            ])),
            ..WorkspaceEdit::default()
        };
        let output = output_of(TemplateCreation {
            edit,
            folders: Vec::new(),
            open: Some(Location::new(
                uri,
                Range::new(Position::new(1, 0), Position::new(1, 0)),
            )),
        });
        let path = PathBuf::from("/work/src/main/java/dev/Main.java");
        assert_eq!(output.files, [(path.clone(), "class Main {}\n".to_owned())]);
        assert_eq!(output.open, Some((path, Position::new(1, 0))));
        Ok(())
    }
}
