use std::time::Duration;

const FIRST_WAIT: Duration = Duration::from_secs(1);
const LONGEST_WAIT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) struct Backoff {
    failures: u32,
}

impl Backoff {
    pub(in crate::environment_shell) fn next_wait(&mut self) -> Duration {
        let wait = FIRST_WAIT
            .checked_mul(1_u32.checked_shl(self.failures).unwrap_or(u32::MAX))
            .map_or(LONGEST_WAIT, |wait| wait.min(LONGEST_WAIT));
        self.failures = self.failures.saturating_add(1);
        wait
    }

    pub(in crate::environment_shell) fn reset(&mut self) {
        self.failures = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_failed_reach_waits_twice_as_long_up_to_the_longest_wait() {
        let mut backoff = Backoff::default();
        let waits: Vec<Duration> = (0..7).map(|_| backoff.next_wait()).collect();
        assert_eq!(
            waits,
            [1, 2, 4, 8, 16, 30, 30].map(Duration::from_secs).to_vec()
        );
        for _ in 0..40 {
            backoff.next_wait();
        }
        assert_eq!(backoff.next_wait(), LONGEST_WAIT);
        backoff.reset();
        assert_eq!(backoff.next_wait(), FIRST_WAIT);
    }
}
