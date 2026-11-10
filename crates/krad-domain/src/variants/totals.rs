use super::run::{VariantId, VariantRun};
use crate::usage::Spend;
use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunTotals {
    pub spent: Spend,
    pub wall: Option<Duration>,
    pub cheapest: Option<VariantId>,
    pub fastest: Option<VariantId>,
}

impl VariantRun {
    #[must_use]
    pub fn totals(&self, now: SystemTime) -> RunTotals {
        let spent = self.variants.iter().map(|variant| variant.spent).sum();
        let start = self
            .variants
            .iter()
            .filter_map(|variant| variant.started_at)
            .min();
        let end = self
            .variants
            .iter()
            .filter(|variant| variant.started_at.is_some())
            .map(|variant| variant.finished_at.unwrap_or(now))
            .max();
        let wall = start
            .zip(end)
            .map(|(start, end)| end.duration_since(start).unwrap_or_default());
        let contenders = || {
            self.variants
                .iter()
                .filter(|variant| variant.status.can_win() || variant.status.is_closed())
                .filter(|variant| variant.finished_at.is_some())
        };
        let cheapest = contenders()
            .filter(|variant| !variant.spent.cost.is_zero())
            .min_by_key(|variant| variant.spent.cost)
            .map(|variant| variant.id);
        let fastest = contenders()
            .filter_map(|variant| Some((variant.elapsed(now)?, variant.id)))
            .min()
            .map(|(_, id)| id);
        RunTotals {
            spent,
            wall,
            cheapest,
            fastest,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{TokenUsage, UsdCost};
    use crate::variants::fixtures::{finish, launch, run_of};
    use crate::variants::VariantRunId;
    use std::time::UNIX_EPOCH;

    const MINUTE: Duration = Duration::from_mins(1);

    fn spend(dollars: f64, output: u64) -> Spend {
        Spend {
            tokens: TokenUsage {
                output,
                ..TokenUsage::default()
            },
            cost: UsdCost::try_from(dollars).unwrap_or_default(),
        }
    }

    #[test]
    fn totals_add_the_spend_and_span_from_the_first_start_to_the_last_end() {
        let mut run = run_of(VariantRunId(1), 3);
        launch(&mut run, VariantId(1), UNIX_EPOCH);
        launch(&mut run, VariantId(2), UNIX_EPOCH + MINUTE);
        finish(&mut run, VariantId(1), UNIX_EPOCH + 5 * MINUTE);
        finish(&mut run, VariantId(2), UNIX_EPOCH + 3 * MINUTE);
        for (id, cost, tokens) in [(1, 0.40, 400), (2, 0.90, 100)] {
            if let Some(variant) = run.variant_mut(VariantId(id)) {
                variant.spent = spend(cost, tokens);
            }
        }
        let totals = run.totals(UNIX_EPOCH + 10 * MINUTE);
        assert_eq!(totals.spent, spend(1.30, 500));
        assert_eq!(totals.wall, Some(5 * MINUTE));
        assert_eq!(totals.cheapest, Some(VariantId(1)));
        assert_eq!(totals.fastest, Some(VariantId(2)));
    }

    #[test]
    fn a_variant_still_working_stretches_the_wall_time_but_never_wins_a_badge() {
        let mut run = run_of(VariantRunId(1), 2);
        launch(&mut run, VariantId(1), UNIX_EPOCH);
        launch(&mut run, VariantId(2), UNIX_EPOCH);
        finish(&mut run, VariantId(1), UNIX_EPOCH + MINUTE);
        let totals = run.totals(UNIX_EPOCH + 7 * MINUTE);
        assert_eq!(totals.wall, Some(7 * MINUTE));
        assert_eq!(totals.fastest, Some(VariantId(1)));
        assert_eq!(totals.cheapest, None);
        assert_eq!(run_of(VariantRunId(2), 2).totals(UNIX_EPOCH).wall, None);
    }
}
