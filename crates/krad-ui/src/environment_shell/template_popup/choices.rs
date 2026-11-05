use gpui_kit::{AppContext as _, Context, Window};
use krad_domain::{Host, LanguageId, LanguageTemplate};
use krad_host::HostLink;
use krad_lsp::lsp_types::Position;
use krad_repo::EntryKind;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::super::entry_input::entry_name;
use super::super::language_servers::{ServerKey, ServerTemplates, TemplateOutput};
use super::super::EnvironmentShell;

const TEMPLATE_FAILED_TEXT: &str = "The file could not be created";
const PLAIN_GROUP: &str = "Other";
const PLAIN_LABEL: &str = "Empty file";
const ALREADY_EXISTS_TEXT: &str = "already exists";

#[derive(Clone)]
pub(in crate::environment_shell) enum ChoiceSource {
    Server { server: Box<ServerKey>, id: String },
    Manifest { language: LanguageId, id: String },
    Plain,
}

impl ChoiceSource {
    const fn rank(&self) -> u8 {
        match self {
            Self::Server { .. } => 0,
            Self::Manifest { .. } => 1,
            Self::Plain => 2,
        }
    }
}

#[derive(Clone)]
pub(in crate::environment_shell) struct TemplateChoice {
    pub(super) group: String,
    pub(super) label: String,
    language: Option<LanguageId>,
    source: ChoiceSource,
}

impl TemplateChoice {
    pub(super) fn plain() -> Self {
        Self {
            group: PLAIN_GROUP.to_owned(),
            label: PLAIN_LABEL.to_owned(),
            language: None,
            source: ChoiceSource::Plain,
        }
    }

    pub(super) const fn is_plain(&self) -> bool {
        matches!(self.source, ChoiceSource::Plain)
    }

    fn shown_beside(&self, hidden: &HashSet<LanguageId>) -> bool {
        !matches!(self.source, ChoiceSource::Manifest { .. })
            || self
                .language
                .as_ref()
                .is_none_or(|language| !hidden.contains(language))
    }
}

struct MarkedTemplate {
    language: LanguageId,
    template: LanguageTemplate,
}

pub(super) fn manifest_output(
    template: &LanguageTemplate,
    folder: &Path,
    package: Option<&str>,
    name: &str,
) -> Option<TemplateOutput> {
    let rendered = template.render(name, package)?;
    let target = rendered
        .folders
        .iter()
        .fold(folder.to_path_buf(), |dir, part| dir.join(part));
    let file = target.join(&rendered.file);
    let (line, column) = rendered.cursor;
    Some(TemplateOutput {
        folders: vec![target],
        files: vec![(file.clone(), rendered.text)],
        open: Some((file, Position::new(line, column))),
    })
}

fn has_marker(link: &HostLink, folder: &Path, stop: Option<&Path>, markers: &[String]) -> bool {
    for dir in folder.ancestors() {
        if markers.iter().any(|marker| link.exists(&dir.join(marker))) {
            return true;
        }
        if Some(dir) == stop {
            return false;
        }
    }
    false
}

