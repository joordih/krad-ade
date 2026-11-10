use super::fold::{HistoryMessage, HistorySpeaker};
use crate::notes::{Note, Properties, KEY_TAGS};
use std::borrow::Cow;

pub const CHAT_NOTES_FOLDER: &str = "Chats";
const KEY_CHAT: &str = "chat";
const KEY_HARNESS: &str = "harness";
const CHAT_TAGS: &str = "[chat]";
const USER_HEADING: &str = "You";
const FALLBACK_STEM: &str = "Chat";
const QUOTE: char = '"';
const QUOTE_STAND_IN: char = '\u{201d}';
const STEM_STAND_IN: char = '-';
const SECTION_HEADING: &str = "\n## ";
const HEADING_BREAK: &str = "\n\n";
const SECTION_END: &str = "\n";
const TITLE_HEADING: &str = "# ";
const HIDDEN_PREFIX: char = '.';
const UNSAFE_IN_FILE_NAMES: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
const MAX_STEM_BYTES: usize = 120;
const WINDOWS_TRAILING: [char; 2] = ['.', ' '];
const WINDOWS_RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
const WINDOWS_NUMBERED: [&str; 2] = ["COM", "LPT"];
const WINDOWS_NUMBERS: std::ops::RangeInclusive<char> = '0'..='9';
const RESERVED_SUFFIX: &str = "-chat";
const EXTENSION_SEPARATOR: char = '.';
const FENCE_CHARS: [char; 2] = ['`', '~'];
const MIN_FENCE: usize = 3;
const MAX_FENCE_INDENT: usize = 3;
const INDENT: char = ' ';
const LINE_BREAK: char = '\n';

fn quoted(value: &str) -> String {
    let inner: String = value
        .chars()
        .map(|character| {
            if character == QUOTE {
                QUOTE_STAND_IN
            } else {
                character
            }
        })
        .filter(|character| !character.is_control())
        .collect();
    format!("{QUOTE}{inner}{QUOTE}")
}

