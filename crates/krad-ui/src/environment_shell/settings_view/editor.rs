use gpui_kit::{
    div, px, AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
};
use krad_domain::{EditorSettings, FileIconColors, Minimap, ProblemLens, ProjectCheckLimit};

use super::super::dialog_parts::{segment, SegmentSize};
use super::{form_row, panel_card, segmented_track, EnvironmentShell};
use crate::theme::{palette, SPACE_3};

const LENS_LABEL: &str = "Problem lens";
const LENS_HINT: &str =
    "Writes the message of a line's most severe problem after the end of the line.";
const LENS_PREFIX: &str = "editor-problem-lens";
const MINIMAP_LABEL: &str = "Minimap";
const MINIMAP_HINT: &str =
    "A scaled-down picture of the whole file beside its text; drag it to scroll.";
const MINIMAP_PREFIX: &str = "editor-minimap";
const LIMIT_LABEL: &str = "Project check limit";
const LIMIT_HINT: &str = "The most files of one language a group's project check opens for language servers that only check open files. Past it, Problems says the check is incomplete.";
const LIMIT_PREFIX: &str = "editor-project-check-limit";
const ICONS_LABEL: &str = "File icon colors";
const ICONS_HINT: &str =
    "The colors of the file tree's icons; Automatic follows the app's light or dark look.";
const ICONS_PREFIX: &str = "editor-file-icon-colors";

impl EnvironmentShell {
    fn edit_editor_settings(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut EditorSettings),
    ) {
        edit(&mut self.settings.editor);
        self.save_settings(cx);
        cx.notify();
    }

    pub(super) fn editor_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.editor;
        let lenses = ProblemLens::ALL.map(|lens| {
            segment(
                (LENS_PREFIX, lens as u64),
                lens.display_name(),
                None,
                current.problem_lens == lens,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_editor_settings(cx, |editor| editor.problem_lens = lens);
            }))
        });
        let minimaps = Minimap::ALL.map(|minimap| {
            segment(
                (MINIMAP_PREFIX, minimap as u64),
                minimap.display_name(),
                None,
                current.minimap == minimap,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_editor_settings(cx, |editor| editor.minimap = minimap);
            }))
        });
        let limits = ProjectCheckLimit::PRESETS.map(|limit| {
            segment(
                (LIMIT_PREFIX, u64::from(limit.files())),
                limit.files().to_string(),
                None,
                current.project_check_limit == limit,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_editor_settings(cx, |editor| editor.project_check_limit = limit);
            }))
        });
        let icon_colors = FileIconColors::ALL.map(|colors| {
            segment(
                (ICONS_PREFIX, colors as u64),
                colors.display_name(),
                None,
                self.settings.file_icon_colors == colors,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.settings.file_icon_colors = colors;
                this.save_settings(cx);
                cx.notify();
            }))
        });
        let rows = vec![
            form_row(
                LENS_LABEL,
                Some(LENS_HINT),
                segmented_track(pal).children(lenses).into_any_element(),
                pal,
            ),
            form_row(
                MINIMAP_LABEL,
                Some(MINIMAP_HINT),
                segmented_track(pal).children(minimaps).into_any_element(),
                pal,
            ),
            form_row(
                ICONS_LABEL,
                Some(ICONS_HINT),
                segmented_track(pal)
                    .children(icon_colors)
                    .into_any_element(),
                pal,
            ),
            form_row(
                LIMIT_LABEL,
                Some(LIMIT_HINT),
                segmented_track(pal).children(limits).into_any_element(),
                pal,
            ),
        ];
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_3))
            .child(self.font_card(super::FontZone::Editor, cx))
            .child(panel_card(rows, pal))
            .into_any_element()
    }
}
