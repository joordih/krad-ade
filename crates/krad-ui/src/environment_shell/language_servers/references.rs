use gpui_kit::{Context, Window};
use krad_domain::FileOpening;
use krad_lsp::lsp_types::request::References;
use krad_lsp::lsp_types::{
    Location, PartialResultParams, Position, ReferenceContext, ReferenceParams,
    TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
};
use krad_lsp::{file_uri, uri_path};
use std::path::{Path, PathBuf};

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::{EnvironmentShell, LiveEnv};
use super::Document;
use crate::actions::FindReferences;

const NO_REFERENCES_TEXT: &str = "No references found";
const LINE_BASE: u32 = 1;

fn reference_label(location: &Location, root: Option<&Path>) -> Option<(String, PathBuf)> {
    let path = uri_path(&location.uri)?;
    let shown = root
        .and_then(|root| path.strip_prefix(root).ok())
        .unwrap_or(&path)
        .to_string_lossy()
        .into_owned();
    let line = location.range.start.line + LINE_BASE;
    Some((format!("{shown}:{line}"), path))
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn open_file_at(
        &mut self,
        path: &Path,
        at: Option<Position>,
        cx: &mut Context<Self>,
    ) {
        self.open_file_tab(path, FileOpening::Pinned, cx);
        let Some(live) = self.envs.get_mut(&self.active) else {
            return;
        };
        let Some(id) = live.focused else {
            return;
        };
        if let Some(at) = at {
            live.pending_cursors.insert(id, at);
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn on_find_references(
        &mut self,
        _: &FindReferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.envs.get(&self.active) else {
            return;
        };
        let Some((id, path)) = live.shown_file() else {
            return;
        };
        let Some(state) = live
            .editors
            .get(&id)
            .and_then(|editor| editor.state.clone())
        else {
            return;
        };
        let document = Document::new(&live.data.host, &path);
        let (Some(client), Ok(uri)) = (self.language_servers.client_of(&document), file_uri(&path))
        else {
            return;
        };
        let root = self
            .server_key(live, id, &path)
            .and_then(|key| key.root.folder().map(Path::to_path_buf));
        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier::new(uri),
                position: state.read(cx).cursor_position(),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: ReferenceContext {
                include_declaration: true,
            },
        };
        let answer = client.request::<References>(&params);
        let mouse = window.mouse_position();
        let origin = (mouse.x.as_f32(), mouse.y.as_f32());
        cx.spawn(async move |this, cx| {
            let found = answer.await.ok().flatten().unwrap_or_default();
            this.update(cx, |shell, cx| {
                shell.show_references(&found, root.as_deref(), origin, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn show_references(
        &mut self,
        found: &[Location],
        root: Option<&Path>,
        origin: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let entries: Vec<MenuEntry> = found
            .iter()
            .filter_map(|location| {
                let (label, path) = reference_label(location, root)?;
                Some(MenuEntry::item(
                    label,
                    None,
                    MenuAction::OpenLocation(path, location.range.start),
                ))
            })
            .collect();
        if entries.is_empty() {
            self.toast(NO_REFERENCES_TEXT, cx);
            return;
        }
        self.open_menu(origin, MenuPlacement::Below, entries, cx);
    }
}

impl LiveEnv {
    pub(in crate::environment_shell) fn take_pending_cursor(
        &mut self,
        id: krad_domain::TerminalId,
    ) -> Option<Position> {
        self.pending_cursors.remove(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_lsp::lsp_types::{Range, Uri};
    use std::str::FromStr;

    #[test]
    fn a_reference_reads_as_its_path_in_the_group_and_its_line() {
        let Ok(uri) = Uri::from_str("file:///work/api/src/lib.rs") else {
            unreachable!("a literal file URI parses");
        };
        let location = Location::new(uri, Range::new(Position::new(9, 0), Position::new(9, 4)));
        let label = reference_label(&location, Some(Path::new("/work/api")));
        assert_eq!(
            label.map(|(label, _)| label).as_deref(),
            Some("src/lib.rs:10")
        );
    }
}
