use libghostty_vt::key::{self, Key, Mods};
use libghostty_vt::{Error, Terminal};

const ESC: u8 = 0x1b;
const DELETE_WORD_LEFT: [u8; 1] = [0x17];
const DELETE_WORD_RIGHT: [u8; 2] = [ESC, b'd'];
const SHIFT_ENTER: [u8; 2] = [ESC, b'\r'];
const ENTER_KEY: &str = "enter";
const BACKSPACE_KEY: &str = "backspace";
const DELETE_KEY: &str = "delete";
const NO_TEXT: char = '\0';

const NAMED_KEYS: [(&str, Key); 28] = [
    (ENTER_KEY, Key::Enter),
    (BACKSPACE_KEY, Key::Backspace),
    ("tab", Key::Tab),
    ("escape", Key::Escape),
    ("space", Key::Space),
    ("up", Key::ArrowUp),
    ("down", Key::ArrowDown),
    ("left", Key::ArrowLeft),
    ("right", Key::ArrowRight),
    ("home", Key::Home),
    ("end", Key::End),
    ("pageup", Key::PageUp),
    ("pagedown", Key::PageDown),
    ("insert", Key::Insert),
    (DELETE_KEY, Key::Delete),
    ("f1", Key::F1),
    ("f2", Key::F2),
    ("f3", Key::F3),
    ("f4", Key::F4),
    ("f5", Key::F5),
    ("f6", Key::F6),
    ("f7", Key::F7),
    ("f8", Key::F8),
    ("f9", Key::F9),
    ("f10", Key::F10),
    ("f11", Key::F11),
    ("f12", Key::F12),
    ("menu", Key::ContextMenu),
];

const CHARACTER_KEYS: [(char, Key); 47] = [
    ('a', Key::A),
    ('b', Key::B),
    ('c', Key::C),
    ('d', Key::D),
    ('e', Key::E),
    ('f', Key::F),
    ('g', Key::G),
    ('h', Key::H),
    ('i', Key::I),
    ('j', Key::J),
    ('k', Key::K),
    ('l', Key::L),
    ('m', Key::M),
    ('n', Key::N),
    ('o', Key::O),
    ('p', Key::P),
    ('q', Key::Q),
    ('r', Key::R),
    ('s', Key::S),
    ('t', Key::T),
    ('u', Key::U),
    ('v', Key::V),
    ('w', Key::W),
    ('x', Key::X),
    ('y', Key::Y),
    ('z', Key::Z),
    ('0', Key::Digit0),
    ('1', Key::Digit1),
    ('2', Key::Digit2),
    ('3', Key::Digit3),
    ('4', Key::Digit4),
    ('5', Key::Digit5),
    ('6', Key::Digit6),
    ('7', Key::Digit7),
    ('8', Key::Digit8),
    ('9', Key::Digit9),
    ('`', Key::Backquote),
    ('-', Key::Minus),
    ('=', Key::Equal),
    ('[', Key::BracketLeft),
    (']', Key::BracketRight),
    ('\\', Key::Backslash),
    (';', Key::Semicolon),
    ('\'', Key::Quote),
    (',', Key::Comma),
    ('.', Key::Period),
    ('/', Key::Slash),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl From<KeyModifiers> for Mods {
    fn from(held: KeyModifiers) -> Self {
        let mut mods = Self::empty();
        if held.ctrl {
            mods |= Self::CTRL;
        }
        if held.alt {
            mods |= Self::ALT;
        }
        if held.shift {
            mods |= Self::SHIFT;
        }
        mods
    }
}

fn ghostty_key(key: &str) -> Option<(Key, char)> {
    if let Some((_, named)) = NAMED_KEYS.iter().find(|(name, _)| *name == key) {
        return Some((*named, NO_TEXT));
    }
    let mut chars = key.chars();
    let typed = chars.next()?.to_ascii_lowercase();
    if chars.next().is_some() {
        return None;
    }
    CHARACTER_KEYS
        .iter()
        .find(|(character, _)| *character == typed)
        .map(|(character, found)| (*found, *character))
}

fn legacy_override(key: &str, held: KeyModifiers) -> Option<Vec<u8>> {
    match key {
        ENTER_KEY if held.shift && !held.ctrl && !held.alt => Some(SHIFT_ENTER.to_vec()),
        BACKSPACE_KEY if held.ctrl && !held.alt => Some(DELETE_WORD_LEFT.to_vec()),
        DELETE_KEY if held.ctrl => Some(DELETE_WORD_RIGHT.to_vec()),
        _ => None,
    }
}

fn alt_gr_text(key: &str, key_char: Option<&str>, held: KeyModifiers) -> Option<Vec<u8>> {
    let text = key_char.filter(|text| !text.is_empty() && *text != key)?;
    (held.ctrl && held.alt).then(|| text.as_bytes().to_vec())
}

pub(crate) struct KeyInput {
    encoder: key::Encoder<'static>,
    event: key::Event<'static>,
}

impl KeyInput {
    pub(crate) fn new() -> Result<Self, Error> {
        Ok(Self {
            encoder: key::Encoder::new()?,
            event: key::Event::new()?,
        })
    }

    pub(crate) fn encode(
        &mut self,
        term: &Terminal<'_, '_>,
        key: &str,
        key_char: Option<&str>,
        held: KeyModifiers,
    ) -> Option<Vec<u8>> {
        if let Some(text) = alt_gr_text(key, key_char, held) {
            return Some(text);
        }
        let legacy = term
            .kitty_keyboard_flags()
            .map_or(true, |flags| flags.is_empty());
        if legacy {
            if let Some(bytes) = legacy_override(key, held) {
                return Some(bytes);
            }
        }
        let text = key_char.filter(|text| !text.is_empty());
        let Some((named, unshifted)) = ghostty_key(key) else {
            return text.map(|text| text.as_bytes().to_vec());
        };
        let mods = Mods::from(held);
        let consumed = if unshifted != NO_TEXT && text.is_some() {
            mods & Mods::SHIFT
        } else {
            Mods::empty()
        };
        self.event
            .set_action(key::Action::Press)
            .set_key(named)
            .set_mods(mods)
            .set_consumed_mods(consumed)
            .set_unshifted_codepoint(unshifted)
            .set_utf8(text);
        let mut bytes = Vec::new();
        self.encoder
            .set_options_from_terminal(term)
            .set_macos_option_as_alt(key::OptionAsAlt::True)
            .encode_to_vec(&self.event, &mut bytes)
            .ok()?;
        if bytes.is_empty() {
            return text.map(|text| text.as_bytes().to_vec());
        }
        Some(bytes)
    }
}
