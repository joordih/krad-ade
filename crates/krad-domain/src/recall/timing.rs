use crate::usage::BudgetStatus;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

pub const DIGEST_AFTER_QUIET: Duration = Duration::from_mins(10);
pub const MIN_NEW_MESSAGES: usize = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatDigests {
    #[default]
    Automatic,
    Off,
}

impl ChatDigests {
    #[must_use]
    pub const fn is_automatic(&self) -> bool {
        matches!(self, Self::Automatic)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatPhase {
    Working,
    Quiet { since: Instant },
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DigestCheck {
    pub phase: ChatPhase,
    pub messages: usize,
    pub covered: usize,
    pub digests: ChatDigests,
    pub budget: Option<BudgetStatus>,
    pub writing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigestReason {
    ChatEnded,
    WentQuiet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigestSkip {
    TurnedOff,
    NearBudget,
    NothingNew,
    StillWorking,
    AlreadyWriting,
    NotQuietYet(Duration),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigestDecision {
    Write(DigestReason),
    Skip(DigestSkip),
}

#[must_use]
pub fn digest_decision(check: &DigestCheck, now: Instant) -> DigestDecision {
    let skip = DigestDecision::Skip;
    if !check.digests.is_automatic() {
        return skip(DigestSkip::TurnedOff);
    }
    if check
        .budget
        .is_some_and(|status| status >= BudgetStatus::Approaching)
    {
        return skip(DigestSkip::NearBudget);
    }
    if check.writing {
        return skip(DigestSkip::AlreadyWriting);
    }
    if check.messages < check.covered + MIN_NEW_MESSAGES {
        return skip(DigestSkip::NothingNew);
    }
    match check.phase {
        ChatPhase::Ended => DigestDecision::Write(DigestReason::ChatEnded),
        ChatPhase::Working => skip(DigestSkip::StillWorking),
        ChatPhase::Quiet { since } => {
            let quiet = now.saturating_duration_since(since);
            match DIGEST_AFTER_QUIET.checked_sub(quiet) {
                Some(left) if !left.is_zero() => skip(DigestSkip::NotQuietYet(left)),
                _ => DigestDecision::Write(DigestReason::WentQuiet),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(phase: ChatPhase) -> DigestCheck {
        DigestCheck {
            phase,
            messages: 4,
            covered: 0,
            digests: ChatDigests::Automatic,
            budget: None,
            writing: false,
        }
    }

    #[test]
    fn an_ended_chat_with_new_messages_is_digested_at_once() {
        let now = Instant::now();
        assert_eq!(
            digest_decision(&check(ChatPhase::Ended), now),
            DigestDecision::Write(DigestReason::ChatEnded)
        );
    }

    #[test]
    fn a_quiet_chat_waits_for_the_pause_then_is_digested() {
        let start = Instant::now();
        let quiet = check(ChatPhase::Quiet { since: start });
        let early = start + Duration::from_mins(4);
        assert_eq!(
            digest_decision(&quiet, early),
            DigestDecision::Skip(DigestSkip::NotQuietYet(Duration::from_mins(6)))
        );
        assert_eq!(
            digest_decision(&quiet, start + DIGEST_AFTER_QUIET),
            DigestDecision::Write(DigestReason::WentQuiet)
        );
        assert_eq!(
            digest_decision(&check(ChatPhase::Working), start + DIGEST_AFTER_QUIET),
            DigestDecision::Skip(DigestSkip::StillWorking)
        );
    }

    #[test]
    fn the_toggle_the_budget_and_old_content_hold_it_back() {
        let now = Instant::now();
        let off = DigestCheck {
            digests: ChatDigests::Off,
            ..check(ChatPhase::Ended)
        };
        assert_eq!(
            digest_decision(&off, now),
            DigestDecision::Skip(DigestSkip::TurnedOff)
        );
        for status in [BudgetStatus::Approaching, BudgetStatus::Exceeded] {
            let near = DigestCheck {
                budget: Some(status),
                ..check(ChatPhase::Ended)
            };
            assert_eq!(
                digest_decision(&near, now),
                DigestDecision::Skip(DigestSkip::NearBudget)
            );
        }
        let within = DigestCheck {
            budget: Some(BudgetStatus::Within),
            ..check(ChatPhase::Ended)
        };
        assert!(matches!(
            digest_decision(&within, now),
            DigestDecision::Write(_)
        ));
        let seen = DigestCheck {
            covered: 3,
            ..check(ChatPhase::Ended)
        };
        assert_eq!(
            digest_decision(&seen, now),
            DigestDecision::Skip(DigestSkip::NothingNew)
        );
        let busy = DigestCheck {
            writing: true,
            ..check(ChatPhase::Ended)
        };
        assert_eq!(
            digest_decision(&busy, now),
            DigestDecision::Skip(DigestSkip::AlreadyWriting)
        );
    }
}
