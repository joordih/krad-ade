use super::Effort;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffortSnap {
    Kept,
    Moved { from: Effort, to: Effort },
    Dropped(Effort),
}

#[must_use]
pub fn snap_effort(current: Option<Effort>, allowed: &[Effort]) -> EffortSnap {
    let Some(current) = current else {
        return EffortSnap::Kept;
    };
    if allowed.contains(&current) {
        return EffortSnap::Kept;
    }
    let distance = |level: &Effort| level.rank().abs_diff(current.rank());
    allowed
        .iter()
        .copied()
        .min_by_key(|level| (distance(level), *level))
        .map_or(EffortSnap::Dropped(current), |to| EffortSnap::Moved {
            from: current,
            to,
        })
}

impl Effort {
    #[must_use]
    pub fn rank(self) -> usize {
        Self::LEVELS
            .iter()
            .position(|level| *level == self)
            .unwrap_or_default()
    }

    #[must_use]
    pub fn ordered(levels: impl IntoIterator<Item = Self>) -> Vec<Self> {
        let mut ordered: Vec<Self> = levels.into_iter().collect();
        ordered.sort_unstable();
        ordered.dedup();
        ordered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HAIKU_LIKE: [Effort; 3] = [Effort::Low, Effort::Medium, Effort::High];

    #[test]
    fn levels_are_put_in_rising_order_once() {
        assert_eq!(
            Effort::ordered([Effort::Max, Effort::Low, Effort::Minimal, Effort::Low]),
            [Effort::Minimal, Effort::Low, Effort::Max]
        );
    }

    #[test]
    fn an_allowed_level_stays_and_none_chosen_stays_unset() {
        assert_eq!(
            snap_effort(Some(Effort::Medium), &HAIKU_LIKE),
            EffortSnap::Kept
        );
        assert_eq!(snap_effort(None, &HAIKU_LIKE), EffortSnap::Kept);
    }

    #[test]
    fn a_level_the_new_model_lacks_moves_to_the_nearest_one() {
        assert_eq!(
            snap_effort(Some(Effort::Max), &HAIKU_LIKE),
            EffortSnap::Moved {
                from: Effort::Max,
                to: Effort::High
            }
        );
        assert_eq!(
            snap_effort(Some(Effort::Minimal), &HAIKU_LIKE),
            EffortSnap::Moved {
                from: Effort::Minimal,
                to: Effort::Low
            }
        );
        assert_eq!(
            snap_effort(Some(Effort::Medium), &[Effort::Low, Effort::High]),
            EffortSnap::Moved {
                from: Effort::Medium,
                to: Effort::Low
            }
        );
    }

    #[test]
    fn a_model_without_levels_drops_the_chosen_one() {
        assert_eq!(
            snap_effort(Some(Effort::High), &[]),
            EffortSnap::Dropped(Effort::High)
        );
    }
}
