use std::time::{Duration, Instant};

pub(super) const HOVER_GRACE: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PointerZone {
    Text,
    Card,
    Elsewhere,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PointerStep {
    Stay,
    Leave,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HoverPointer {
    OnText,
    OnCard,
    Leaving(Instant),
}

impl HoverPointer {
    pub(super) fn moved(&mut self, zone: PointerZone, now: Instant) -> PointerStep {
        match (zone, *self) {
            (PointerZone::Text, _) => {
                *self = Self::OnText;
                PointerStep::Stay
            }
            (PointerZone::Card, _) => {
                *self = Self::OnCard;
                PointerStep::Stay
            }
            (PointerZone::Elsewhere, Self::Leaving(_)) if self.grace_elapsed(now) => {
                PointerStep::Close
            }
            (PointerZone::Elsewhere, Self::Leaving(_)) => PointerStep::Stay,
            (PointerZone::Elsewhere, Self::OnText | Self::OnCard) => {
                *self = Self::Leaving(now);
                PointerStep::Leave
            }
        }
    }

    pub(super) fn grace_elapsed(self, now: Instant) -> bool {
        matches!(self, Self::Leaving(since) if now.saturating_duration_since(since) >= HOVER_GRACE)
    }

    pub(super) const fn takes_new_hover(self, has_content: bool) -> bool {
        match self {
            Self::OnText => true,
            Self::OnCard => false,
            Self::Leaving(_) => has_content,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WITHIN_GRACE: Duration = Duration::from_millis(100);

    #[test]
    fn the_card_survives_the_trip_from_the_text_to_the_card() {
        let start = Instant::now();
        let mut pointer = HoverPointer::OnText;
        assert_eq!(
            pointer.moved(PointerZone::Elsewhere, start),
            PointerStep::Leave
        );
        assert_eq!(
            pointer.moved(PointerZone::Elsewhere, start + WITHIN_GRACE),
            PointerStep::Stay
        );
        assert_eq!(
            pointer.moved(PointerZone::Card, start + WITHIN_GRACE),
            PointerStep::Stay
        );
        assert_eq!(pointer, HoverPointer::OnCard);
        assert!(!pointer.grace_elapsed(start + HOVER_GRACE));
    }

    #[test]
    fn the_card_closes_once_the_pointer_stays_away_past_the_grace() {
        let start = Instant::now();
        let mut pointer = HoverPointer::OnCard;
        assert_eq!(
            pointer.moved(PointerZone::Elsewhere, start),
            PointerStep::Leave
        );
        assert!(!pointer.grace_elapsed(start + WITHIN_GRACE));
        assert!(pointer.grace_elapsed(start + HOVER_GRACE));
        assert_eq!(
            pointer.moved(PointerZone::Elsewhere, start + HOVER_GRACE),
            PointerStep::Close
        );
    }

    #[test]
    fn hovers_answered_on_the_way_never_replace_the_card_with_nothing() {
        let leaving = HoverPointer::Leaving(Instant::now());
        assert!(!leaving.takes_new_hover(false));
        assert!(leaving.takes_new_hover(true));
        assert!(!HoverPointer::OnCard.takes_new_hover(true));
        assert!(HoverPointer::OnText.takes_new_hover(false));
    }
}
