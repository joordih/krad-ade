use super::super::automation::{AppTrigger, AutomationId, CodeHostTrigger, Trigger, WorkingCopy};
use super::super::schedule::KraddieSchedule;
use super::super::{KraddieWorkspace, Weekdays};
use super::*;
use crate::agent::AgentPreset;
use crate::attention::TimeOfDay;
use crate::code_host::CodeHost;
use crate::ids::EnvironmentName;
use std::path::PathBuf;

const KRADDIE_SWITCH: &str = "\"switch\": \"on\"";

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

fn first_format(kraddie: &Kraddie, legacy: &str) -> String {
    KraddieFile::new(kraddie.exported(), Vec::new())
        .write()
        .unwrap_or_else(|_| unreachable!())
        .replace("\"format\": 2", "\"format\": 1")
        .replacen(KRADDIE_SWITCH, &format!("{KRADDIE_SWITCH}{legacy}"), 1)
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
    assert!(!read.migrated);
}

#[test]
fn a_first_format_kraddie_becomes_one_automation_with_its_instructions_and_triggers() {
    let legacy = r#",
    "instructions": "Keep the build green.",
    "schedule": {"kind": "at", "time": "06:30", "days": ["mon", "tue", "wed", "thu", "fri"]},
    "events": ["ci_failed", "git_changed"]"#;
    let read = KraddieFile::read(&first_format(&kraddie(), legacy))
        .unwrap_or_else(|error| unreachable!("{error}"));
    assert!(read.migrated);
    let [automation] = read.automations.as_slice() else {
        unreachable!("one automation");
    };
    assert_eq!(automation.id, AutomationId::FIRST);
    assert_eq!(automation.name.as_str(), "Nightly");
    assert_eq!(automation.instructions, "Keep the build green.");
    let schedule = KraddieSchedule::At {
        time: TimeOfDay::new(6, 30).unwrap_or_else(|_| unreachable!()),
        days: Weekdays::WORKDAYS,
    };
    assert_eq!(
        automation.triggers,
        vec![
            Trigger::Manual,
            Trigger::Schedule { schedule },
            Trigger::CodeHost {
                host: CodeHost::Github,
                event: CodeHostTrigger::CiFailed,
            },
            Trigger::CodeHost {
                host: CodeHost::Gitlab,
                event: CodeHostTrigger::CiFailed,
            },
            Trigger::App {
                event: AppTrigger::GitChanged,
            },
        ]
    );
}

#[test]
fn a_bare_first_format_kraddie_still_loads_and_can_be_run_by_hand() {
    let read = KraddieFile::read(&first_format(&kraddie(), ""))
        .unwrap_or_else(|error| unreachable!("{error}"));
    assert_eq!(read.kraddie.name.as_str(), "Nightly");
    let [automation] = read.automations.as_slice() else {
        unreachable!("one automation");
    };
    assert!(automation.is_manual());
    assert_eq!(automation.instructions, "");
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

const LEGACY_KRADDIE_KEY: &str = "\"pet\":";
const LEGACY_ROUNDS_KEY: &str = "\"heartbeat\":";

#[test]
fn a_file_from_before_the_rename_still_reads() {
    let mut owner = kraddie();
    owner.settings.rounds.switch = crate::attention::Switch::On;
    let text = KraddieFile::new(owner.clone(), vec![routine()])
        .write()
        .unwrap_or_else(|_| unreachable!());
    let old = text
        .replacen("\"kraddie\":", LEGACY_KRADDIE_KEY, 1)
        .replacen("\"rounds\":", LEGACY_ROUNDS_KEY, 1);
    assert!(
        old.contains(LEGACY_KRADDIE_KEY) && old.contains(LEGACY_ROUNDS_KEY),
        "{old}"
    );
    let read = KraddieFile::read(&old).unwrap_or_else(|error| unreachable!("{error}"));
    assert_eq!(read.kraddie, owner);
    assert_eq!(read.automations, vec![routine()]);
}
