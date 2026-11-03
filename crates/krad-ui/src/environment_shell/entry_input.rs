use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    div, AnyElement, AppContext as _, Context, Entity, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Styled, Subscription, Window,
};
use krad_repo::EntryKind;
use std::path::{Path, PathBuf};

use super::rename::KEY_ESCAPE;
use super::EnvironmentShell;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum EntryEdit {
    Create { dir: PathBuf, kind: EntryKind },
    Rename(PathBuf),
}

pub(super) struct EntryInput {
    edit: EntryEdit,
    field: Entity<InputState>,
    _events: Subscription,
}

pub(super) fn entry_name(text: &str) -> Option<&str> {
    let name = text.trim();
    (!name.is_empty()).then_some(name)
}

impl EnvironmentShell {
    pub(super) fn begin_entry_input(
        &mut self,
        edit: EntryEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let initial = match &edit {
            EntryEdit::Rename(path) => path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
            EntryEdit::Create { .. } => String::new(),
        };
        let field = cx.new(|cx| InputState::new(window, cx).default_value(initial));
        let events = cx.subscribe_in(
            &field,
            window,
            |this: &mut Self, _, event: &InputEvent, _, cx| match event {
                InputEvent::PressEnter { .. } => this.commit_entry_input(cx),
                InputEvent::Blur => this.cancel_entry_input(cx),
                InputEvent::Change | InputEvent::Focus => {}
            },
        );
        field.update(cx, |field, cx| field.focus(window, cx));
        self.entry_input = Some(EntryInput {
            edit,
            field,
            _events: events,
        });
        cx.notify();
    }

    pub(super) fn cancel_entry_input(&mut self, cx: &mut Context<Self>) {
        if self.entry_input.take().is_some() {
            cx.notify();
        }
    }

    fn commit_entry_input(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.entry_input.take() else {
            return;
        };
        cx.notify();
        let text = input.field.read(cx).value().to_string();
        let Some(name) = entry_name(&text) else {
            return;
        };
        match input.edit {
            EntryEdit::Create { dir, kind } => self.create_file_entry(dir, kind, name, cx),
            EntryEdit::Rename(path) => self.rename_file(path, name, cx),
        }
    }

    pub(super) fn new_entry_draft(&self) -> Option<(&Path, EntryKind)> {
        match &self.entry_input.as_ref()?.edit {
            EntryEdit::Create { dir, kind } => Some((dir.as_path(), *kind)),
            EntryEdit::Rename(_) => None,
        }
    }

    pub(super) fn is_renaming(&self, path: &Path) -> bool {
        self.entry_input.as_ref().is_some_and(
            |input| matches!(&input.edit, EntryEdit::Rename(renamed) if renamed == path),
        )
    }

    pub(super) fn entry_input_field(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let input = self.entry_input.as_ref()?;
        Some(
            div()
                .flex_1()
                .min_w_0()
                .capture_key_down(cx.listener(|this: &mut Self, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.key == KEY_ESCAPE {
                        cx.stop_propagation();
                        this.cancel_entry_input(cx);
                    }
                }))
                .child(Input::new(&input.field).appearance(false).cleanable(false))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_name_creates_nothing_and_spaces_around_a_name_are_dropped() {
        assert_eq!(entry_name("  Main.java "), Some("Main.java"));
        assert_eq!(entry_name("   "), None);
    }
}
