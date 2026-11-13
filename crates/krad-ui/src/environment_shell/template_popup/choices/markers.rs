use gpui_kit::Context;
use krad_protocol::{Answer, FileAsk, NewFile};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::super::super::language_servers::TemplateOutput;
use super::super::super::EnvironmentShell;
use super::{ChoiceSource, MarkedTemplate, TemplateChoice};

pub(super) fn marker_paths(folder: &Path, stop: Option<&Path>, markers: &[String]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for dir in folder.ancestors() {
        paths.extend(markers.iter().map(|marker| dir.join(marker)));
        if Some(dir) == stop {
            break;
        }
    }
    paths
}

pub(super) fn has_marker(
    present: &HashSet<PathBuf>,
    folder: &Path,
    stop: Option<&Path>,
    markers: &[String],
) -> bool {
    marker_paths(folder, stop, markers)
        .iter()
        .any(|path| present.contains(path))
}

fn present_of(answer: Answer) -> Option<Vec<bool>> {
    match answer {
        Answer::Present { present } => Some(present),
        _ => None,
    }
}

impl From<TemplateOutput> for FileAsk {
    fn from(output: TemplateOutput) -> Self {
        Self::CreateFiles {
            folders: output.folders,
            files: output
                .files
                .into_iter()
                .map(|(path, text)| NewFile { path, text })
                .collect(),
        }
    }
}

impl EnvironmentShell {
    pub(super) fn check_template_markers(
        &self,
        folder: PathBuf,
        marked: Vec<MarkedTemplate>,
        cx: &mut Context<Self>,
    ) {
        let stop = self.file_tree_root().map(Path::to_path_buf);
        let paths: Vec<PathBuf> = marked
            .iter()
            .flat_map(|marked| marker_paths(&folder, stop.as_deref(), &marked.template.markers))
            .collect();
        let asked = self.ask_active_file(
            FileAsk::Present {
                paths: paths.clone(),
            },
            present_of,
            cx,
        );
        let checking = folder;
        cx.spawn(async move |this, cx| {
            let present: HashSet<PathBuf> = asked.await.map_or_default(|present| {
                paths
                    .into_iter()
                    .zip(present)
                    .filter_map(|(path, present)| present.then_some(path))
                    .collect()
            });
            let found: Vec<TemplateChoice> = marked
                .into_iter()
                .filter(|marked| {
                    has_marker(
                        &present,
                        &checking,
                        stop.as_deref(),
                        &marked.template.markers,
                    )
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
                .collect();
            this.update(cx, |shell, cx| shell.add_choices(found, &[], cx))
                .unwrap_or_default();
        })
        .detach();
    }
}
