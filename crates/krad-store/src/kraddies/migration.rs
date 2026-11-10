use super::{KraddieShelf, KraddieStoreError};
use krad_domain::{AutomationId, AutomationKey, ReadKraddie};

impl KraddieShelf {
    pub(super) fn adopt(&self, read: &ReadKraddie) -> Result<(), KraddieStoreError> {
        let id = read.kraddie.id();
        self.write_automations(&id, &read.automations)?;
        if !read.migrated {
            return Ok(());
        }
        let legacy = self.legacy_history_path(&id);
        let first = AutomationKey::new(id, AutomationId::FIRST);
        let owns_first = read
            .automations
            .iter()
            .any(|automation| automation.id == first.automation);
        if owns_first && legacy.exists() && !self.history_path(&first).exists() {
            std::fs::rename(&legacy, self.history_path(&first))?;
        }
        self.write(&read.kraddie)
    }
}