fn heading(speaker: HistorySpeaker, harness: &str) -> &str {
    match speaker {
        HistorySpeaker::User => USER_HEADING,
        HistorySpeaker::Agent => harness,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fence {
    mark: char,
    len: usize,
}

fn fence_of(line: &str) -> Option<(Fence, &str)> {
    let indent = line.len() - line.trim_start_matches(INDENT).len();
    if indent > MAX_FENCE_INDENT {
        return None;
    }
    let rest = &line[indent..];
    let mark = rest
        .chars()
        .next()
        .filter(|mark| FENCE_CHARS.contains(mark))?;
    let len = rest.len() - rest.trim_start_matches(mark).len();
    (len >= MIN_FENCE).then(|| (Fence { mark, len }, &rest[len..]))
}

fn closed_fences(text: &str) -> Cow<'_, str> {
    let mut open: Option<Fence> = None;
    for line in text.lines() {
        let Some((fence, info)) = fence_of(line) else {
            continue;
        };
        open = match open {
            None => Some(fence),
            Some(opened)
                if fence.mark == opened.mark
                    && fence.len >= opened.len
                    && info.trim().is_empty() =>
            {
                None
            }
            still => still,
        };
    }
    let Some(opened) = open else {
        return Cow::Borrowed(text);
    };
    let closing: String = std::iter::repeat_n(opened.mark, opened.len).collect();
    Cow::Owned(format!("{text}{LINE_BREAK}{closing}"))
}

pub(super) fn transcript_sections(harness: &str, messages: &[HistoryMessage]) -> String {
    messages
        .iter()
        .filter(|message| !message.text.trim().is_empty())
        .fold(String::new(), |mut sections, message| {
            let text = closed_fences(message.text.trim());
            for part in [
                SECTION_HEADING,
                heading(message.speaker, harness),
                HEADING_BREAK,
                &text,
                SECTION_END,
            ] {
                sections.push_str(part);
            }
            sections
        })
}

#[must_use]
pub fn chat_note_markdown(title: &str, harness: &str, messages: &[HistoryMessage]) -> String {
    let mut properties = Properties::default();
    properties.set(KEY_CHAT, quoted(title));
    properties.set(KEY_HARNESS, quoted(harness));
    properties.set(KEY_TAGS, CHAT_TAGS);
    let body = format!(
        "{TITLE_HEADING}{}{SECTION_END}{}",
        title.trim(),
        transcript_sections(harness, messages)
    );
    Note { properties, body }.to_markdown()
}

#[must_use]
pub fn chat_note_stem(title: &str) -> String {
    let stem: String = title
        .chars()
        .filter(|character| !character.is_control())
        .map(|character| {
            if UNSAFE_IN_FILE_NAMES.contains(&character) {
                STEM_STAND_IN
            } else {
                character
            }
        })
        .collect();
    let stem = shortened(stem.trim().trim_start_matches(HIDDEN_PREFIX).trim());
    let stem = stem.trim_end_matches(WINDOWS_TRAILING);
    if stem.is_empty() {
        return FALLBACK_STEM.to_owned();
    }
    if is_reserved_on_windows(stem) {
        return format!("{stem}{RESERVED_SUFFIX}");
    }
    stem.to_owned()
}

fn shortened(stem: &str) -> &str {
    if stem.len() <= MAX_STEM_BYTES {
        return stem;
    }
    let mut end = MAX_STEM_BYTES;
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    &stem[..end]
}

fn is_reserved_on_windows(stem: &str) -> bool {
    let base = stem
        .split(EXTENSION_SEPARATOR)
        .next()
        .unwrap_or(stem)
        .trim_end();
    let upper = base.to_ascii_uppercase();
    if WINDOWS_RESERVED.contains(&upper.as_str()) {
        return true;
    }
    WINDOWS_NUMBERED.iter().any(|prefix| {
        upper.strip_prefix(prefix).is_some_and(|number| {
            let mut digits = number.chars();
            digits
                .next()
                .is_some_and(|digit| WINDOWS_NUMBERS.contains(&digit))
                && digits.next().is_none()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(speaker: HistorySpeaker, text: &str) -> HistoryMessage {
        HistoryMessage {
            speaker,
            text: text.to_owned(),
        }
    }

    #[test]
    fn a_chat_becomes_a_note_with_its_origin_and_one_section_per_message() {
        let messages = [
            message(HistorySpeaker::User, "Why is the \"tail\" slow?\n"),
            message(
                HistorySpeaker::Agent,
                "  It rereads the file.\n\n- fix: keep the offset ",
            ),
            message(HistorySpeaker::Agent, " \n "),
        ];
        assert_eq!(
            chat_note_markdown("Chat · 12", "Claude Code", &messages),
            "---\n\
             chat: \"Chat · 12\"\n\
             harness: \"Claude Code\"\n\
             tags: [chat]\n\
             ---\n\
             # Chat · 12\n\
             \n\
             ## You\n\
             \n\
             Why is the \"tail\" slow?\n\
             \n\
             ## Claude Code\n\
             \n\
             It rereads the file.\n\
             \n\
             - fix: keep the offset\n"
        );
    }

    #[test]
    fn the_exported_note_reads_back_with_its_properties() {
        let text = chat_note_markdown("Say \"hi\"", "Codex", &[]);
        let note = Note::parse(&text);
        assert_eq!(
            note.properties.get(KEY_CHAT),
            Some("Say \u{201d}hi\u{201d}")
        );
        assert_eq!(note.properties.get(KEY_HARNESS), Some("Codex"));
        assert_eq!(note.properties.list(KEY_TAGS), ["chat"]);
        assert_eq!(note.title("fallback"), "Say \"hi\"");
    }

    #[test]
    fn a_note_name_drops_what_file_systems_refuse() {
        assert_eq!(chat_note_stem("Chat · 12"), "Chat · 12");
        assert_eq!(chat_note_stem("fix a/b: why?"), "fix a-b- why-");
        assert_eq!(chat_note_stem("  ..hidden "), "hidden");
        assert_eq!(chat_note_stem(" \n "), "Chat");
    }

    #[test]
    fn a_note_name_is_bounded_and_never_a_windows_device() {
        let long = "é".repeat(MAX_STEM_BYTES);
        let stem = chat_note_stem(&long);
        assert!(stem.len() <= MAX_STEM_BYTES);
        assert!(stem.chars().all(|character| character == 'é'));
        assert_eq!(chat_note_stem("con"), "con-chat");
        assert_eq!(chat_note_stem("Aux.notes"), "Aux.notes-chat");
        assert_eq!(chat_note_stem("COM7"), "COM7-chat");
        assert_eq!(chat_note_stem("COM10"), "COM10");
        assert_eq!(chat_note_stem("console"), "console");
        assert_eq!(chat_note_stem("done. . "), "done");
    }

    #[test]
    fn an_unclosed_code_fence_is_closed_inside_its_own_section() {
        let messages = [
            message(
                HistorySpeaker::Agent,
                "Run:\n````sh\ncargo test\n```\nstill code",
            ),
            message(HistorySpeaker::User, "~~~\nfine\n~~~"),
        ];
        let sections = transcript_sections("Codex", &messages);
        assert_eq!(
            sections,
            "\n## Codex\n\nRun:\n````sh\ncargo test\n```\nstill code\n````\n\
             \n## You\n\n~~~\nfine\n~~~\n"
        );
    }
}
