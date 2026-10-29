use gpui_kit::component::InteractiveElementExt as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{NoteMode, NoteTab};

use super::super::dialog_parts::{segment, SegmentSize};
use super::super::session_pane::tab_shell;
use super::super::EnvironmentShell;
use super::NoteEditor;
use crate::components::{icon, tooltip_view, Button, IconName};
use crate::theme::{palette, ICON_SM, RADIUS_XS, SPACE_0_5, SPACE_1};

const TAB_PREFIX: &str = "note-tab";
const CLOSE_PREFIX: &str = "note-tab-close";
const GRAPH_TAB_ID: &str = "note-tab-graph";
const GRAPH_CLOSE_ID: &str = "note-tab-graph-close";
const MODE_PREFIX: &str = "note-mode";
const BTN_CONTINUE: &str = "note-continue";
const BTN_INSPECTOR: &str = "note-inspector";
const GRAPH_LABEL: &str = "Graph";
const CLOSE_TOOLTIP: &str = "Close";
const CONTINUE_TOOLTIP: &str = "Ask an agent to write the next section of this note";
const INSPECTOR_TOOLTIP: &str = "Show or hide the inspector";
const DIRTY_MARK: &str = "•";
const CLOSE_BUTTON: f32 = 18.0;
const TAB_TITLE_MAX_W: f32 = 160.0;
const CLOSE_ICON: f32 = 12.0;
const PATH_SEPARATOR: char = '/';
const NOTE_SUFFIX: &str = ".md";
const MODES: [NoteMode; 3] = [NoteMode::Source, NoteMode::Reading, NoteMode::Split];

const fn mode_label(mode: NoteMode) -> &'static str {
    match mode {
        NoteMode::Source => "Source",
        NoteMode::Reading => "Reading",
        NoteMode::Split => "Split",
    }
}

fn tab_title(path: &str) -> &str {
    let name = path.rsplit(PATH_SEPARATOR).next().unwrap_or(path);
    name.strip_suffix(NOTE_SUFFIX).unwrap_or(name)
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn note_top_bar(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.notes.root.as_ref()?;
        let controls = self
            .note_tabs()
            .and_then(|tabs| tabs.active_tab())
            .filter(|_| !self.notes.graph.showing)
            .map(|tab| self.note_controls(&tab.path, tab.mode, cx));
        let tabs: Vec<NoteTab> = self.note_tabs().map_or_default(|tabs| tabs.tabs.clone());
        let active = self.note_tabs().and_then(|tabs| tabs.active);
        let graph_shown = self.notes.graph.showing;
        let mut items: Vec<AnyElement> = Vec::new();
        if self.notes.graph.open {
            items.push(Self::graph_tab(graph_shown, cx));
        }
        for (index, tab) in tabs.iter().enumerate() {
            let shown = !graph_shown && active == Some(index);
            items.push(self.note_tab(index, tab, shown, cx));
        }
        let strip = div()
            .flex()
            .flex_row()
            .flex_1()
            .min_w_0()
            .h_full()
            .items_center()
            .gap(px(SPACE_0_5))
            .occlude()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(SPACE_0_5))
                    .overflow_hidden()
                    .children(items),
            )
            .children(controls)
            .into_any_element();
        Some(strip)
    }

    fn graph_tab(shown: bool, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        tab_shell(GRAPH_TAB_ID, shown, pal, cx)
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.notes.graph.showing = true;
                cx.notify();
            }))
            .child(icon(IconName::Flow, ICON_SM, pal.text_muted))
            .child(GRAPH_LABEL)
            .child(
                Self::note_close_button(GRAPH_CLOSE_ID.into(), pal, cx).on_click(cx.listener(
                    |this: &mut Self, _, _, cx| {
                        cx.stop_propagation();
                        this.notes.graph.open = false;
                        this.notes.graph.showing = false;
                        cx.notify();
                    },
                )),
            )
            .into_any_element()
    }

    fn note_tab(
        &self,
        index: usize,
        tab: &NoteTab,
        shown: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let dirty = self
            .notes
            .editors
            .get(&tab.path)
            .is_some_and(NoteEditor::is_dirty);
        let closed = tab.path.clone();
        tab_shell((TAB_PREFIX, index), shown, pal, cx)
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.notes.graph.showing = false;
                this.show_note_tab(index, cx);
            }))
            .on_double_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_note_tabs(cx, |tabs| tabs.pin(index));
            }))
            .child(icon(IconName::Note, ICON_SM, pal.text_muted))
            .child(
                div()
                    .max_w(px(TAB_TITLE_MAX_W))
                    .truncate()
                    .when(!tab.pinned, Styled::italic)
                    .child(tab_title(&tab.path).to_owned()),
            )
            .when(dirty, |tab| {
                tab.child(div().text_color(rgb(pal.text_muted)).child(DIRTY_MARK))
            })
            .child(
                Self::note_close_button((CLOSE_PREFIX, index).into(), pal, cx).on_click(
                    cx.listener(move |this: &mut Self, _, _, cx| {
                        cx.stop_propagation();
                        this.save_note(&closed, cx);
                        this.edit_note_tabs(cx, |tabs| tabs.close(index));
                    }),
                ),
            )
            .into_any_element()
    }

    fn note_close_button(
        id: gpui_kit::ElementId,
        pal: &crate::theme::Palette,
        _: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(CLOSE_BUTTON))
            .rounded(px(RADIUS_XS))
            .hover(|style| style.bg(pal.bg_hover))
            .tooltip(|_, cx| tooltip_view(CLOSE_TOOLTIP.into(), cx))
            .child(icon(IconName::Close, CLOSE_ICON, pal.text_muted))
    }

    fn show_note_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(path) = self
            .note_tabs()
            .and_then(|tabs| tabs.active_tab())
            .map(|tab| tab.path.clone())
        {
            self.save_note(&path, cx);
        }
        self.edit_note_tabs(cx, |tabs| tabs.show(index));
        let path = self
            .note_tabs()
            .and_then(|tabs| tabs.active_tab())
            .map(|tab| tab.path.clone());
        if let Some(editor) = path.and_then(|path| self.notes.editors.get_mut(&path)) {
            editor.focus_pending = true;
        }
    }

    pub(super) fn note_controls(
        &self,
        path: &str,
        mode: NoteMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let index = self.note_tabs().and_then(|tabs| tabs.active);
        let modes = MODES.map(|choice| {
            segment(
                (MODE_PREFIX, choice as u64),
                mode_label(choice),
                None,
                choice == mode,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                if let Some(index) = index {
                    this.edit_note_tabs(cx, |tabs| tabs.set_mode(index, choice));
                }
            }))
        });
        let continued = path.to_owned();
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_1))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .rounded(px(RADIUS_XS))
                    .bg(pal.bg_hover)
                    .children(modes),
            )
            .child(
                Button::new(BTN_CONTINUE)
                    .icon(IconName::Magic)
                    .small()
                    .tooltip(CONTINUE_TOOLTIP)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.continue_note_with_agent(&continued, cx);
                    })),
            )
            .child(
                Button::new(BTN_INSPECTOR)
                    .icon(IconName::Note)
                    .small()
                    .tooltip(INSPECTOR_TOOLTIP)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.toggle_note_inspector(cx);
                    })),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_are_titled_by_the_note_name() {
        assert_eq!(tab_title("Ideas/Spark.md"), "Spark");
        assert_eq!(tab_title("Plan.md"), "Plan");
    }
}