fn write_output(link: &HostLink, output: &TemplateOutput) -> Result<(), String> {
    for folder in &output.folders {
        link.create_dir_all(folder)
            .map_err(|error| error.to_string())?;
    }
    for (path, text) in &output.files {
        if link.exists(path) {
            return Err(format!("{} {ALREADY_EXISTS_TEXT}", path.display()));
        }
        if let Some(parent) = path.parent() {
            link.create_dir_all(parent)
                .map_err(|error| error.to_string())?;
        }
        link.write(path, text.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

impl EnvironmentShell {
    fn add_choices(
        &mut self,
        added: Vec<TemplateChoice>,
        hidden: &[LanguageId],
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = &mut self.template_popup else {
            return;
        };
        popup.hidden.extend(hidden.iter().cloned());
        let hidden_now: HashSet<LanguageId> = popup.hidden.iter().cloned().collect();
        popup
            .choices
            .retain(|choice| choice.shown_beside(&hidden_now));
        popup.choices.extend(
            added
                .into_iter()
                .filter(|choice| choice.shown_beside(&hidden_now)),
        );
        popup.choices.sort_by_key(|choice| choice.source.rank());
        popup.pending = popup.pending.saturating_sub(1);
        popup.selected = popup.selected.min(popup.choices.len().saturating_sub(1));
        if popup.only_plain_left() {
            let typed = popup.field.read(cx).value().to_string();
            let file = entry_name(&typed).map(|name| format!("{name}.{}", popup.extension));
            let folder = popup.folder.clone();
            self.close_template_popup(cx);
            if let Some(file) = file {
                self.create_file_entry(folder, EntryKind::File, &file, cx);
            }
            return;
        }
        cx.notify();
    }

    pub(super) fn gather_template_choices(&mut self, cx: &mut Context<Self>) {
        let Some((host, folder, extension, languages)) =
            self.template_popup.as_ref().map(|popup| {
                (
                    popup.host.clone(),
                    popup.folder.clone(),
                    popup.extension.clone(),
                    popup.languages.clone(),
                )
            })
        else {
            return;
        };
        let mut placed = Vec::new();
        let mut marked = Vec::new();
        for language in self
            .language_catalog()
            .iter()
            .filter(|language| languages.contains(&language.id))
        {
            for template in language
                .templates
                .iter()
                .filter(|template| template.creates(&extension))
            {
                if !template.markers.is_empty() {
                    marked.push(MarkedTemplate {
                        language: language.id.clone(),
                        template: template.clone(),
                    });
                } else if language.package_in(&folder).is_some() {
                    placed.push(TemplateChoice {
                        group: template.group.clone(),
                        label: template.label.clone(),
                        language: Some(language.id.clone()),
                        source: ChoiceSource::Manifest {
                            language: language.id.clone(),
                            id: template.id.clone(),
                        },
                    });
                }
            }
        }
        let servers = self.server_templates(&host, &folder, &languages, cx);
        if let Some(popup) = &mut self.template_popup {
            popup.pending = 2 + servers.len();
        }
        self.add_choices(placed, &[], cx);
        let link = self.active_link();
        let stop = self.file_tree_root().map(Path::to_path_buf);
        let checking = folder.clone();
        let checked = cx.background_spawn(async move {
            marked
                .into_iter()
                .filter(|marked| {
                    has_marker(&link, &checking, stop.as_deref(), &marked.template.markers)
                })
                .map(|marked| TemplateChoice {
                    group: marked.template.group.clone(),
                    label: marked.template.label.clone(),
                    language: Some(marked.language.clone()),
                    source: ChoiceSource::Manifest {
                        language: marked.language,
                        id: marked.template.id,
                    },
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let found = checked.await;
            this.update(cx, |shell, cx| shell.add_choices(found, &[], cx))
                .unwrap_or_default();
        })
        .detach();
        for asked in servers {
            cx.spawn(async move |this, cx| {
                let answer: Option<ServerTemplates> = asked.await;
                this.update(cx, |shell, cx| {
                    let Some(answer) = answer.filter(|answer| !answer.templates.is_empty()) else {
                        shell.add_choices(Vec::new(), &[], cx);
                        return;
                    };
                    let choices = answer
                        .templates
                        .into_iter()
                        .map(|template| TemplateChoice {
                            group: template.group,
                            label: template.label,
                            language: None,
                            source: ChoiceSource::Server {
                                server: Box::new(answer.server.clone()),
                                id: template.id,
                            },
                        })
                        .collect();
                    shell.add_choices(choices, &answer.languages, cx);
                })
                .unwrap_or_default();
            })
            .detach();
        }
    }

    pub(super) fn run_template_choice(
        &mut self,
        host: &Host,
        folder: &Path,
        choice: &TemplateChoice,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let output = match &choice.source {
            ChoiceSource::Server { server, id } => {
                let Some(asked) = self.create_from_server_template(server, folder, id, name, cx)
                else {
                    return;
                };
                Some(asked)
            }
            ChoiceSource::Plain => return,
            ChoiceSource::Manifest { language, id } => {
                let catalog = self.language_catalog();
                let Some(definition) = catalog.get(language) else {
                    return;
                };
                let Some(template) = definition
                    .templates
                    .iter()
                    .find(|template| template.id == *id)
                else {
                    return;
                };
                let package = definition.package_in(folder);
                let Some(output) = manifest_output(template, folder, package.as_deref(), name)
                else {
                    return;
                };
                self.write_template_output(host, output, window, cx);
                None
            }
        };
        if let Some(asked) = output {
            let host = host.clone();
            cx.spawn_in(window, async move |this, cx| {
                let output = asked.await;
                this.update_in(cx, |shell, window, cx| match output {
                    Some(output) => shell.write_template_output(&host, output, window, cx),
                    None => shell.toast(TEMPLATE_FAILED_TEXT, cx),
                })
                .unwrap_or_default();
            })
            .detach();
        }
    }

    fn write_template_output(
        &mut self,
        host: &Host,
        output: TemplateOutput,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let link = self.active_link();
        if link.host() != *host {
            return;
        }
        self.close_template_popup(cx);
        let opened = output.open.clone();
        let writing = cx.background_spawn(async move { write_output(&link, &output) });
        cx.spawn(async move |this, cx| {
            let written = writing.await;
            this.update(cx, |shell, cx| {
                if let Err(error) = written {
                    shell.toast(&format!("{TEMPLATE_FAILED_TEXT}: {error}"), cx);
                    return;
                }
                shell.reload_file_tree(cx);
                shell.refresh_git(cx);
                if let Some((path, at)) = opened {
                    shell.select_tree_entry(PathBuf::from(&path));
                    shell.open_file_at(&path, Some(at), cx);
                }
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
    fn a_manifest_template_lands_in_its_package_folder_and_opens_at_its_cursor() {
        let template = LanguageTemplate {
            id: "class".to_owned(),
            label: "Class".to_owned(),
            group: "Java Class".to_owned(),
            file: "{{name}}.java".to_owned(),
            body: "public class {{name}} {\n    $0\n}\n".to_owned(),
            package_line: Some("package {{package}};".to_owned()),
            markers: Vec::new(),
        };
        let folder = Path::new("/p/src/main/java/dev");
        let output = manifest_output(&template, folder, Some("dev"), "crypto.Cipher");
        let file = PathBuf::from("/p/src/main/java/dev/crypto/Cipher.java");
        assert_eq!(
            output,
            Some(TemplateOutput {
                folders: vec![PathBuf::from("/p/src/main/java/dev/crypto")],
                files: vec![(
                    file.clone(),
                    "package dev.crypto;\n\npublic class Cipher {\n    \n}\n".to_owned()
                )],
                open: Some((file, Position::new(3, 4))),
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_marker_counts_in_the_folder_or_any_folder_above_it_up_to_the_root() -> std::io::Result<()>
    {
        let root = std::env::temp_dir().join(format!("krad-markers-{}", std::process::id()));
        let nested = root.join("web/src/components");
        std::fs::create_dir_all(&nested)?;
        std::fs::write(root.join("web/package.json"), "{}")?;
        let markers = ["package.json".to_owned()];
        let link = HostLink::Local;
        let found = has_marker(&link, &nested, Some(&root), &markers);
        let outside = has_marker(&link, &root, Some(&root), &markers);
        std::fs::remove_dir_all(&root)?;
        assert!(found);
        assert!(!outside);
        Ok(())
    }
}
