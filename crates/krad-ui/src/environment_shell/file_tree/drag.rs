use gpui_kit::{
    div, px, rgb, Context, FontWeight, IntoElement, ParentElement, Render, SharedString, Styled,
    Window,
};
use krad_repo::{DirEntry, EntryKind};
use std::path::PathBuf;

use crate::components::{icon, IconName};
use crate::theme::{
    floating_shadow, palette, ICON, LINE_SM, RADIUS_SM, SPACE_1_5, SPACE_2, TEXT_SM,
};

const DRAG_PREVIEW_H: f32 = 32.0;

#[derive(Clone)]
pub(in crate::environment_shell) struct DraggedFile {
    pub(in crate::environment_shell) path: PathBuf,
    label: SharedString,
    glyph: IconName,
}

impl DraggedFile {
    pub(super) fn new(entry: &DirEntry) -> Self {
        let glyph = match entry.kind {
            EntryKind::Directory => IconName::Folder,
            EntryKind::File => IconName::File,
        };
        Self::labelled(entry.path.clone(), entry.name.clone(), glyph)
    }

    pub(in crate::environment_shell) fn labelled(
        path: PathBuf,
        label: impl Into<SharedString>,
        glyph: IconName,
    ) -> Self {
        Self {
            path,
            label: label.into(),
            glyph,
        }
    }
}

impl Render for DraggedFile {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = palette(cx);
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(DRAG_PREVIEW_H))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_SM))
            .bg(rgb(pal.bg_surface))
            .shadow(floating_shadow(pal))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(pal.text_strong))
            .child(icon(self.glyph, ICON, pal.text_strong))
            .child(self.label.clone())
    }
}
