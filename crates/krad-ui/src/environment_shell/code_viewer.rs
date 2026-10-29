mod lines;
mod selection;

use gpui_kit::base::TextView as BaseTextView;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, ElementId, FocusHandle, FontWeight,
    InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, UniformListScrollHandle,
};
use krad_repo::{ChangeKind, RepoError};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::dialog_parts::{segment, SegmentSize};
use super::file_tree::change_color;
use super::layout::{width_handle, SidePanel};
use super::markdown_images::with_local_images;
use super::settings_view::segmented_track;
use super::EnvironmentShell;
use crate::components::{card, chip, icon, Button, IconName};
use crate::highlight::{highlight_lines, Language, Span};
use crate::theme::{
    palette, Palette, CARD_HEADER_H, ICON_SM, LINE_SM, LINE_XS, SPACE_2, SPACE_3, TEXT_SM, TEXT_XS,
};
use selection::CodeSelection;

const BTN_CLOSE_VIEWER: &str = "close-viewer";
const TOOLTIP_CLOSE_VIEWER: &str = "Close file";
const MAX_VIEW_BYTES: u64 = 2 * 1024 * 1024;
const TAB_AS_SPACES: &str = "    ";
const LOADING_TEXT: &str = "Loading…";
const BINARY_TEXT: &str = "Binary file, not shown";
const TRUNCATED_TEXT: &str = "Showing the first 2 MB";
const VIEWER_FRACTION: f32 = 0.5;
pub(super) const VIEWER_MIN_W: f32 = 360.0;
const VIEWER_MAX_W: f32 = 960.0;
const FOOTER_H: f32 = 28.0;
const LINES_SUFFIX: &str = "lines";
const ENCODING_LABEL: &str = "UTF-8";
const READ_ONLY_LABEL: &str = "Read only";
const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown"];
const PREVIEW_ID: &str = "code-viewer-preview";
const SEGMENT_SOURCE_ID: &str = "code-viewer-source";
const SEGMENT_PREVIEW_ID: &str = "code-viewer-rendered";
const SOURCE_LABEL: &str = "Markdown";
const PREVIEW_LABEL: &str = "Preview";
const LANGUAGE_NAMES: &[(&str, &str)] = &[
    ("rs", "Rust"),
    ("ts", "TypeScript"),
    ("tsx", "TypeScript"),
    ("js", "JavaScript"),
    ("jsx", "JavaScript"),
    ("mjs", "JavaScript"),
    ("cjs", "JavaScript"),
    ("py", "Python"),
    ("go", "Go"),
    ("c", "C"),
    ("h", "C"),
    ("cc", "C++"),
    ("cpp", "C++"),
    ("hpp", "C++"),
    ("cs", "C#"),
    ("java", "Java"),
    ("kt", "Kotlin"),
    ("swift", "Swift"),
    ("css", "CSS"),
    ("scss", "SCSS"),
    ("html", "HTML"),
    ("sh", "Shell"),
    ("bash", "Shell"),
    ("zsh", "Shell"),
    ("ps1", "PowerShell"),
    ("toml", "TOML"),
    ("yaml", "YAML"),
    ("yml", "YAML"),
    ("json", "JSON"),
    ("md", "Markdown"),
];

pub(super) struct CodeView {
    pub(super) path: PathBuf,
    label: String,
    content: ViewerContent,
    scroll: UniformListScrollHandle,
    mode: ViewerMode,
    selection: Option<CodeSelection>,
    focus: FocusHandle,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum ViewerMode {
    #[default]
    Source,
    Preview,
}

enum ViewerContent {
    Loading,
    Text(Arc<ViewerText>),
    Unavailable(String),
}

struct ViewerText {
    lines: Vec<SharedString>,
    spans: Vec<Vec<Span>>,
    widest: usize,
    truncated: bool,
    markdown: Option<SharedString>,
}

fn load(path: &Path) -> ViewerContent {
    match krad_repo::read_text(path, MAX_VIEW_BYTES) {
        Ok(file) => {
            let text = file.text.replace('\t', TAB_AS_SPACES);
            let spans = highlight_lines(&text, Language::from_path(path));
            let markdown = is_markdown(path).then(|| SharedString::from(text.clone()));
            let lines: Vec<SharedString> = text
                .lines()
                .map(|line| SharedString::from(line.to_owned()))
                .collect();
            ViewerContent::Text(Arc::new(ViewerText {
                widest: widest_line(&lines),
                lines,
                spans,
                truncated: file.truncated,
                markdown,
            }))
        }
        Err(RepoError::Binary) => ViewerContent::Unavailable(BINARY_TEXT.to_owned()),
        Err(error) => ViewerContent::Unavailable(error.to_string()),
    }
}

fn language_name(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    LANGUAGE_NAMES
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, name)| *name)
}

