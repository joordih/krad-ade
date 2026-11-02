use gpui_kit::component::command::CommandGroup;
use gpui_kit::{App, Context, Window};
use std::path::{Path, PathBuf};

use super::super::EnvironmentShell;
use super::row::{command_row, Glyph};
use crate::actions::QuickOpen;
use crate::components::IconName;

const FILES_FOUND: usize = 40;
const PALETTE_GROUP_FILES: &str = "Files";
const FILE_GLYPH: Glyph = (IconName::File, None);

impl EnvironmentShell {
    pub(super) fn palette_root(&self) -> Option<PathBuf> {
        let live = self.envs.get(&self.active)?;
        let group = live.data.group(live.selected_group?)?;
        Some(group.root.clone())
    }

    pub(super) fn index_palette_files(&mut self, cx: &mut Context<Self>) {
        self.refresh_file_index(self.palette_root(), cx);
    }

    pub(super) fn find_palette_files(&mut self, query: &str) {
        let query = query.trim();
        self.palette_files = match self.palette_root().filter(|_| !query.is_empty()) {
            Some(root) => self.indexed_files(&root, query, FILES_FOUND),
            None => Vec::new(),
        };
    }

    pub(in crate::environment_shell) fn refind_palette_files(&mut self, cx: &App) {
        let Some(query) = self
            .palette
            .as_ref()
            .map(|palette| palette.read(cx).query(cx).to_string())
        else {
            return;
        };
        self.find_palette_files(&query);
    }

    pub(in crate::environment_shell) fn on_quick_open(
        &mut self,
        _: &QuickOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.palette_open {
            self.toggle_palette(window, cx);
        }
    }
}

pub(super) fn file_group(files: &[PathBuf], root: Option<&Path>, query: &str) -> CommandGroup {
    CommandGroup::new()
        .label(PALETTE_GROUP_FILES)
        .items(files.iter().map(|file| {
            let shown = root
                .and_then(|root| file.strip_prefix(root).ok())
                .unwrap_or(file);
            command_row(shown.to_string_lossy().into_owned(), FILE_GLYPH, None)
                .keywords([query.to_owned()])
        }))
}
