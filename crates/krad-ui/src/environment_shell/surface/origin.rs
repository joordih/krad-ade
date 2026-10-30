use gpui_kit::{div, px, rgb, AnyElement, IntoElement, ParentElement, Styled};
use krad_domain::{is_worker_brief, ChatEntry};
use std::time::{Duration, SystemTime};

use crate::components::{icon, IconName};
use crate::theme::{Palette, ICON_XS, LINE_XS, SPACE_1_5, TEXT_XS};

const MINUTE: Duration = Duration::from_mins(1);
const HOUR: Duration = Duration::from_hours(1);
const DAY: Duration = Duration::from_hours(24);
const JUST_NOW: &str = "just now";
const MINUTES_UNIT: &str = "min";
const HOURS_UNIT: &str = "h";
const DAYS_UNIT: &str = "d";
const AGO: &str = "ago";
const FROM_LEAD: &str = "From Lead";
const FROM_USER: &str = "From you";
const ORIGIN_SEPARATOR: &str = " · ";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::environment_shell) enum Opener {
    Lead,
    User,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::environment_shell) struct Origin {
    pub(in crate::environment_shell) opener: Opener,
    pub(in crate::environment_shell) started: Option<SystemTime>,
}

impl Origin {
    pub(super) fn of(entries: &[ChatEntry], started: Option<SystemTime>) -> Option<Self> {
        let first = entries.iter().find_map(|entry| match entry {
            ChatEntry::User { text, .. } => Some(text),
            ChatEntry::Event(_) | ChatEntry::Asked(_) => None,
        })?;
        let opener = if is_worker_brief(first) {
            Opener::Lead
        } else {
            Opener::User
        };
        Some(Self { opener, started })
    }
}

impl Origin {
    fn label(self, now: SystemTime) -> String {
        let from = match self.opener {
            Opener::Lead => FROM_LEAD,
            Opener::User => FROM_USER,
        };
        let age = self
            .started
            .and_then(|started| now.duration_since(started).ok())
            .map(age_label);
        match age {
            Some(age) => format!("{from}{ORIGIN_SEPARATOR}{age}"),
            None => from.to_owned(),
        }
    }
}

pub(super) fn age_label(elapsed: Duration) -> String {
    if elapsed < MINUTE {
        return JUST_NOW.to_owned();
    }
    let (unit, name) = if elapsed < HOUR {
        (MINUTE, MINUTES_UNIT)
    } else if elapsed < DAY {
        (HOUR, HOURS_UNIT)
    } else {
        (DAY, DAYS_UNIT)
    };
    format!("{} {name} {AGO}", elapsed.as_secs() / unit.as_secs())
}

pub(super) fn origin_line(origin: Origin, pal: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(SPACE_1_5))
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_muted))
        .child(icon(IconName::Crown, ICON_XS, pal.text_muted))
        .child(origin.label(SystemTime::now()))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AgentPreset, ChatEvent};

    #[test]
    fn ages_read_in_the_largest_whole_unit() {
        assert_eq!(age_label(Duration::from_secs(20)), "just now");
        assert_eq!(age_label(6 * MINUTE + Duration::from_secs(30)), "6 min ago");
        assert_eq!(age_label(3 * HOUR), "3 h ago");
        assert_eq!(age_label(2 * DAY), "2 d ago");
    }

    #[test]
    fn a_conversation_a_worker_brief_opened_comes_from_the_lead() {
        let user = |text: String| ChatEntry::User {
            text,
            preset: AgentPreset::ClaudeCode,
            model: None,
            images: Vec::new(),
        };
        let brief = krad_domain::chat_worker_brief("Verify the signature");
        let origin = Origin::of(&[ChatEntry::Event(ChatEvent::Done), user(brief)], None);
        assert_eq!(origin.map(|origin| origin.opener), Some(Opener::Lead));
        let asked = Origin::of(&[user("Why?".to_owned())], None);
        assert_eq!(asked.map(|origin| origin.opener), Some(Opener::User));
        assert_eq!(Origin::of(&[], None), None);
    }

    #[test]
    fn the_origin_line_says_who_opened_the_conversation_and_when() {
        let now = SystemTime::now();
        let lead = Origin {
            opener: Opener::Lead,
            started: now.checked_sub(Duration::from_mins(6)),
        };
        assert_eq!(lead.label(now), "From Lead · 6 min ago");
        let user = Origin {
            opener: Opener::User,
            started: None,
        };
        assert_eq!(user.label(now), "From you");
    }
}
