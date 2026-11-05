use gpui_kit::{div, px, rgb, AnyElement, IntoElement, ParentElement, SharedString, Styled};
use krad_chat::Speaker;
use krad_domain::TerminalId;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

use super::super::chat::{AnswerPart, ChatMessage};
use super::super::EnvironmentShell;
use super::SurfaceChat;
use crate::components::thinking_indicator;
use crate::theme::{Palette, LINE_XS, SPACE_2, TEXT_XS};

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;
const PHRASE_INTERVAL: Duration = Duration::from_secs(4);
const PHRASE_STRIDE: u64 = 7;

const THINKING_PHRASES: [&str; 20] = [
    "Thinking…",
    "Pondering…",
    "Mulling it over…",
    "Musing…",
    "Reflecting…",
    "Considering…",
    "Weighing options…",
    "Daydreaming…",
    "Puzzling…",
    "Contemplating…",
    "Ruminating…",
    "Noodling…",
    "Brainstorming…",
    "Deliberating…",
    "Connecting dots…",
    "Chewing on it…",
    "Imagining…",
    "Wondering…",
    "Scheming…",
    "Reasoning…",
];

const WORKING_PHRASES: [&str; 30] = [
    "Working…",
    "Brewing…",
    "Tinkering…",
    "Untangling…",
    "Cooking…",
    "Sketching…",
    "Wrangling…",
    "Crafting…",
    "Assembling…",
    "Whittling…",
    "Polishing…",
    "Forging…",
    "Stitching…",
    "Kneading…",
    "Simmering…",
    "Weaving…",
    "Hammering away…",
    "Shuffling bits…",
    "Juggling…",
    "Sorting things out…",
    "Building…",
    "Fiddling…",
    "Rigging…",
    "Sculpting…",
    "Patching…",
    "Tidying up…",
    "Plumbing…",
    "Carving…",
    "Mixing…",
    "Hustling…",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TurnPhase {
    Thinking,
    Working,
}

impl TurnPhase {
    pub(super) fn of(last: Option<&ChatMessage>) -> Self {
        let wrote = last
            .filter(|message| message.speaker == Speaker::Assistant)
            .and_then(|message| message.parts.last());
        match wrote {
            Some(AnswerPart::Tool(_) | AnswerPart::Text(_) | AnswerPart::Ask(_)) => Self::Working,
            Some(AnswerPart::Thinking(_)) | None => Self::Thinking,
        }
    }

    const fn phrases(self) -> &'static [&'static str] {
        match self {
            Self::Thinking => &THINKING_PHRASES,
            Self::Working => &WORKING_PHRASES,
        }
    }
}

pub(in crate::environment_shell) fn elapsed_label(elapsed: Duration) -> String {
    let total = elapsed.as_secs();
    let hours = total / SECONDS_PER_HOUR;
    let minutes = total % SECONDS_PER_HOUR / SECONDS_PER_MINUTE;
    let seconds = total % SECONDS_PER_MINUTE;
    if total < SECONDS_PER_MINUTE {
        format!("{seconds}s")
    } else if total < SECONDS_PER_HOUR {
        format!("{minutes}m {seconds:02}s")
    } else {
        format!("{hours}h {minutes:02}m {seconds:02}s")
    }
}

fn phrase(kind: TurnPhase, seed: u64, elapsed: Duration) -> &'static str {
    let phrases = kind.phrases();
    let Some(count) = u64::try_from(phrases.len()).ok().filter(|count| *count > 0) else {
        return "";
    };
    let step = elapsed.as_secs() / PHRASE_INTERVAL.as_secs();
    let index = step.wrapping_mul(PHRASE_STRIDE).wrapping_add(seed % count) % count;
    usize::try_from(index)
        .ok()
        .and_then(|index| phrases.get(index))
        .copied()
        .unwrap_or_default()
}