fn widest_line(lines: &[SharedString]) -> usize {
    lines
        .iter()
        .enumerate()
        .max_by_key(|(_, line)| line.chars().count())
        .map_or(0, |(index, _)| index)
}

fn is_markdown(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        MARKDOWN_EXTENSIONS
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    })
}

impl EnvironmentShell {
    pub(super) fn open_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let label = self
            .active_group()
            .and_then(|group| path.strip_prefix(&group.root).ok())
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        self.viewer = Some(CodeView {
            path: path.clone(),
            label,
            content: ViewerContent::Loading,
            scroll: UniformListScrollHandle::new(),
            mode: ViewerMode::default(),
            selection: None,
            focus: cx.focus_handle(),
        });
        cx.notify();
        let loading = cx.background_spawn({
            let path = path.clone();
            async move { load(&path) }
        });
        cx.spawn(async move |this, cx| {
            let content = loading.await;
            this.update(cx, |shell, cx| {
                if let Some(viewer) = shell.viewer.as_mut().filter(|viewer| viewer.path == path) {
                    viewer.content = content;
                    cx.notify();
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn set_viewer_mode(&mut self, mode: ViewerMode, cx: &mut Context<Self>) {
        if let Some(viewer) = self.viewer.as_mut() {
            viewer.mode = mode;
            cx.notify();
        }
    }

    pub(super) fn close_viewer(&mut self, cx: &mut Context<Self>) {
        self.viewer = None;
        cx.notify();
    }

    pub(super) fn viewer_width(&self, card_width: f32) -> f32 {
        if self.viewer.is_none() {
            return 0.0;
        }
        self.panel_widths.viewer_in(card_width).unwrap_or_else(|| {
            (card_width * VIEWER_FRACTION).clamp(VIEWER_MIN_W, VIEWER_MAX_W.min(card_width))
        })
    }

    fn viewer_change(&self, viewer: &CodeView) -> Option<ChangeKind> {
        let path = &viewer.path;
        let root = &self.active_group()?.root;
        let git = self.git.get(root)?;
        let relative = path.strip_prefix(&git.repo_root).ok()?;
        git.status
            .changes
            .iter()
            .find(|change| change.path == relative)
            .map(|change| change.kind)
    }

    pub(super) fn code_viewer(&self, width: f32, cx: &mut Context<Self>) -> Option<AnyElement> {
        let viewer = self.viewer.as_ref()?;
        let pal = palette(cx);
        let change = self.viewer_change(viewer);
        let text = match &viewer.content {
            ViewerContent::Text(text) => Some(text),
            ViewerContent::Loading | ViewerContent::Unavailable(_) => None,
        };
        let group = self.active_group().map(|group| group.name.clone());
        let markdown = text.and_then(|text| text.markdown.clone());
        let mode_toggle = markdown
            .is_some()
            .then(|| Self::viewer_mode_toggle(viewer.mode, cx));
        let header = div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(CARD_HEADER_H))
            .pl(px(SPACE_3))
            .pr(px(SPACE_2))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .child(icon(IconName::File, ICON_SM, pal.text_muted))
            .child(
                div()
                    .flex_shrink_0()
                    .max_w_1_2()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_strong))
                    .child(viewer.label.clone()),
            )
            .children(group.map(|name| {
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(pal.text_muted))
                    .child(name)
            }))
            .child(div().flex_1())
            .when_some(change, |header, kind| {
                header.child(
                    div()
                        .size(px(SPACE_2))
                        .rounded_full()
                        .bg(rgb(change_color(kind, pal))),
                )
            })
            .children(mode_toggle)
            .children(language_name(&viewer.path).map(|name| chip(name, pal)))
            .child(
                Button::new(BTN_CLOSE_VIEWER)
                    .icon(IconName::Close)
                    .small()
                    .tooltip(TOOLTIP_CLOSE_VIEWER)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.close_viewer(cx))),
            );
        let body = match &viewer.content {
            ViewerContent::Loading => Self::viewer_note(LOADING_TEXT, pal),
            ViewerContent::Unavailable(reason) => Self::viewer_note(reason, pal),
            ViewerContent::Text(text) => match (viewer.mode, markdown) {
                (ViewerMode::Preview, Some(markdown)) => {
                    Self::markdown_preview(&markdown, viewer.path.parent(), pal)
                }
                _ => Self::code_lines(text, viewer, cx),
            },
        };
        Some(
            card(pal, cx)
                .relative()
                .flex_none()
                .w(px(width))
                .h_full()
                .child(header)
                .child(body)
                .children(text.map(|text| Self::viewer_footer(text, pal)))
                .child(width_handle(SidePanel::Viewer, width, cx))
                .into_any_element(),
        )
    }

    fn viewer_mode_toggle(mode: ViewerMode, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let option = |id: &'static str, label: &'static str, value: ViewerMode| {
            segment(
                id,
                label,
                None,
                mode == value,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| this.set_viewer_mode(value, cx)))
        };
        segmented_track(pal)
            .flex_none()
            .child(option(
                SEGMENT_PREVIEW_ID,
                PREVIEW_LABEL,
                ViewerMode::Preview,
            ))
            .child(option(SEGMENT_SOURCE_ID, SOURCE_LABEL, ViewerMode::Source))
            .into_any_element()
    }

    fn markdown_preview(
        markdown: &SharedString,
        folder: Option<&Path>,
        pal: &Palette,
    ) -> AnyElement {
        let view =
            BaseTextView::markdown(ElementId::from(PREVIEW_ID), markdown.clone()).selectable(true);
        let view = match folder {
            Some(folder) => with_local_images(view, folder, markdown),
            None => view,
        };
        div()
            .id(PREVIEW_ID)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(SPACE_3))
            .text_size(px(TEXT_SM))
            .text_color(rgb(pal.text_strong))
            .child(view)
            .into_any_element()
    }

    fn viewer_footer(text: &ViewerText, pal: &Palette) -> AnyElement {
        let lines = if text.truncated {
            TRUNCATED_TEXT.to_owned()
        } else {
            format!("{} {LINES_SUFFIX}", text.lines.len())
        };
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_3))
            .h(px(FOOTER_H))
            .px(px(SPACE_3))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .text_color(rgb(pal.text_muted))
            .child(lines)
            .child(ENCODING_LABEL)
            .child(div().flex_1())
            .child(READ_ONLY_LABEL)
            .into_any_element()
    }

    fn viewer_note(text: &str, pal: &Palette) -> AnyElement {
        div()
            .flex_1()
            .p(px(SPACE_3))
            .text_size(px(TEXT_SM))
            .text_color(rgb(pal.text_subtle))
            .child(text.to_owned())
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_extensions_name_their_language_in_any_case() {
        assert_eq!(language_name(Path::new("api/index.ts")), Some("TypeScript"));
        assert_eq!(language_name(Path::new("src/MAIN.RS")), Some("Rust"));
        assert_eq!(language_name(Path::new("LICENSE")), None);
        assert_eq!(language_name(Path::new("data.bin")), None);
    }

    #[test]
    fn the_widest_line_counts_characters_not_bytes() {
        let lines = ["short", "ñññññññ", "longer"].map(SharedString::from);
        assert_eq!(widest_line(&lines), 1);
        assert_eq!(widest_line(&[]), 0);
    }

    #[test]
    fn only_markdown_files_can_render_in_any_case() {
        assert!(is_markdown(Path::new("docs/README.md")));
        assert!(is_markdown(Path::new("NOTES.Markdown")));
        assert!(!is_markdown(Path::new("src/main.rs")));
        assert!(!is_markdown(Path::new("md")));
    }

    #[test]
    fn every_extension_is_listed_once() {
        for (index, (extension, _)) in LANGUAGE_NAMES.iter().enumerate() {
            assert!(
                !LANGUAGE_NAMES[index + 1..]
                    .iter()
                    .any(|(other, _)| other == extension),
                "{extension} is listed twice"
            );
        }
    }
}
