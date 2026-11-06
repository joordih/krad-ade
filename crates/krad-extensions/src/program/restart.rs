use std::time::Duration;

pub const RESTART_DELAYS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(4),
    Duration::from_secs(15),
];
pub const STABLE_RUN: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RestartPolicy {
    crashes: usize,
}

impl RestartPolicy {
    pub fn after_crash(&mut self, ran_for: Duration) -> Option<Duration> {
        if ran_for >= STABLE_RUN {
            self.crashes = 0;
        }
        let delay = RESTART_DELAYS.get(self.crashes).copied();
        self.crashes += 1;
        delay
    }

    pub fn reset(&mut self) {
        self.crashes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUICK_CRASH: Duration = Duration::from_secs(2);

    #[test]
    fn quick_crashes_wait_longer_each_time_then_give_up() {
        let mut policy = RestartPolicy::default();
        let delays: Vec<_> = (0..4).map(|_| policy.after_crash(QUICK_CRASH)).collect();
        assert_eq!(
            delays,
            [
                Some(RESTART_DELAYS[0]),
                Some(RESTART_DELAYS[1]),
                Some(RESTART_DELAYS[2]),
                None
            ]
        );
    }

    #[test]
    fn a_crash_after_a_stable_run_starts_counting_again() {
        let mut policy = RestartPolicy::default();
        policy.after_crash(QUICK_CRASH);
        policy.after_crash(QUICK_CRASH);
        assert_eq!(policy.after_crash(STABLE_RUN), Some(RESTART_DELAYS[0]));
    }

    #[test]
    fn a_manual_retry_forgets_earlier_crashes() {
        let mut policy = RestartPolicy::default();
        for _ in RESTART_DELAYS {
            policy.after_crash(QUICK_CRASH);
        }
        policy.reset();
        assert_eq!(policy.after_crash(QUICK_CRASH), Some(RESTART_DELAYS[0]));
    }
}