fn turn_seed(started: Instant) -> u64 {
    let mut hasher = DefaultHasher::new();
    started.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn status_line(
    key: u64,
    kind: TurnPhase,
    started: Instant,
    activity: Option<&str>,
    mono: SharedString,
    pal: &Palette,
) -> AnyElement {
    let elapsed = started.elapsed();
    let label = phrase(kind, turn_seed(started), elapsed);
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_2))
        .min_w_0()
        .child(thinking_indicator(
            key,
            label.into(),
            elapsed_label(elapsed).into(),
            mono,
            pal,
        ))
        .children(activity.map(|activity| {
            div()
                .min_w_0()
                .truncate()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_muted))
                .child(activity.to_owned())
        }))
        .into_any_element()
}

impl EnvironmentShell {
    pub(super) fn turn_started(&self, id: TerminalId, chat: &SurfaceChat) -> Instant {
        let tracked = self
            .envs
            .get(&self.active)
            .and_then(|live| live.working_since.get(&id))
            .copied();
        let started = tracked
            .or_else(|| chat.working_since.get())
            .unwrap_or_else(Instant::now);
        chat.working_since.set(Some(started));
        started
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_time_reads_in_seconds_then_minutes_then_hours() {
        assert_eq!(elapsed_label(Duration::from_secs(0)), "0s");
        assert_eq!(elapsed_label(Duration::from_millis(42_900)), "42s");
        assert_eq!(elapsed_label(Duration::from_secs(59)), "59s");
        assert_eq!(elapsed_label(Duration::from_secs(60)), "1m 00s");
        assert_eq!(elapsed_label(Duration::from_secs(187)), "3m 07s");
        assert_eq!(elapsed_label(Duration::from_secs(3599)), "59m 59s");
        assert_eq!(elapsed_label(Duration::from_secs(3600)), "1h 00m 00s");
        assert_eq!(elapsed_label(Duration::from_secs(3727)), "1h 02m 07s");
    }

    #[test]
    fn the_phrase_holds_within_an_interval_and_moves_on_after_it() {
        let seed = 12_345;
        let first = phrase(TurnPhase::Working, seed, Duration::ZERO);
        let within = PHRASE_INTERVAL.saturating_sub(Duration::from_millis(1));
        assert_eq!(phrase(TurnPhase::Working, seed, within), first);
        assert_eq!(phrase(TurnPhase::Working, seed, Duration::ZERO), first);
        assert_ne!(phrase(TurnPhase::Working, seed, PHRASE_INTERVAL), first);
    }

    #[test]
    fn every_phrase_comes_round_before_any_repeats() {
        for kind in [TurnPhase::Thinking, TurnPhase::Working] {
            let count = kind.phrases().len();
            let mut shown: Vec<&str> = (0u32..)
                .take(count)
                .map(|step| phrase(kind, 3, PHRASE_INTERVAL * step))
                .collect();
            shown.sort_unstable();
            shown.dedup();
            assert_eq!(shown.len(), count);
        }
    }

    #[test]
    fn thinking_and_working_draw_from_their_own_phrases() {
        for elapsed in (0u32..)
            .take(WORKING_PHRASES.len())
            .map(|step| PHRASE_INTERVAL * step)
        {
            assert!(THINKING_PHRASES.contains(&phrase(TurnPhase::Thinking, 9, elapsed)));
            assert!(WORKING_PHRASES.contains(&phrase(TurnPhase::Working, 9, elapsed)));
        }
        assert!(THINKING_PHRASES
            .iter()
            .chain(&WORKING_PHRASES)
            .all(|phrase| phrase.ends_with('…')));
    }

    #[test]
    fn the_same_turn_keeps_the_same_seed() {
        let started = Instant::now();
        assert_eq!(turn_seed(started), turn_seed(started));
    }

    #[test]
    fn a_turn_thinks_until_the_answer_starts_writing() {
        assert_eq!(TurnPhase::of(None), TurnPhase::Thinking);
    }
}
