use gpui_kit::component::InteractiveElementExt as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, uniform_list, AnyElement, AppContext as _, Context, Div, FontWeight,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::FileOpening;
use krad_repo::{VaultEntry, VaultEntryKind};
use std::ops::Range;

use super::super::context_menu::{MenuAction, MenuEntry};
use super::super::file_tree::{DraggedFile, FileAction};
use super::super::EnvironmentShell;
use super::NoteAction;
use crate::components::{icon, Button, IconName};
use crate::theme::{
    palette, row_fill, ICON_SM, ICON_XS, LINE_SM, LINE_XS, NAV_ROW_H, RADIUS_XS, SPACE_0_5,
    SPACE_1, SPACE_1_5, SPACE_2, TEXT_SM, TEXT_XS,
};

const TREE_ID: &str = "vault-tree";
const ROW_PREFIX: &str = "vault-row";
const ROW_H: f32 = 26.0;
const INDENT: f32 = 14.0;
const BTN_NEW_NOTE: &str = "vault-new-note";
const BTN_NEW_FOLDER: &str = "vault-new-folder";
const BTN_COLLAPSE: &str = "vault-collapse";
const BTN_GRAPH: &str = "vault-graph";
const BTN_CLEAR_TAG: &str = "vault-clear-tag";
const BTN_GENERATE: &str = "vault-generate";
const TOOLTIP_GENERATE: &str = "Generate documentation from the code with an agent";
const TOOLTIP_NEW_NOTE: &str = "New note";
const TOOLTIP_NEW_FOLDER: &str = "New folder";
const TOOLTIP_COLLAPSE: &str = "Collapse all folders";
const TOOLTIP_GRAPH: &str = "Open the graph of the vault";
const TOOLTIP_CLEAR_TAG: &str = "Show every note";
const VAULT_LABEL: &str = "NOTES";
const NEW_NOTE_LABEL: &str = "New note";
const NEW_FOLDER_LABEL: &str = "New folder";
const OPEN_LABEL: &str = "Open in a new tab";
const RENAME_LABEL: &str = "Rename";
const COPY_PATH_LABEL: &str = "Copy path";
const REVEAL_LABEL: &str = "Reveal in file manager";
const DELETE_LABEL: &str = "Delete";
const TAG_PREFIX: &str = "#";
const PATH_SEPARATOR: char = '/';
const NOTE_SUFFIX: &str = ".md";

struct TreeRow<'a> {
    entry: &'a VaultEntry,
    depth: usize,
}

fn name_of(path: &str) -> &str {
    path.rsplit(PATH_SEPARATOR).next().unwrap_or(path)
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once(PATH_SEPARATOR)
        .map_or("", |(parent, _)| parent)
}

#[allow(clippy::cast_precision_loss)]
fn indent(depth: usize) -> f32 {
    depth as f32 * INDENT
}

