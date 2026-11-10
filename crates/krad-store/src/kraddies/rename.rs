use super::{read_kraddie, remove_if_present, KraddieShelf, KraddieStoreError};
use krad_domain::{Automation, AutomationKey, Kraddie, KraddieHistory, KraddieId};
use std::io::ErrorKind;
use std::path::PathBuf;

pub struct KraddieRename<'a> {
    pub old: &'a KraddieId,
    pub kraddie: &'a Kraddie,
    pub automations: &'a [Automation],
    pub histories: &'a [(AutomationKey, KraddieHistory)],
}

impl KraddieShelf {
    /// # Errors
    /// Fails when the renamed Kraddie cannot be written in full, and then leaves the old one as
    /// it was; once the new one is in place, fails only when an old file cannot be removed.
    pub fn rename(&self, rename: &KraddieRename<'_>) -> Result<(), KraddieStoreError> {
        let new = rename.kraddie.id();
        if self.kraddie_path(&new).exists() {
            return Err(std::io::Error::from(ErrorKind::AlreadyExists).into());
        }
        let mut written = Vec::new();
        if let Err(error) = self.stage(rename, &new, &mut written) {
            for path in &written {
                drop(remove_if_present(path));
            }
            return Err(error);
        }
        self.delete_files(rename.old)
    }

    fn stage(
        &self,
        rename: &KraddieRename<'_>,
        new: &KraddieId,
        written: &mut Vec<PathBuf>,
    ) -> Result<(), KraddieStoreError> {
        self.write(rename.kraddie)?;
        written.push(self.kraddie_path(new));
        self.write_automations(new, rename.automations)?;
        written.push(self.automations_path(new));
        for (key, history) in rename.histories {
            self.write_history(key, history)?;
            written.push(self.history_path(key));
        }
        read_kraddie(&self.kraddie_path(new))?;
        self.rename_profile(rename.old, new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AgentPreset, AutomationId, KraddieOutcome, KraddieWake, Trigger};
    use std::time::SystemTime;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "krad-store-kraddie-rename-{name}-{}",
            std::process::id()
        ))
    }

    struct Saved {
        shelf: KraddieShelf,
        kraddie: Kraddie,
        renamed: Kraddie,
        automations: Vec<Automation>,
        histories: Vec<(AutomationKey, KraddieHistory)>,
    }

    fn saved(dir: &std::path::Path) -> Result<Saved, Box<dyn std::error::Error>> {
        let shelf = KraddieShelf::at(dir.to_path_buf());
        let kraddie = Kraddie::new("Night Owl".parse()?, AgentPreset::ClaudeCode);
        let mut renamed = kraddie.clone();
        renamed.name = "Early Bird".parse()?;
        let mut automation = Automation::new(AutomationId(1), "Bugs".parse()?);
        automation.triggers = vec![Trigger::Manual];
        let mut history = KraddieHistory::default();
        history.start(
            SystemTime::now(),
            KraddieWake::Manual,
            KraddieOutcome::Finished,
        );
        shelf.write(&kraddie)?;
        shelf.write_automations(&kraddie.id(), std::slice::from_ref(&automation))?;
        let old_key = AutomationKey::new(kraddie.id(), AutomationId(1));
        shelf.write_history(&old_key, &history)?;
        std::fs::create_dir_all(dir.join("night-owl"))?;
        std::fs::write(dir.join("night-owl").join("memory.json"), b"[]")?;
        let new_key = AutomationKey::new(renamed.id(), AutomationId(1));
        Ok(Saved {
            shelf,
            kraddie,
            renamed,
            automations: vec![automation],
            histories: vec![(new_key, history)],
        })
    }

    fn order<'a>(saved: &'a Saved, old: &'a KraddieId) -> KraddieRename<'a> {
        KraddieRename {
            old,
            kraddie: &saved.renamed,
            automations: &saved.automations,
            histories: &saved.histories,
        }
    }

    #[test]
    fn a_rename_moves_the_kraddie_its_automations_runs_and_profile() -> TestResult {
        let dir = scratch("moved");
        drop(std::fs::remove_dir_all(&dir));
        let saved = saved(&dir)?;
        let old = saved.kraddie.id();
        saved.shelf.rename(&order(&saved, &old))?;
        let read = saved.shelf.read_all();
        let [shelved] = read.as_slice() else {
            unreachable!("one Kraddie");
        };
        assert_eq!(shelved.kraddie, saved.renamed);
        assert_eq!(shelved.automations.len(), 1);
        assert_eq!(shelved.automations[0].history.len(), 1);
        assert!(dir.join("early-bird").join("memory.json").exists());
        assert!(!dir.join("night-owl").exists());
        assert!(!dir.join("night-owl.automations.json").exists());
        assert!(!dir.join("night-owl.1.runs.json").exists());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn a_failed_rename_leaves_the_old_kraddie_whole() -> TestResult {
        let dir = scratch("failed");
        drop(std::fs::remove_dir_all(&dir));
        let saved = saved(&dir)?;
        std::fs::create_dir_all(dir.join("early-bird"))?;
        std::fs::write(dir.join("early-bird").join("in-the-way"), b"in the way")?;
        let old = saved.kraddie.id();
        let failed = saved.shelf.rename(&order(&saved, &old));
        assert!(failed.is_err());
        let read = saved.shelf.read_all();
        let [shelved] = read.as_slice() else {
            unreachable!("only the old Kraddie");
        };
        assert_eq!(shelved.kraddie, saved.kraddie);
        assert_eq!(shelved.automations.len(), 1);
        assert_eq!(shelved.automations[0].history.len(), 1);
        assert!(dir.join("night-owl").join("memory.json").exists());
        assert!(!dir.join("early-bird.kraddie").exists());
        assert!(!dir.join("early-bird.automations.json").exists());
        assert!(!dir.join("early-bird.1.runs.json").exists());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
