use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, StatefulInteractiveElement, Styled,
};
use std::path::{Path, PathBuf};

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::file_tree::FileAction;
use super::super::EnvironmentShell;
use crate::components::{icon, tooltip_view, Button, ButtonVariant, EmptyState, IconName};
use crate::theme::{
    palette, ICON_SM, ICON_XS, LINE_SM, NAV_ROW_H, RADIUS_SM, RADIUS_XS, SPACE_0_5, SPACE_1,
    SPACE_1_5, SPACE_2, TEXT_SM,
};

const ROOT_HEADER_ID: &str = "side-files-root";
const BTN_GROUP_ROOT: &str = "side-files-group-root";
const BTN_COLLAPSE: &str = "side-files-collapse";
const BTN_SEARCH: &str = "side-files-search";
const BTN_NEW_ENTRY: &str = "side-files-new-entry";
const TOOLTIP_NEW_ENTRY: &str = "New file or folder";
const NEW_FILE_LABEL: &str = "New file";
const NEW_FOLDER_LABEL: &str = "New folder";
const HEADER_H: f32 = NAV_ROW_H + SPACE_1_5;
const TOOLTIP_COLLAPSE: &str = "Collapse all folders";
const TOOLTIP_SEARCH: &str = "Search in files";
const TOOLTIP_GROUP_ROOT: &str = "Back to the group's folder";
const TOOLTIP_PARENTS: &str = "Open a folder above this one";
const GROUP_ROOT_LABEL: &str = "Group's folder";
const ROOT_LABEL: &str = "/";
const NO_FOLDER_TITLE: &str = "No folder";
const NO_FOLDER_TEXT: &str = "Select a group to browse its files.";

fn folder_label(folder: &Path) -> String {
    folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| ROOT_LABEL.to_owned())
}

pub(in crate::environment_shell) fn parent_folders(path: &Path) -> Vec<(String, PathBuf)> {
    path.ancestors()
        .skip(1)
        .map(|folder| (folder_label(folder), folder.to_path_buf()))
        .collect()
}

impl EnvironmentShell {
    pub(super) fn side_files(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(root) = self.file_tree_root().map(Path::to_path_buf) else {
            return EmptyState::new(IconName::Folder, NO_FOLDER_TITLE, NO_FOLDER_TEXT)
                .into_any_element();
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .gap(px(SPACE_0_5))
                    .p(px(SPACE_1))
                    .child(self.root_header(&root, cx))
                    .child(if self.side_search_open() {
                        self.side_search_view(cx)
                    } else {
                        self.file_tree_list(cx)
                    }),
            )
            .into_any_element()
    }

    fn explorer_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let searching = self.side_search_open();
        let tool = |id: &'static str, glyph: IconName, tooltip: &'static str| {
            Button::new(id).icon(glyph).small().tooltip(tooltip)
        };
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_0_5))
            .p(px(SPACE_0_5))
            .rounded(px(RADIUS_SM))
            .bg(pal.bg_hover)
            .child(
                tool(BTN_SEARCH, IconName::Search, TOOLTIP_SEARCH)
                    .variant(if searching {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Ghost
                    })
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.toggle_side_search(window, cx);
                    })),
            )
            .child(
                tool(BTN_COLLAPSE, IconName::CollapseAll, TOOLTIP_COLLAPSE)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.collapse_file_tree(cx))),
            )
            .child(
                tool(BTN_NEW_ENTRY, IconName::Add, TOOLTIP_NEW_ENTRY).on_click(cx.listener(
                    |this: &mut Self, event: &gpui_kit::ClickEvent, _, cx| {
                        this.close_side_search(cx);
                        let Some(dir) = this.new_entry_folder() else {
                            return;
                        };
                        let at = event.position();
                        let entries = vec![
                            MenuEntry::item(
                                NEW_FILE_LABEL,
                                Some(IconName::FileAdd),
                                MenuAction::File(FileAction::NewFile(dir.clone())),
                            ),
                            MenuEntry::item(
                                NEW_FOLDER_LABEL,
                                Some(IconName::FolderAdd),
                                MenuAction::File(FileAction::NewFolder(dir)),
                            ),
                        ];
                        this.open_menu(
                            (at.x.as_f32(), at.y.as_f32()),
                            MenuPlacement::Below,
                            entries,
                            cx,
                        );
                    },
                )),
            )
            .into_any_element()
    }

    fn root_header(&self, root: &Path, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let away = self.file_tree_navigated();
        let parents = root.to_path_buf();
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(HEADER_H))
            .child(
                div()
                    .id(ROOT_HEADER_ID)
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .h_full()
                    .px(px(SPACE_2))
                    .rounded(px(RADIUS_XS))
                    .cursor_pointer()
                    .hover(|style| style.bg(pal.bg_hover))
                    .tooltip(|_, cx| tooltip_view(TOOLTIP_PARENTS.into(), cx))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let entries = this.parent_menu_entries(&parents);
                            this.open_menu_at(event, entries, cx);
                        }),
                    )
                    .child(icon(IconName::Folder, ICON_SM, pal.text_muted))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_SM))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(pal.text_strong))
                            .child(folder_label(root)),
                    )
                    .child(icon(IconName::ChevronDown, ICON_XS, pal.text_muted)),
            )
            .when(away, |bar| {
                bar.child(
                    Button::new(BTN_GROUP_ROOT)
                        .icon(IconName::Home)
                        .small()
                        .tooltip(TOOLTIP_GROUP_ROOT)
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.return_file_tree(cx);
                        })),
                )
            })
            .child(self.explorer_actions(cx))
            .into_any_element()
    }

    fn parent_menu_entries(&self, root: &Path) -> Vec<MenuEntry> {
        let folder_item = |label: String, folder: PathBuf| {
            MenuEntry::item(
                label,
                Some(IconName::Folder),
                MenuAction::File(FileAction::ShowFolder(folder)),
            )
        };
        let mut entries = Vec::new();
        if let Some(group_root) = self
            .active_group()
            .map(|group| group.root.clone())
            .filter(|_| self.file_tree_navigated())
        {
            entries.push(MenuEntry::item(
                GROUP_ROOT_LABEL,
                Some(IconName::Home),
                MenuAction::File(FileAction::ShowFolder(group_root)),
            ));
            entries.push(MenuEntry::Separator);
        }
        entries.extend(
            parent_folders(root)
                .into_iter()
                .map(|(label, folder)| folder_item(label, folder)),
        );
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folders_above_run_from_the_nearest_to_the_disk_root() {
        let parents = parent_folders(Path::new("/Users/ana/project"));
        let labels: Vec<&str> = parents.iter().map(|(label, _)| label.as_str()).collect();
        assert_eq!(labels, ["ana", "Users", "/"]);
        assert_eq!(parents[0].1, PathBuf::from("/Users/ana"));
        assert_eq!(parents[2].1, PathBuf::from("/"));
        assert_eq!(folder_label(Path::new("/Users/ana/project")), "project");
    }
}
