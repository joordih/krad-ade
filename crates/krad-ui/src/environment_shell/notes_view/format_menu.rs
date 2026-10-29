use gpui_kit::base::input::{Copy, Cut, Paste, SelectAll};
use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::{Action, Context, Window};
use krad_domain::{format_edit, NoteFormat};

use super::super::EnvironmentShell;

const CUT_LABEL: &str = "Cut";
const COPY_LABEL: &str = "Copy";
const PASTE_LABEL: &str = "Paste";
const SELECT_ALL_LABEL: &str = "Select All";
const FORMAT_LABEL: &str = "Format";
const PARAGRAPH_LABEL: &str = "Paragraph";
const INSERT_LABEL: &str = "Insert";

const FORMATS: [(&str, NoteFormat); 7] = [
    ("Bold", NoteFormat::Bold),
    ("Italic", NoteFormat::Italic),
    ("Strikethrough", NoteFormat::Strikethrough),
    ("Highlight", NoteFormat::Highlight),
    ("Code", NoteFormat::InlineCode),
    ("Math", NoteFormat::InlineMath),
    ("Link", NoteFormat::Link),
];

const PARAGRAPHS: [(&str, NoteFormat); 7] = [
    ("Heading 1", NoteFormat::Heading1),
    ("Heading 2", NoteFormat::Heading2),
    ("Heading 3", NoteFormat::Heading3),
    ("Bulleted list", NoteFormat::BulletList),
    ("Numbered list", NoteFormat::NumberedList),
    ("Task list", NoteFormat::TaskList),
    ("Quote", NoteFormat::Quote),
];

const INSERTS: [(&str, NoteFormat); 5] = [
    ("Code block", NoteFormat::CodeBlock),
    ("Math block", NoteFormat::MathBlock),
    ("Table", NoteFormat::Table),
    ("Callout", NoteFormat::Callout),
    ("Divider", NoteFormat::Divider),
];

#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = krad, no_json)]
pub(in crate::environment_shell) struct FormatNote(pub(in crate::environment_shell) NoteFormat);

fn submenu(items: &[(&str, NoteFormat)]) -> NativeMenu {
    items
        .iter()
        .fold(NativeMenu::new(), |menu, (label, format)| {
            menu.menu(*label, Box::new(FormatNote(*format)))
        })
}

pub(super) fn note_editor_menu(menu: NativeMenu) -> NativeMenu {
    menu.menu(CUT_LABEL, Box::new(Cut))
        .menu(COPY_LABEL, Box::new(Copy))
        .menu(PASTE_LABEL, Box::new(Paste))
        .menu(SELECT_ALL_LABEL, Box::new(SelectAll))
        .separator()
        .submenu(FORMAT_LABEL, submenu(&FORMATS))
        .submenu(PARAGRAPH_LABEL, submenu(&PARAGRAPHS))
        .submenu(INSERT_LABEL, submenu(&INSERTS))
}

impl EnvironmentShell {
    pub(super) fn format_note(
        &mut self,
        path: &str,
        format: NoteFormat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self
            .notes
            .editors
            .get(path)
            .and_then(|editor| editor.state.clone())
        else {
            return;
        };
        state.update(cx, |state, cx| {
            let text = state.value().to_string();
            let edit = format_edit(&text, state.selected_range(), format);
            state.set_selected_range(edit.range.clone(), cx);
            state.replace(edit.text, window, cx);
            let start = edit.range.start;
            state.set_selected_range(start + edit.selection.start..start + edit.selection.end, cx);
            state.focus(window, cx);
        });
    }
}
