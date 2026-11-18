use super::super::automation::{AutomationId, Trigger, WorkingCopy};
use super::super::KraddieWorkspace;
use super::*;
use crate::agent::AgentPreset;
use crate::ids::EnvironmentName;
use std::path::PathBuf;

fn kraddie() -> Kraddie {
    let mut kraddie = Kraddie::new(
        "Nightly".parse().unwrap_or_else(|_| unreachable!()),
        AgentPreset::Codex,
    );
    kraddie.workspace = Some(KraddieWorkspace {
        environment: EnvironmentName::new("main".to_owned()),
        repo: PathBuf::from("/work/api"),
    });
    kraddie
}

fn routine() -> Automation {
    let mut automation = Automation::new(
        AutomationId(2),
        "Triage".parse().unwrap_or_else(|_| unreachable!()),
    );
    automation.triggers = vec![Trigger::Manual];
    automation.instructions = "Triage the failing jobs.".to_owned();
    automation.session.working_copy = WorkingCopy::ThisRepo;
    automation.session.group = Some(PathBuf::from("/work/api"));
    automation
}

#[test]
fn an_exported_kraddie_carries_its_automations_without_the_users_paths() {
    let exported = KraddieFile::new(kraddie().exported(), vec![routine().exported()]);
    let text = exported.write().unwrap_or_else(|_| unreachable!());
    assert!(text.contains("\"format\": 2"), "{text}");
    assert!(!text.contains("/work/api"), "{text}");
    let read = KraddieFile::read(&text).unwrap_or_else(|error| unreachable!("{error}"));
    assert_eq!(read.kraddie, exported.kraddie);
    assert_eq!(read.automations, exported.automations);
}

#[test]
fn importing_refuses_newer_formats_broken_designs_and_odd_kraddies() {
    let mut file = KraddieFile::new(kraddie(), Vec::new());
    file.format = KraddieFormatVersion(KRADDIE_FORMAT_VERSION.0 + 1);
    let newer = serde_json::to_string(&file).unwrap_or_else(|_| unreachable!());
    assert!(matches!(
        KraddieFile::read(&newer),
        Err(KraddieFileError::NewerFormat(_))
    ));
    assert!(matches!(
        KraddieFile::read("{}"),
        Err(KraddieFileError::Unreadable(_))
    ));
    assert!(matches!(
        KraddieFile::read("not json"),
        Err(KraddieFileError::Unreadable(_))
    ));
    let mut broken = kraddie();
    broken.design.palette.clear();
    let text = KraddieFile::new(broken, Vec::new())
        .write()
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(
        KraddieFile::read(&text),
        Err(KraddieFileError::Design(DesignError::NoPalette))
    );
    let twice = KraddieFile::new(kraddie(), vec![routine(), routine()])
        .write()
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(
        KraddieFile::read(&twice),
        Err(KraddieFileError::Automations(
            AutomationsFileError::RepeatedId
        ))
    );
    let shell = Kraddie::new(
        "Shell".parse().unwrap_or_else(|_| unreachable!()),
        AgentPreset::Shell,
    );
    let text = KraddieFile::new(shell, Vec::new())
        .write()
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(KraddieFile::read(&text), Err(KraddieFileError::NotAnAgent));
    let bad_name = KraddieFile::new(kraddie(), Vec::new())
        .write()
        .unwrap_or_else(|_| unreachable!())
        .replace("\"Nightly\"", "\"   \"");
    assert!(matches!(
        KraddieFile::read(&bad_name),
        Err(KraddieFileError::Unreadable(_))
    ));
}
