use std::time::Duration;

const FIRST_DELAY: Duration = Duration::from_secs(1);
const LONGEST_DELAY: Duration = Duration::from_mins(1);
const GROWTH_FACTOR: u32 = 2;
pub(super) const RECONNECT_ATTEMPTS: u32 = 8;

#[must_use]
pub(super) fn reconnect_delay(attempt: u32) -> Option<Duration> {
    if attempt == 0 || attempt > RECONNECT_ATTEMPTS {
        return None;
    }
    let grown = (1..attempt).fold(FIRST_DELAY, |delay, _| {
        delay.saturating_mul(GROWTH_FACTOR).min(LONGEST_DELAY)
    });
    Some(grown.min(LONGEST_DELAY))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_double_from_one_second_up_to_a_minute() {
        let delays: Vec<u64> = (1..=RECONNECT_ATTEMPTS)
            .filter_map(reconnect_delay)
            .map(|delay| delay.as_secs())
            .collect();
        assert_eq!(delays, [1, 2, 4, 8, 16, 32, 60, 60]);
    }

    #[test]
    fn attempts_outside_the_schedule_have_no_delay() {
        assert_eq!(reconnect_delay(0), None);
        assert_eq!(reconnect_delay(RECONNECT_ATTEMPTS + 1), None);
    }
}
