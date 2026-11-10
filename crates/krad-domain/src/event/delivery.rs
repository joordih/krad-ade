use std::ops::RangeInclusive;
use std::time::Duration;

pub const WEBHOOK_MAX_ATTEMPTS: u32 = 5;
pub const WEBHOOK_FIRST_BACKOFF: Duration = Duration::from_secs(2);
pub const WEBHOOK_MAX_BACKOFF: Duration = Duration::from_secs(60);
const BACKOFF_FACTOR: u32 = 2;
const SUCCESS_STATUSES: RangeInclusive<u16> = 200..=299;
const CLIENT_ERROR_STATUSES: RangeInclusive<u16> = 400..=499;
const REQUEST_TIMEOUT_STATUS: u16 = 408;
const TOO_MANY_REQUESTS_STATUS: u16 = 429;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryOutcome {
    Delivered,
    Refused,
    Failed,
}

impl DeliveryOutcome {
    #[must_use]
    pub fn of_status(status: u16) -> Self {
        if SUCCESS_STATUSES.contains(&status) {
            return Self::Delivered;
        }
        let retried = matches!(status, REQUEST_TIMEOUT_STATUS | TOO_MANY_REQUESTS_STATUS);
        if CLIENT_ERROR_STATUSES.contains(&status) && !retried {
            return Self::Refused;
        }
        Self::Failed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryStep {
    Done,
    RetryAfter(Duration),
    GiveUp,
}

#[must_use]
pub fn backoff_after(attempt: u32) -> Duration {
    let doublings = attempt.saturating_sub(1);
    let factor = BACKOFF_FACTOR.checked_pow(doublings).unwrap_or(u32::MAX);
    WEBHOOK_FIRST_BACKOFF
        .checked_mul(factor)
        .map_or(WEBHOOK_MAX_BACKOFF, |wait| wait.min(WEBHOOK_MAX_BACKOFF))
}

#[must_use]
pub fn next_delivery_step(attempt: u32, outcome: DeliveryOutcome) -> DeliveryStep {
    match outcome {
        DeliveryOutcome::Delivered => DeliveryStep::Done,
        DeliveryOutcome::Refused => DeliveryStep::GiveUp,
        DeliveryOutcome::Failed if attempt >= WEBHOOK_MAX_ATTEMPTS => DeliveryStep::GiveUp,
        DeliveryOutcome::Failed => DeliveryStep::RetryAfter(backoff_after(attempt)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_say_whether_to_try_again() {
        assert_eq!(DeliveryOutcome::of_status(204), DeliveryOutcome::Delivered);
        assert_eq!(DeliveryOutcome::of_status(404), DeliveryOutcome::Refused);
        assert_eq!(DeliveryOutcome::of_status(429), DeliveryOutcome::Failed);
        assert_eq!(DeliveryOutcome::of_status(408), DeliveryOutcome::Failed);
        assert_eq!(DeliveryOutcome::of_status(503), DeliveryOutcome::Failed);
        assert_eq!(DeliveryOutcome::of_status(302), DeliveryOutcome::Failed);
    }

    #[test]
    fn the_wait_doubles_up_to_its_ceiling() {
        assert_eq!(backoff_after(1), WEBHOOK_FIRST_BACKOFF);
        assert_eq!(backoff_after(2), WEBHOOK_FIRST_BACKOFF * 2);
        assert_eq!(backoff_after(3), WEBHOOK_FIRST_BACKOFF * 4);
        assert_eq!(backoff_after(40), WEBHOOK_MAX_BACKOFF);
        assert_eq!(backoff_after(u32::MAX), WEBHOOK_MAX_BACKOFF);
    }

    #[test]
    fn failures_retry_a_bounded_number_of_times() {
        let steps: Vec<DeliveryStep> = (1..=WEBHOOK_MAX_ATTEMPTS)
            .map(|attempt| next_delivery_step(attempt, DeliveryOutcome::Failed))
            .collect();
        let retries = steps
            .iter()
            .filter(|step| matches!(step, DeliveryStep::RetryAfter(_)))
            .count();
        assert_eq!(
            retries,
            usize::try_from(WEBHOOK_MAX_ATTEMPTS - 1).unwrap_or(0)
        );
        assert_eq!(steps.last(), Some(&DeliveryStep::GiveUp));
        assert_eq!(
            next_delivery_step(1, DeliveryOutcome::Refused),
            DeliveryStep::GiveUp
        );
        assert_eq!(
            next_delivery_step(3, DeliveryOutcome::Delivered),
            DeliveryStep::Done
        );
    }
}
