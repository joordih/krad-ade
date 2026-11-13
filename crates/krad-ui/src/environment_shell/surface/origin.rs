use gpui_kit::{div, px, rgb, AnyElement, IntoElement, ParentElement, Styled};
use krad_domain::{ChatOpener, ChatOrigin};
use std::time::{Duration, SystemTime};

use crate::components::{icon, IconName};
use crate::theme::{Palette, ICON_XS, LINE_XS, SPACE_1_5, TEXT_XS};

const MINUTE: Duration = Duration::from_mins(1);
const HOUR: Duration = Duration::from_hours(1);
const DAY: Duration = Duration::from_hours(24);
const MONTH: Duration = Duration::from_hours(24 * 30);
const YEAR: Duration = Duration::from_hours(24 * 365);
const JUST_NOW: &str = "just now";
const MINUTES_UNIT: &str = "min";
const HOURS_UNIT: &str = "h";
const DAYS_UNIT: &str = "d";
const MONTHS_UNIT: &str = "mo";
const YEARS_UNIT: &str = "y";
const AGO: &str = "ago";
const FROM_LEAD: &str = "From Lead";
const FROM_USER: &str = "From you";
const ORIGIN_SEPARATOR: &str = " · ";

fn origin_label(origin: ChatOrigin, now: SystemTime) -> String {
    let from = match origin.opener {
        ChatOpener::Lead => FROM_LEAD,
        ChatOpener::User => FROM_USER,
    };
    let age = origin
        .started
        .and_then(|started| now.duration_since(started).ok())
        .map(age_label);
    match age {
        Some(age) => format!("{from}{ORIGIN_SEPARATOR}{age}"),
        None => from.to_owned(),
    }
}

pub(in crate::environment_shell) fn age_label(elapsed: Duration) -> String {
    if elapsed < MINUTE {
        return JUST_NOW.to_owned();
    }
    let (unit, name) = if elapsed < HOUR {
        (MINUTE, MINUTES_UNIT)
    } else if elapsed < DAY {
        (HOUR, HOURS_UNIT)
    } else if elapsed < MONTH {
        (DAY, DAYS_UNIT)
    } else if elapsed < YEAR {
        (MONTH, MONTHS_UNIT)
    } else {
        (YEAR, YEARS_UNIT)
    };
    format!("{} {name} {AGO}", elapsed.as_secs() / unit.as_secs())
}

pub(super) fn origin_line(origin: ChatOrigin, pal: &Palette) -> AnyElement {
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
        .child(origin_label(origin, SystemTime::now()))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages_read_in_the_largest_whole_unit() {
        assert_eq!(age_label(Duration::from_secs(20)), "just now");
        assert_eq!(age_label(6 * MINUTE + Duration::from_secs(30)), "6 min ago");
        assert_eq!(age_label(3 * HOUR), "3 h ago");
        assert_eq!(age_label(2 * DAY), "2 d ago");
        assert_eq!(age_label(70 * DAY), "2 mo ago");
        assert_eq!(age_label(800 * DAY), "2 y ago");
    }

    #[test]
    fn the_origin_line_says_who_opened_the_conversation_and_when() {
        let now = SystemTime::now();
        let lead = ChatOrigin {
            opener: ChatOpener::Lead,
            started: now.checked_sub(Duration::from_mins(6)),
        };
        assert_eq!(origin_label(lead, now), "From Lead · 6 min ago");
        let user = ChatOrigin {
            opener: ChatOpener::User,
            started: None,
        };
        assert_eq!(origin_label(user, now), "From you");
    }
}
