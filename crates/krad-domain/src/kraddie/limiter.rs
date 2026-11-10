use super::automation::AutomationKey;
use super::name::KraddieId;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_KRADDIE_RUNS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunRefusal {
    AlreadyRunning,
    AtCapacity,
}

impl fmt::Display for RunRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRunning => f.write_str("The automation is already running"),
            Self::AtCapacity => write!(f, "{MAX_KRADDIE_RUNS} automations are already running"),
        }
    }
}

impl std::error::Error for RunRefusal {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KraddieRunLimiter {
    running: Vec<AutomationKey>,
}

impl KraddieRunLimiter {
    /// # Errors
    /// Refuses an automation already running, and any automation once [`MAX_KRADDIE_RUNS`] run.
    pub fn begin(&mut self, run: &AutomationKey) -> Result<(), RunRefusal> {
        if self.is_running(run) {
            return Err(RunRefusal::AlreadyRunning);
        }
        if self.running.len() >= MAX_KRADDIE_RUNS {
            return Err(RunRefusal::AtCapacity);
        }
        self.running.push(run.clone());
        Ok(())
    }

    pub fn end(&mut self, run: &AutomationKey) {
        self.running.retain(|running| running != run);
    }

    pub fn end_kraddie(&mut self, kraddie: &KraddieId) {
        self.running.retain(|running| running.kraddie != *kraddie);
    }

    #[must_use]
    pub fn is_running(&self, run: &AutomationKey) -> bool {
        self.running.contains(run)
    }

    #[must_use]
    pub fn has_kraddie_running(&self, kraddie: &KraddieId) -> bool {
        self.running
            .iter()
            .any(|running| running.kraddie == *kraddie)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.running.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.running.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::automation::AutomationId;

    fn id(text: &str) -> AutomationKey {
        let kraddie = KraddieId::try_from(text.to_owned()).unwrap_or_else(|_| unreachable!());
        AutomationKey::new(kraddie, AutomationId::FIRST)
    }

    #[test]
    fn one_run_per_automation_and_a_bounded_number_overall() {
        let mut limiter = KraddieRunLimiter::default();
        assert_eq!(limiter.begin(&id("a")), Ok(()));
        assert_eq!(limiter.begin(&id("a")), Err(RunRefusal::AlreadyRunning));
        for name in ["b", "c"].into_iter().take(MAX_KRADDIE_RUNS - 1) {
            assert_eq!(limiter.begin(&id(name)), Ok(()));
        }
        assert_eq!(limiter.len(), MAX_KRADDIE_RUNS);
        assert_eq!(limiter.begin(&id("d")), Err(RunRefusal::AtCapacity));
        limiter.end(&id("a"));
        assert!(!limiter.is_running(&id("a")));
        assert_eq!(limiter.begin(&id("d")), Ok(()));
        limiter.end(&id("missing"));
        assert_eq!(limiter.len(), MAX_KRADDIE_RUNS);
        let other = AutomationKey::new(id("d").kraddie, AutomationId(2));
        assert_eq!(limiter.begin(&other), Err(RunRefusal::AtCapacity));
        assert!(limiter.has_kraddie_running(&id("d").kraddie));
        limiter.end_kraddie(&id("d").kraddie);
        assert!(!limiter.has_kraddie_running(&id("d").kraddie));
        assert_eq!(limiter.begin(&other), Ok(()));
    }
}
