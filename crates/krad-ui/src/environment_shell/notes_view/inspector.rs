use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{outline, FileOpening, Note, NoteMode};

mod properties;

pub(in crate::environment_shell) use properties::PropertyEdit;

use super::super::EnvironmentShell;
use crate::components::{Button, IconName};
use crate::theme::{
    palette, row_fill, LINE_SM, LINE_XS, NAV_ROW_H, RADIUS_XS, SPACE_0_5, SPACE_1, SPACE_2,
    SPACE_3, TEXT_SM, TEXT_XS,
};

const TAB_PREFIX: &str = "inspector-tab";
const INSPECTOR_BODY_ID: &str = "inspector-body";
const ROW_PREFIX: &str = "inspector-row";
const NOTHING_LINKS: &str = "No note links here yet.";
const NO_HEADINGS: &str = "This note has no headings.";
const NO_TAGS: &str = "No tags in the vault.";
const NO_NOTE: &str = "Open a note to see it here.";
const HEADING_INDENT: f32 = 12.0;
const TAG_PREFIX: &str = "#";
const TABS: [InspectorTab; 5] = [
    InspectorTab::Backlinks,
    InspectorTab::Outline,
    InspectorTab::Tags,
    InspectorTab::Properties,
    InspectorTab::Graph,
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) enum InspectorTab {
    #[default]
    Backlinks,
    Outline,
    Tags,
    Properties,
    Graph,
}

impl InspectorTab {
    const fn glyph(self) -> IconName {
        match self {
            Self::Backlinks => IconName::Exchange,
            Self::Outline => IconName::Layers,
            Self::Tags => IconName::Flag,
            Self::Properties => IconName::Settings,
            Self::Graph => IconName::Flow,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Backlinks => "Backlinks",
            Self::Outline => "Outline",
            Self::Tags => "Tags",
            Self::Properties => "Properties",
            Self::Graph => "Local graph",
        }
    }
}

impl EnvironmentShell {
    fn open_note_path(&self) -> Option<String> {
        self.note_tabs()
            .and_then(|tabs| tabs.active_tab())
            .map(|tab| tab.path.clone())
    }

