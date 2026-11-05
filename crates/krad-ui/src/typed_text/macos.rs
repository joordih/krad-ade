use std::borrow::Cow;

use gpui_kit::{Capslock, Keystroke};

pub(crate) fn typed_text(keystroke: &Keystroke, capslock: Capslock) -> Option<Cow<'_, str>> {
    let text = keystroke.key_char.as_deref()?;
    let modifiers = keystroke.modifiers;
    if !capslock.on || modifiers.control || modifiers.platform || !is_lowercase_letter(text) {
        return Some(Cow::Borrowed(text));
    }
    Some(Cow::Owned(text.to_uppercase()))
}

fn is_lowercase_letter(text: &str) -> bool {
    let mut chars = text.chars();
    matches!((chars.next(), chars.next()), (Some(letter), None) if letter.is_lowercase() && letter.to_uppercase().count() == 1)
}

#[cfg(test)]
mod tests {
    use gpui_kit::Modifiers;

    use super::*;

    const ON: Capslock = Capslock { on: true };
    const OFF: Capslock = Capslock { on: false };

    fn stroke(key_char: &str, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key_char.to_lowercase(),
            key_char: Some(key_char.to_owned()),
        }
    }

    fn typed(stroke: &Keystroke, capslock: Capslock) -> Option<String> {
        typed_text(stroke, capslock).map(Cow::into_owned)
    }

    #[test]
    fn caps_lock_types_letters_in_capitals() {
        assert_eq!(
            typed(&stroke("a", Modifiers::none()), ON).as_deref(),
            Some("A")
        );
        assert_eq!(
            typed(&stroke("ñ", Modifiers::none()), ON).as_deref(),
            Some("Ñ")
        );
    }

    #[test]
    fn caps_lock_with_shift_still_types_capitals() {
        assert_eq!(
            typed(&stroke("A", Modifiers::shift()), ON).as_deref(),
            Some("A")
        );
    }

    #[test]
    fn caps_lock_leaves_digits_symbols_and_shortcuts_alone() {
        assert_eq!(
            typed(&stroke("1", Modifiers::none()), ON).as_deref(),
            Some("1")
        );
        assert_eq!(
            typed(&stroke("-", Modifiers::none()), ON).as_deref(),
            Some("-")
        );
        assert_eq!(
            typed(&stroke("c", Modifiers::control()), ON).as_deref(),
            Some("c")
        );
        assert_eq!(
            typed(&stroke("s", Modifiers::command()), ON).as_deref(),
            Some("s")
        );
    }

    #[test]
    fn letters_stay_as_typed_without_caps_lock() {
        assert_eq!(
            typed(&stroke("a", Modifiers::none()), OFF).as_deref(),
            Some("a")
        );
    }

    #[test]
    fn a_key_without_text_types_nothing() {
        let stroke = Keystroke {
            modifiers: Modifiers::none(),
            key: "left".to_owned(),
            key_char: None,
        };
        assert_eq!(typed(&stroke, ON), None);
    }
}
