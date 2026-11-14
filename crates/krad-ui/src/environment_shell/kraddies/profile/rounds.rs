use krad_domain::quiet_since;
use std::time::SystemTime;

use super::super::super::EnvironmentShell;
use super::super::KraddieEntry;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn next_check_in(
        &self,
        entry: &KraddieEntry,
    ) -> Option<SystemTime> {
        if !entry.kraddie.switch.is_on() {
            return None;
        }
        let id = entry.kraddie.id();
        let since = quiet_since(
            entry.last_started(),
            self.runtime.kraddies.opened_at,
            self.runtime.kraddies.minds.last_check_in(&id),
        );
        entry.kraddie.settings.rounds.next_after(since)
    }
}
