use std::borrow::Cow;

use gpui_kit::{Capslock, Keystroke};

pub(crate) fn typed_text(keystroke: &Keystroke, _: Capslock) -> Option<Cow<'_, str>> {
    keystroke.key_char.as_deref().map(Cow::Borrowed)
}
