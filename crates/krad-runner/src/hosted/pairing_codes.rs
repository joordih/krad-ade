use krad_domain::{PairingCode, PairingPermissions, Timestamp};
use krad_host::new_pairing_code;
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

const CODE_LIFETIME: Duration = Duration::from_mins(10);
const FAILED_TRIES_WINDOW: Duration = Duration::from_mins(1);
const MAX_FAILED_TRIES: usize = 10;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Redeemed {
    Granted(PairingPermissions),
    Unknown,
    TooManyTries,
}

struct Offered {
    permissions: PairingPermissions,
    until: Instant,
}

#[derive(Default)]
struct Ledger {
    offered: HashMap<PairingCode, Offered>,
    failed: VecDeque<Instant>,
}

impl Ledger {
    fn forget_old_failures(&mut self, now: Instant) {
        while self
            .failed
            .front()
            .is_some_and(|tried| now.duration_since(*tried) > FAILED_TRIES_WINDOW)
        {
            self.failed.pop_front();
        }
    }
}

#[derive(Default)]
pub(crate) struct PairingCodes {
    ledger: Mutex<Ledger>,
}

impl PairingCodes {
    fn ledger(&self) -> MutexGuard<'_, Ledger> {
        self.ledger.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn offer(
        &self,
        permissions: PairingPermissions,
    ) -> Option<(PairingCode, Timestamp)> {
        let code = new_pairing_code()?;
        let now = Instant::now();
        let mut ledger = self.ledger();
        ledger.offered.retain(|_, offer| offer.until > now);
        ledger.offered.insert(
            code.clone(),
            Offered {
                permissions,
                until: now + CODE_LIFETIME,
            },
        );
        Some((code, Timestamp::from(SystemTime::now() + CODE_LIFETIME)))
    }

    pub(crate) fn redeem(&self, code: &PairingCode) -> Redeemed {
        let now = Instant::now();
        let mut ledger = self.ledger();
        ledger.forget_old_failures(now);
        if ledger.failed.len() >= MAX_FAILED_TRIES {
            return Redeemed::TooManyTries;
        }
        match ledger.offered.remove(code) {
            Some(offer) if offer.until > now => Redeemed::Granted(offer.permissions),
            _ => {
                ledger.failed.push_back(now);
                Redeemed::Unknown
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::PairingPermission;

    #[test]
    fn a_pairing_code_is_redeemed_once_with_the_permissions_it_was_made_with() {
        let codes = PairingCodes::default();
        let read: PairingPermissions = [PairingPermission::Read].into_iter().collect();
        let offered = codes.offer(read.clone());
        assert!(offered.is_some());
        let Some((code, _)) = offered else { return };
        assert_eq!(codes.redeem(&code), Redeemed::Granted(read));
        assert_eq!(codes.redeem(&code), Redeemed::Unknown);
    }

    #[test]
    fn too_many_wrong_codes_lock_out_even_a_right_one() -> Result<(), Box<dyn std::error::Error>> {
        let codes = PairingCodes::default();
        let (code, _) = codes
            .offer(PairingPermissions::all())
            .ok_or("no pairing code was made")?;
        let wrong: PairingCode = "AAAAAAAA".parse()?;
        for _ in 0..MAX_FAILED_TRIES {
            assert_eq!(codes.redeem(&wrong), Redeemed::Unknown);
        }
        assert_eq!(codes.redeem(&code), Redeemed::TooManyTries);
        Ok(())
    }
}
