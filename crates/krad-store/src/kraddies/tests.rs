use super::*;
use krad_domain::{AgentPreset, AutomationId, KraddieOutcome, KraddieWake, Trigger, WorkingCopy};
use std::time::SystemTime;

type TestResult = Result<(), Box<dyn std::error::Error>>;

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
    std::fs::write(dir.join("broken.kraddie"), b"{")?;
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
    let junk = dir.join("junk.kraddie");
    std::fs::write(&junk, b"{\"format\":1}")?;
    assert!(matches!(
        import_kraddie(&junk),
        Err(KraddieStoreError::File(_))
    ));
    assert!(matches!(
        import_kraddie(&dir.join("missing.kraddie")),
        Err(KraddieStoreError::Io(_))
    ));
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn a_kraddie_file_without_its_automations_file_hands_them_over() -> TestResult {
    let dir = scratch("adopt");
    std::fs::create_dir_all(&dir)?;
    let kraddie = Kraddie::new("Lantern".parse()?, AgentPreset::ClaudeCode);
    let automations = vec![routine(1, "Sweep")?];
    let text = KraddieFile::new(kraddie.clone(), automations.clone()).write()?;
    std::fs::write(dir.join("lantern.kraddie"), text)?;
    let shelf = KraddieShelf::at(dir.clone());
    let read = shelf.read_all();
    let [shelved] = read.as_slice() else {
        unreachable!("one Kraddie");
    };
    assert_eq!(shelved.kraddie, kraddie);
    assert_eq!(shelved.migration_problem, None);
    assert!(dir.join("lantern.automations.json").exists());
    assert_eq!(shelf.read_all(), read);
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
