use super::*;
use krad_domain::{AgentPreset, AutomationId, KraddieOutcome, KraddieWake, Trigger, WorkingCopy};
use std::time::SystemTime;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const LEGACY_FOLDER: &str = "pets";

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("krad-store-kraddies-{name}-{}", std::process::id()))
}

fn routine(id: u32, name: &str) -> Result<Automation, Box<dyn std::error::Error>> {
    let mut automation = Automation::new(AutomationId(id), name.parse()?);
    automation.triggers = vec![Trigger::Manual];
    Ok(automation)
}

fn finished_once() -> KraddieHistory {
    let mut history = KraddieHistory::default();
    history.start(
        SystemTime::now(),
        KraddieWake::Manual,
        KraddieOutcome::Finished,
    );
    history
}

#[test]
fn kraddies_their_automations_and_each_history_live_in_their_own_files() -> TestResult {
    let dir = scratch("shelf");
    let shelf = KraddieShelf::at(dir.clone());
    let kraddie = Kraddie::new("Night Owl".parse()?, AgentPreset::ClaudeCode);
    let automations = vec![routine(1, "Bugs")?, routine(2, "Docs")?];
    shelf.write(&kraddie)?;
    shelf.write_automations(&kraddie.id(), &automations)?;
    let key = AutomationKey::new(kraddie.id(), AutomationId(2));
    shelf.write_history(&key, &finished_once())?;
    std::fs::write(dir.join("broken.kradpet"), b"{")?;
    let read = shelf.read_all();
    let [shelved] = read.as_slice() else {
        unreachable!("one Kraddie");
    };
    assert_eq!(shelved.kraddie, kraddie);
    assert_eq!(shelved.migration_problem, None);
    let histories: Vec<usize> = shelved
        .automations
        .iter()
        .map(|shelved| shelved.history.len())
        .collect();
    assert_eq!(histories, vec![0, 1]);
    assert!(dir.join("night-owl.kraddie").exists());
    assert!(dir.join("night-owl.automations.json").exists());
    assert!(dir.join("night-owl.2.runs.json").exists());
    shelf.delete(&kraddie.id())?;
    shelf.delete(&kraddie.id())?;
    assert_eq!(shelf.read_all(), Vec::new());
    assert!(!dir.join("night-owl.2.runs.json").exists());
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn an_old_kraddie_file_migrates_once_and_keeps_its_run_history() -> TestResult {
    let dir = scratch("migrate");
    std::fs::create_dir_all(&dir)?;
    let shelf = KraddieShelf::at(dir.clone());
    let kraddie = Kraddie::new("Gardener".parse()?, AgentPreset::Codex);
    let old = KraddieFile::new(kraddie.clone(), Vec::new())
        .write()?
        .replace("\"format\": 2", "\"format\": 1")
        .replacen(
            "\"switch\": \"on\"",
            "\"switch\": \"on\", \"instructions\": \"Weed the backlog.\"",
            1,
        );
    std::fs::write(dir.join("gardener.kradpet"), old)?;
    std::fs::write(
        dir.join("gardener.json"),
        serde_json::to_vec(&finished_once())?,
    )?;
    let first = shelf.read_all();
    let [shelved] = first.as_slice() else {
        unreachable!("one Kraddie");
    };
    let [migrated] = shelved.automations.as_slice() else {
        unreachable!("one automation");
    };
    assert_eq!(migrated.automation.instructions, "Weed the backlog.");
    assert_eq!(migrated.history.len(), 1);
    assert!(!dir.join("gardener.json").exists());
    assert!(dir.join("gardener.1.runs.json").exists());
    assert!(!dir.join("gardener.kradpet").exists());
    let rewritten = std::fs::read_to_string(dir.join("gardener.kraddie"))?;
    assert!(rewritten.contains("\"format\": 2"), "{rewritten}");
    assert!(!rewritten.contains("Weed"), "{rewritten}");
    assert_eq!(shelf.read_all(), first);
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn an_exported_kraddie_imports_back_with_its_automations_and_junk_is_refused() -> TestResult {
    let dir = scratch("export");
    std::fs::create_dir_all(&dir)?;
    let kraddie = Kraddie::new("Gardener".parse()?, AgentPreset::Codex);
    let mut automation = routine(1, "Weed")?;
    automation.session.working_copy = WorkingCopy::ThisRepo;
    automation.session.group = Some(PathBuf::from("/work/garden"));
    let path = dir.join("gardener.kraddie");
    export_kraddie(&kraddie, std::slice::from_ref(&automation), &path)?;
    let imported = import_kraddie(&path)?;
    assert_eq!(imported.kraddie, kraddie.exported());
    assert_eq!(imported.automations, vec![automation.exported()]);
    assert!(!std::fs::read_to_string(&path)?.contains("/work/garden"));
    let junk = dir.join("junk.kradpet");
    std::fs::write(&junk, b"{\"format\":1}")?;
    assert!(matches!(
        import_kraddie(&junk),
        Err(KraddieStoreError::File(_))
    ));
    assert!(matches!(
        import_kraddie(&dir.join("missing.kradpet")),
        Err(KraddieStoreError::Io(_))
    ));
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn kraddies_saved_in_the_old_folder_load_from_the_new_one() -> TestResult {
    let config = scratch("old-folder");
    let old = config.join(LEGACY_FOLDER);
    std::fs::create_dir_all(&old)?;
    let kraddie = Kraddie::new("Lantern".parse()?, AgentPreset::ClaudeCode);
    let text = KraddieFile::new(kraddie.clone(), vec![routine(1, "Sweep")?]).write()?;
    std::fs::write(old.join("lantern.kradpet"), text)?;
    let shelf = KraddieShelf::at(config.join("kraddies"));
    assert!(shelf.settle_folder().is_empty());
    let read = shelf.read_all();
    let [shelved] = read.as_slice() else {
        unreachable!("one Kraddie");
    };
    assert_eq!(shelved.kraddie, kraddie);
    assert!(config.join("kraddies").join("lantern.kraddie").exists());
    assert!(!old.exists());
    assert_eq!(shelf.read_all(), read);
    std::fs::remove_dir_all(&config)?;
    Ok(())
}

#[test]
fn an_old_file_beside_its_new_one_never_replaces_it() -> TestResult {
    let dir = scratch("both");
    std::fs::create_dir_all(&dir)?;
    let shelf = KraddieShelf::at(dir.clone());
    let kraddie = Kraddie::new("Gardener".parse()?, AgentPreset::Codex);
    shelf.write(&kraddie)?;
    let older = Kraddie::new("Gardener".parse()?, AgentPreset::ClaudeCode);
    let old = KraddieFile::new(older, Vec::new())
        .write()?
        .replace("\"format\": 2", "\"format\": 1");
    std::fs::write(dir.join("gardener.kradpet"), old)?;
    let read = shelf.read_all();
    let [shelved] = read.as_slice() else {
        unreachable!("one Kraddie");
    };
    assert_eq!(shelved.kraddie, kraddie);
    assert_eq!(
        import_kraddie(&dir.join("gardener.kraddie"))?.kraddie,
        kraddie
    );
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
