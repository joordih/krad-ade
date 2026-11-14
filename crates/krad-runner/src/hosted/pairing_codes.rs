use krad_domain::{PairingCode, PairingPermissions, Timestamp};
use krad_host::new_pairing_code;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

const CODE_LIFETIME: Duration = Duration::from_mins(10);

struct Offered {
    permissions: PairingPermissions,
    until: Instant,
}

#[derive(Default)]
pub(crate) struct PairingCodes {
    offered: Mutex<HashMap<PairingCode, Offered>>,
}

impl PairingCodes {
    fn offered(&self) -> MutexGuard<'_, HashMap<PairingCode, Offered>> {
        self.offered.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn offer(
        &self,
        permissions: PairingPermissions,
    ) -> Option<(PairingCode, Timestamp)> {
        let code = new_pairing_code()?;
        let now = Instant::now();
        let mut offered = self.offered();
        offered.retain(|_, offer| offer.until > now);
        offered.insert(
            code.clone(),
            Offered {
                permissions,
                until: now + CODE_LIFETIME,
            },
        );
        Some((code, Timestamp::from(SystemTime::now() + CODE_LIFETIME)))
    }

    pub(crate) fn redeem(&self, code: &PairingCode) -> Option<PairingPermissions> {
        let offer = self.offered().remove(code)?;
        (offer.until > Instant::now()).then_some(offer.permissions)
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
        assert_eq!(codes.redeem(&code), Some(read));
        assert_eq!(codes.redeem(&code), None);
    }
}