impl EnvironmentShell {
    fn vault_rows(&self) -> Vec<TreeRow<'_>> {
        let notes = &self.notes;
        let tagged = |path: &str| {
            notes
                .tag_filter
                .as_ref()
                .is_none_or(|tag| notes.index.has_tag(path, tag))
        };
        let holds_tagged = |folder: &str| {
            notes.entries.iter().any(|entry| {
                entry.kind == VaultEntryKind::Note
                    && entry.path.starts_with(&format!("{folder}{PATH_SEPARATOR}"))
                    && tagged(&entry.path)
            })
        };
        notes
            .entries
            .iter()
            .filter(|entry| {
                let mut folder = parent_of(&entry.path);
                while !folder.is_empty() {
                    if !notes.expanded.contains(folder) {
                        return false;
                    }
                    folder = parent_of(folder);
                }
                match (entry.kind, notes.tag_filter.is_some()) {
                    (_, false) => true,
                    (VaultEntryKind::Note, true) => tagged(&entry.path),
                    (VaultEntryKind::Folder, true) => holds_tagged(&entry.path),
                    (VaultEntryKind::Attachment, true) => false,
                }
            })
            .map(|entry| TreeRow {
                entry,
                depth: entry.path.matches(PATH_SEPARATOR).count(),
            })
            .collect()
    }

    pub(in crate::environment_shell) fn vault_tree(&self, cx: &mut Context<Self>) -> AnyElement {
        let count = self.vault_rows().len();
        let list = uniform_list(
            TREE_ID,
            count,
            cx.processor(|this: &mut Self, range: Range<usize>, _: &mut Window, cx| {
                let rows = this.vault_rows();
                rows.get(range)
                    .unwrap_or_default()
                    .iter()
                    .map(|row| this.vault_row(row, cx))
                    .collect::<Vec<_>>()
            }),
        )
        .flex_1()
        .min_h_0();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .p(px(SPACE_1))
            .gap(px(SPACE_0_5))
            .child(Self::tree_toolbar(cx))
            .children(self.tag_filter_row(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this: &mut Self, event: &MouseDownEvent, _, cx| {
                            this.open_menu_at(event, Self::vault_folder_menu(""), cx);
                        }),
                    )
                    .on_drop(
                        cx.listener(|this: &mut Self, dragged: &DraggedFile, _, cx| {
                            if let Some(vault) = this.vault_file("") {
                                Self::move_file(dragged.path.clone(), vault, cx);
                            }
                        }),
                    )
                    .child(list),
            )
            .into_any_element()
    }

    fn tree_toolbar(cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let tool = |id: &'static str, glyph: IconName, tooltip: &'static str| {
            Button::new(id).icon(glyph).small().tooltip(tooltip)
        };
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_0_5))
            .h(px(NAV_ROW_H))
            .pl(px(SPACE_2))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_muted))
                    .child(VAULT_LABEL),
            )
            .child(
                tool(BTN_NEW_NOTE, IconName::FileAdd, TOOLTIP_NEW_NOTE).on_click(cx.listener(
                    |this: &mut Self, _, _, cx| {
                        let folder = this.new_entry_vault_folder();
                        this.run_note_action(NoteAction::NewNote(folder), cx);
                    },
                )),
            )
            .child(
                tool(BTN_NEW_FOLDER, IconName::FolderAdd, TOOLTIP_NEW_FOLDER).on_click(
                    cx.listener(|this: &mut Self, _, _, cx| {
                        let folder = this.new_entry_vault_folder();
                        this.run_note_action(NoteAction::NewFolder(folder), cx);
                    }),
                ),
            )
            .child(
                tool(BTN_COLLAPSE, IconName::CollapseAll, TOOLTIP_COLLAPSE).on_click(cx.listener(
                    |this: &mut Self, _, _, cx| {
                        this.notes.expanded.clear();
                        cx.notify();
                    },
                )),
            )
            .child(
                tool(BTN_GRAPH, IconName::Flow, TOOLTIP_GRAPH)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.open_vault_graph(cx))),
            )
            .child(
                tool(BTN_GENERATE, IconName::Magic, TOOLTIP_GENERATE).on_click(cx.listener(
                    |this: &mut Self, _, window, cx| this.open_docs_generate(window, cx),
                )),
            )
            .into_any_element()
    }

    fn tag_filter_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pal = palette(cx);
        let tag = self.notes.tag_filter.clone()?;
        Some(
            div()
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(px(SPACE_1))
                .h(px(NAV_ROW_H))
                .px(px(SPACE_2))
                .rounded(px(RADIUS_XS))
                .bg(pal.bg_hover)
                .text_size(px(TEXT_XS))
                .text_color(rgb(pal.accent))
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .child(format!("{TAG_PREFIX}{tag}")),
                )
                .child(
                    Button::new(BTN_CLEAR_TAG)
                        .icon(IconName::Close)
                        .small()
                        .tooltip(TOOLTIP_CLEAR_TAG)
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.notes.tag_filter = None;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    fn new_entry_vault_folder(&self) -> String {
        let Some(selected) = self.notes.selected.as_deref() else {
            return String::new();
        };
        let is_folder = self
            .notes
            .entries
            .iter()
            .any(|entry| entry.path == selected && entry.kind == VaultEntryKind::Folder);
        if is_folder {
            selected.to_owned()
        } else {
            parent_of(selected).to_owned()
        }
    }

    fn vault_folder_menu(folder: &str) -> Vec<MenuEntry> {
        vec![
            MenuEntry::item(
                NEW_NOTE_LABEL,
                Some(IconName::FileAdd),
                MenuAction::Note(NoteAction::NewNote(folder.to_owned())),
            ),
            MenuEntry::item(
                NEW_FOLDER_LABEL,
                Some(IconName::FolderAdd),
                MenuAction::Note(NoteAction::NewFolder(folder.to_owned())),
            ),
        ]
    }

    fn vault_entry_menu(&self, entry: &VaultEntry) -> Vec<MenuEntry> {
        let mut entries = Vec::new();
        match entry.kind {
            VaultEntryKind::Folder => {
                entries.extend(Self::vault_folder_menu(&entry.path));
                entries.push(MenuEntry::Separator);
            }
            VaultEntryKind::Note => {
                entries.push(MenuEntry::item(
                    OPEN_LABEL,
                    Some(IconName::Note),
                    MenuAction::Note(NoteAction::Open(entry.path.clone())),
                ));
                entries.push(MenuEntry::Separator);
            }
            VaultEntryKind::Attachment => {}
        }
        let Some(file) = self.vault_file(&entry.path) else {
            return entries;
        };
        entries.extend([
            MenuEntry::item(
                RENAME_LABEL,
                None,
                MenuAction::File(FileAction::Rename(file.clone())),
            ),
            MenuEntry::item(
                COPY_PATH_LABEL,
                None,
                MenuAction::File(FileAction::CopyPath(file.clone())),
            ),
            MenuEntry::item(
                REVEAL_LABEL,
                Some(IconName::Folder),
                MenuAction::File(FileAction::Reveal(file.clone())),
            ),
            MenuEntry::Separator,
            MenuEntry::destructive(
                DELETE_LABEL,
                IconName::Close,
                MenuAction::File(FileAction::Delete(file)),
            ),
        ]);
        entries
    }

    fn vault_row_label(&self, entry: &VaultEntry) -> String {
        let name = name_of(&entry.path);
        let shown = match entry.kind {
            VaultEntryKind::Note => name.strip_suffix(NOTE_SUFFIX).unwrap_or(name),
            _ => name,
        };
        self.vault_file(&entry.path)
            .and_then(|file| self.file_rename_label(&file))
            .unwrap_or_else(|| shown.to_owned())
    }

    fn vault_row(&self, row: &TreeRow<'_>, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let entry = row.entry;
        let path = entry.path.clone();
        let is_folder = entry.kind == VaultEntryKind::Folder;
        let expanded = self.notes.expanded.contains(&entry.path);
        let open = self
            .note_tabs()
            .and_then(|tabs| tabs.active_tab())
            .is_some_and(|tab| tab.path == entry.path);
        let highlighted = open || self.notes.selected.as_deref() == Some(entry.path.as_str());
        let label = self.vault_row_label(entry);
        let glyph = entry_glyph(entry.kind);
        let chevron = is_folder.then(|| {
            icon(
                if expanded {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                },
                ICON_XS,
                pal.text_subtle,
            )
        });
        let menu = self.vault_entry_menu(entry);
        let dragged = self
            .vault_file(&entry.path)
            .map(|file| DraggedFile::labelled(file, label.clone(), glyph));
        let clicked = path.clone();
        let pinned = path.clone();
        let attachment = (entry.kind == VaultEntryKind::Attachment)
            .then(|| self.vault_file(&entry.path))
            .flatten();
        let row = div()
            .id((ROW_PREFIX, row_key(&entry.path)))
            .flex()
            .flex_row()
            .w_full()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(ROW_H))
            .pl(px(SPACE_2 + indent(row.depth)))
            .pr(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(if entry.kind == VaultEntryKind::Attachment {
                pal.text_muted
            } else {
                pal.text_default
            }))
            .when(highlighted, |row| row.bg(row_fill(pal, cx)))
            .when(!highlighted, |row| {
                row.hover(|style| style.bg(pal.bg_hover))
            })
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.notes.selected = Some(clicked.clone());
                if is_folder {
                    if !this.notes.expanded.remove(&clicked) {
                        this.notes.expanded.insert(clicked.clone());
                    }
                    cx.notify();
                } else if let Some(file) = attachment.clone() {
                    cx.open_with_system(&file);
                } else {
                    this.open_vault_note(&clicked, FileOpening::Preview, cx);
                }
            }))
            .when(entry.kind == VaultEntryKind::Note, |row| {
                row.on_double_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.open_vault_note(&pinned, FileOpening::Pinned, cx);
                }))
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.open_menu_at(event, menu.clone(), cx);
                }),
            );
        self.with_vault_drag(row, entry, dragged, cx)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(ICON_SM))
                    .children(chevron),
            )
            .child(icon(glyph, ICON_SM, pal.text_muted))
            .child(div().flex_1().min_w_0().truncate().child(label))
            .into_any_element()
    }
}

impl EnvironmentShell {
    fn with_vault_drag(
        &self,
        row: Stateful<Div>,
        entry: &VaultEntry,
        dragged: Option<DraggedFile>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let is_folder = entry.kind == VaultEntryKind::Folder;
        let drop_dir = self.vault_file(&entry.path).and_then(|file| {
            if is_folder {
                Some(file)
            } else {
                file.parent().map(std::path::Path::to_path_buf)
            }
        });
        row.when_some(dragged, |row, dragged| {
            row.on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
        })
        .when(is_folder, |row| {
            row.drag_over::<DraggedFile>(|style, _, _, cx| style.bg(palette(cx).bg_selected))
        })
        .on_drop(
            cx.listener(move |_: &mut Self, dragged: &DraggedFile, _, cx| {
                cx.stop_propagation();
                if let Some(dir) = drop_dir.clone() {
                    Self::move_file(dragged.path.clone(), dir, cx);
                }
            }),
        )
    }
}

fn entry_glyph(kind: VaultEntryKind) -> IconName {
    match kind {
        VaultEntryKind::Folder => IconName::Folder,
        VaultEntryKind::Note => IconName::Note,
        VaultEntryKind::Attachment => IconName::Attach,
    }
}

fn row_key(path: &str) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}
