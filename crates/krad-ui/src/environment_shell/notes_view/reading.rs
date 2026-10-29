use gpui_kit::base::text::TextViewState;
use gpui_kit::base::TextView;
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement, Styled,
};
use krad_domain::{
    note_link_target, reading_markdown, FileOpening, Note, ReadingDoc, FOOTNOTE_BACK_SCHEME,
    FOOTNOTE_SCHEME, OPEN_SCHEME,
};
use std::ops::Range;
use std::path::Path;

use super::super::markdown_images::with_local_images;
use super::super::EnvironmentShell;
use super::markdown::reading_extensions;

const READING_PAD_X: f32 = 32.0;
const READING_PAD_Y: f32 = 16.0;
const READING_MAX_W: f32 = 720.0;
const WEB_SCHEMES: [&str; 3] = ["http://", "https://", "mailto:"];
const SPACE_ESCAPE: &str = "%20";
const HEADING_MARK: char = '#';

pub(in crate::environment_shell) struct ReadingView {
    state: Entity<TextViewState>,
    source: String,
    doc: ReadingDoc,
}

impl EnvironmentShell {
    pub(super) fn note_reading(&mut self, path: &str, cx: &mut Context<Self>) -> AnyElement {
        let Some(text) = self
            .notes
            .editors
            .get(path)
            .and_then(|editor| editor.text(cx))
        else {
            return div().flex_1().into_any_element();
        };
        let state = self.reading_state(path, &text, cx);
        let Some(folder) = self
            .vault_file(path)
            .and_then(|file| file.parent().map(Path::to_path_buf))
        else {
            return div().flex_1().into_any_element();
        };
        let weak = cx.entity().downgrade();
        let from = path.to_owned();
        let view = TextView::new(&state)
            .scrollable(true)
            .markdown_extensions(reading_extensions())
            .on_link_click(move |url, _, _, cx| {
                let url = url.to_string();
                let from = from.clone();
                weak.update(cx, |shell, cx| shell.follow_reading_link(&from, &url, cx))
                    .unwrap_or_default();
            });
        let markdown = self
            .notes
            .reading
            .get(path)
            .map_or_default(|reading| reading.doc.markdown.clone());
        let view = with_local_images(view, &folder, &markdown);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .items_center()
            .px(px(READING_PAD_X))
            .py(px(READING_PAD_Y))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .max_w(px(READING_MAX_W))
                    .child(view),
            )
            .into_any_element()
    }

    fn reading_state(
        &mut self,
        path: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Entity<TextViewState> {
        let changed = self
            .notes
            .reading
            .get(path)
            .is_none_or(|reading| reading.source != text);
        if changed {
            let body = Note::parse(text).body;
            let doc = reading_markdown(&self.notes.index, path, &body);
            if let Some(reading) = self.notes.reading.get_mut(path) {
                let markdown = doc.markdown.clone();
                reading
                    .state
                    .update(cx, |state, cx| state.set_text(&markdown, cx));
                text.clone_into(&mut reading.source);
                reading.doc = doc;
            } else {
                let markdown = doc.markdown.clone();
                let state = cx.new(|cx| TextViewState::markdown(&markdown, cx));
                self.notes.reading.insert(
                    path.to_owned(),
                    ReadingView {
                        state,
                        source: text.to_owned(),
                        doc,
                    },
                );
            }
        }
        self.notes.reading.get(path).map_or_else(
            || cx.new(|cx| TextViewState::markdown("", cx)),
            |reading| reading.state.clone(),
        )
    }

    fn reveal_in_reading(&mut self, path: &str, range: Range<usize>, cx: &mut Context<Self>) {
        let Some(reading) = self.notes.reading.get(path) else {
            return;
        };
        reading.state.update(cx, |state, cx| {
            if state.reveal_range(range, cx).is_err() {
                cx.notify();
            }
        });
    }

    pub(super) fn reveal_heading(&mut self, path: &str, nth: usize, cx: &mut Context<Self>) {
        let range = self
            .notes
            .reading
            .get(path)
            .and_then(|reading| reading.doc.headings.get(nth).cloned());
        if let Some(range) = range {
            self.reveal_in_reading(path, range, cx);
        }
    }

    fn follow_reading_link(&mut self, from: &str, url: &str, cx: &mut Context<Self>) {
        if let Some((target, heading)) = note_link_target(url) {
            self.open_linked_note(&target, heading.as_deref(), cx);
            return;
        }
        let doc = self
            .notes
            .reading
            .get(from)
            .map(|reading| reading.doc.clone());
        if let (Some(id), Some(doc)) = (url.strip_prefix(FOOTNOTE_SCHEME), doc.as_ref()) {
            if let Some(range) = doc.footnote_definition(&id.replace(SPACE_ESCAPE, " ")) {
                self.reveal_in_reading(from, range, cx);
            }
            return;
        }
        if let (Some(id), Some(doc)) = (url.strip_prefix(FOOTNOTE_BACK_SCHEME), doc.as_ref()) {
            if let Some(range) = doc.footnote_mark(&id.replace(SPACE_ESCAPE, " ")) {
                self.reveal_in_reading(from, range, cx);
            }
            return;
        }
        if let Some(path) = url.strip_prefix(OPEN_SCHEME) {
            if let Some(file) = self.vault_file(&path.replace(SPACE_ESCAPE, " ")) {
                cx.open_with_system(&file);
            }
            return;
        }
        if WEB_SCHEMES.iter().any(|scheme| url.starts_with(scheme)) {
            cx.open_url(url);
            return;
        }
        let folder = from.rsplit_once('/').map_or("", |(folder, _)| folder);
        let relative = url.replace(SPACE_ESCAPE, " ");
        let joined = if folder.is_empty() {
            relative
        } else {
            format!("{folder}/{relative}")
        };
        if let Some(file) = self.vault_file(&joined) {
            cx.open_with_system(&file);
        }
    }

    fn open_linked_note(&mut self, target: &str, heading: Option<&str>, cx: &mut Context<Self>) {
        let exists = self.notes.index.note(target).is_some();
        if !exists {
            let title = target.trim_end_matches(".md");
            if title.is_empty() {
                return;
            }
            self.run_note_action(super::NoteAction::NewNamed(title.to_owned()), cx);
            return;
        }
        self.open_vault_note(target, FileOpening::Preview, cx);
        let Some(heading) = heading else {
            return;
        };
        let nth = self.notes.index.note(target).and_then(|note| {
            krad_domain::outline(&note.body).iter().position(|known| {
                known
                    .text
                    .eq_ignore_ascii_case(heading.trim_start_matches(HEADING_MARK))
            })
        });
        if let Some(nth) = nth {
            self.notes.pending_heading = Some((target.to_owned(), nth));
        }
    }

    pub(super) fn reveal_pending_heading(&mut self, cx: &mut Context<Self>) {
        let Some((path, nth)) = self.notes.pending_heading.clone() else {
            return;
        };
        if self.notes.reading.contains_key(&path) {
            self.notes.pending_heading = None;
            self.reveal_heading(&path, nth, cx);
        }
    }
}