    pub(in crate::environment_shell) fn inspector(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let shown = self.notes.inspector;
        let tabs = TABS
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let tab = *tab;
                let active = tab == shown;
                let button = Button::new((TAB_PREFIX, index))
                    .icon(tab.glyph())
                    .small()
                    .tooltip(tab.label())
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.notes.inspector = tab;
                        cx.notify();
                    }))
                    .into_any_element();
                marked(button, active, pal)
            })
            .collect::<Vec<_>>();
        let body = match (shown, self.open_note_path()) {
            (InspectorTab::Tags, _) => self.tags_list(cx),
            (_, None) => Self::inspector_hint(NO_NOTE, cx),
            (InspectorTab::Backlinks, Some(path)) => self.backlinks_list(&path, cx),
            (InspectorTab::Outline, Some(path)) => self.outline_list(&path, cx),
            (InspectorTab::Properties, Some(path)) => self.properties_list(&path, cx),
            (InspectorTab::Graph, Some(path)) => self.local_graph(&path, cx),
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_none()
                    .items_center()
                    .justify_around()
                    .h(px(NAV_ROW_H + SPACE_2))
                    .border_b_1()
                    .border_color(pal.divider)
                    .children(tabs),
            )
            .child(
                div()
                    .px(px(SPACE_3))
                    .pt(px(SPACE_2))
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_muted))
                    .child(shown.label().to_uppercase()),
            )
            .child(
                div()
                    .id(INSPECTOR_BODY_ID)
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(SPACE_1))
                    .gap(px(SPACE_0_5))
                    .child(body),
            )
            .into_any_element()
    }

    fn inspector_hint(text: &'static str, cx: &mut Context<Self>) -> AnyElement {
        div()
            .px(px(SPACE_2))
            .py(px(SPACE_1))
            .text_size(px(TEXT_SM))
            .text_color(rgb(palette(cx).text_muted))
            .child(text)
            .into_any_element()
    }

    fn inspector_row(id: usize, cx: &mut Context<Self>) -> gpui_kit::Stateful<gpui_kit::Div> {
        let pal = palette(cx);
        div()
            .id((ROW_PREFIX, id))
            .flex()
            .flex_col()
            .gap(px(SPACE_0_5))
            .px(px(SPACE_2))
            .py(px(SPACE_1))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .hover(|style| style.bg(pal.bg_hover))
    }

    fn backlinks_list(&self, path: &str, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let backlinks = self.notes.index.backlinks(path);
        if backlinks.is_empty() {
            return Self::inspector_hint(NOTHING_LINKS, cx);
        }
        let rows: Vec<AnyElement> = backlinks
            .into_iter()
            .enumerate()
            .map(|(index, link)| {
                let title = self
                    .notes
                    .index
                    .note(&link.from)
                    .map_or_else(|| link.from.clone(), |note| note.title.clone());
                let from = link.from.clone();
                Self::inspector_row(index, cx)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.open_vault_note(&from, FileOpening::Preview, cx);
                    }))
                    .child(
                        div()
                            .text_color(rgb(pal.text_strong))
                            .truncate()
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(TEXT_XS))
                            .line_height(px(LINE_XS))
                            .text_color(rgb(pal.text_muted))
                            .child(link.text),
                    )
                    .into_any_element()
            })
            .collect();
        div().flex().flex_col().children(rows).into_any_element()
    }

    fn outline_list(&self, path: &str, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let Some(text) = self
            .notes
            .editors
            .get(path)
            .and_then(|editor| editor.text(cx))
        else {
            return Self::inspector_hint(NO_HEADINGS, cx);
        };
        let headings = outline(&Note::parse(&text).body);
        if headings.is_empty() {
            return Self::inspector_hint(NO_HEADINGS, cx);
        }
        let front_lines = text.lines().count() - Note::parse(&text).body.lines().count();
        let rows: Vec<AnyElement> = headings
            .into_iter()
            .enumerate()
            .map(|(nth, heading)| {
                let path = path.to_owned();
                let line = heading.line + front_lines;
                let indent = f32::from(heading.level.saturating_sub(1)) * HEADING_INDENT;
                Self::inspector_row(nth, cx)
                    .pl(px(SPACE_2 + indent))
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        this.go_to_heading(&path, nth, line, window, cx);
                    }))
                    .child(
                        div()
                            .truncate()
                            .text_color(rgb(if heading.level == 1 {
                                pal.text_strong
                            } else {
                                pal.text_default
                            }))
                            .child(heading.text),
                    )
                    .into_any_element()
            })
            .collect();
        div().flex().flex_col().children(rows).into_any_element()
    }

    fn go_to_heading(
        &mut self,
        path: &str,
        nth: usize,
        line: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mode = self
            .note_tabs()
            .and_then(|tabs| tabs.active_tab())
            .map(|tab| tab.mode);
        if matches!(mode, Some(NoteMode::Reading | NoteMode::Split)) {
            self.reveal_heading(path, nth, cx);
        }
        if matches!(mode, Some(NoteMode::Source | NoteMode::Split)) {
            if let Some(state) = self
                .notes
                .editors
                .get(path)
                .and_then(|editor| editor.state.clone())
            {
                let line = u32::try_from(line).unwrap_or(u32::MAX);
                state.update(cx, |state, cx| {
                    state.set_cursor_position(
                        gpui_kit::component::input::Position::new(line, 0),
                        window,
                        cx,
                    );
                    state.focus(window, cx);
                });
            }
        }
    }

    fn tags_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let tags = self.notes.index.tags();
        if tags.is_empty() {
            return Self::inspector_hint(NO_TAGS, cx);
        }
        let rows: Vec<AnyElement> = tags
            .into_iter()
            .enumerate()
            .map(|(index, (tag, count))| {
                let active = self.notes.tag_filter.as_deref() == Some(tag.as_str());
                let picked = tag.clone();
                Self::inspector_row(index, cx)
                    .flex_row()
                    .justify_between()
                    .when(active, |row| row.bg(row_fill(pal, cx)))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.notes.tag_filter =
                            if this.notes.tag_filter.as_deref() == Some(picked.as_str()) {
                                None
                            } else {
                                Some(picked.clone())
                            };
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_color(rgb(pal.accent))
                            .truncate()
                            .child(format!("{TAG_PREFIX}{tag}")),
                    )
                    .child(
                        div()
                            .text_color(rgb(pal.text_muted))
                            .child(count.to_string()),
                    )
                    .into_any_element()
            })
            .collect();
        div().flex().flex_col().children(rows).into_any_element()
    }
}

fn marked(tab: AnyElement, active: bool, pal: &crate::theme::Palette) -> AnyElement {
    div()
        .rounded(px(RADIUS_XS))
        .when(active, |tab| tab.bg(pal.bg_hover))
        .child(tab)
        .into_any_element()
}
