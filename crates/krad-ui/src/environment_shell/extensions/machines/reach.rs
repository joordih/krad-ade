use krad_extensions::api::protocol::{Machine, MachineState, TerminalCommand};
use krad_extensions::api::HostKindId;
use krad_extensions::Trust;
use krad_host::MachineChannel;

use super::super::ExtensionRecord;
use super::HostKindRef;

const MISSING_TEXT: &str = "is not installed; add it again in Settings to reach this machine";
const DISABLED_TEXT: &str = "is disabled; enable it in Settings to reach this machine";
const UNTRUSTED_TEXT: &str = "is not trusted; review it in Settings to reach this machine";
const NO_HOST_KIND_TEXT: &str = "no longer offers this kind of machine";
const STOPPED_HINT: &str = "Stopped; start it first";
const NEW_PREFIX: &str = "New";
const ENVIRONMENT_SUFFIX: &str = "environment…";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reach {
    Reachable,
    Missing,
    Disabled,
    Untrusted,
    NoHostKind,
}

impl Reach {
    pub(super) fn explain(self, extension: &str) -> String {
        let reason = match self {
            Self::Reachable => return String::new(),
            Self::Missing => MISSING_TEXT,
            Self::Disabled => DISABLED_TEXT,
            Self::Untrusted => UNTRUSTED_TEXT,
            Self::NoHostKind => NO_HOST_KIND_TEXT,
        };
        format!("{extension} {reason}")
    }
}

pub(super) fn reach_of(record: Option<&ExtensionRecord>, kind: &HostKindId) -> Reach {
    let Some(record) = record else {
        return Reach::Missing;
    };
    if !record.entry.enabled {
        return Reach::Disabled;
    }
    if record.trust() != Trust::Trusted || !record.usable() {
        return Reach::Untrusted;
    }
    let offers = record.manifest().is_some_and(|manifest| {
        manifest
            .contributes
            .host_kinds
            .iter()
            .any(|offered| offered.id == *kind)
    });
    if offers {
        Reach::Reachable
    } else {
        Reach::NoHostKind
    }
}

pub(super) fn host_kind_offers(records: &[ExtensionRecord]) -> Vec<(String, HostKindRef)> {
    records
        .iter()
        .filter(|record| record.usable())
        .filter_map(ExtensionRecord::manifest)
        .flat_map(|manifest| {
            manifest.contributes.host_kinds.iter().map(|kind| {
                (
                    format!("{NEW_PREFIX} {} {ENVIRONMENT_SUFFIX}", kind.title),
                    HostKindRef {
                        extension: manifest.id.clone(),
                        kind: kind.id.clone(),
                    },
                )
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct MachineChoice {
    pub(in crate::environment_shell) machine: Machine,
    pub(in crate::environment_shell) hint: Option<&'static str>,
}

impl MachineChoice {
    pub(in crate::environment_shell) fn available(&self) -> bool {
        self.hint.is_none()
    }
}

pub(super) fn machine_choices(machines: Vec<Machine>) -> Vec<MachineChoice> {
    let mut choices: Vec<MachineChoice> = machines
        .into_iter()
        .map(|machine| {
            let hint = match machine.state {
                MachineState::Running => None,
                MachineState::Stopped => Some(STOPPED_HINT),
            };
            MachineChoice { machine, hint }
        })
        .collect();
    choices.sort_by(|left, right| {
        right
            .available()
            .cmp(&left.available())
            .then_with(|| left.machine.label.cmp(&right.machine.label))
    });
    choices
}

pub(super) fn machine_channel(command: TerminalCommand) -> MachineChannel {
    MachineChannel {
        program: command.program,
        args: command.args,
        env: command.env.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{ExtensionEntry, ExtensionSource};
    use krad_extensions::inspect;
    use krad_extensions::program::ProgramLog;
    use std::collections::BTreeMap;

    use super::super::super::Loaded;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const DOCKER: &str = r#"
id = "docker"
name = "Docker"
version = "0.1.0"
api = "1.0"

[program]
placement = "app"
runtime = "node"
args = ["dist/main.js"]

[[contributes.host_kind]]
id = "container"
title = "Docker container"
"#;

    static NEXT_FOLDER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    fn record(enabled: bool, trusted: bool) -> Result<ExtensionRecord, Box<dyn std::error::Error>> {
        let serial = NEXT_FOLDER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let folder =
            std::env::temp_dir().join(format!("krad-machines-{}-{serial}", std::process::id()));
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join("extension.toml"), DOCKER)?;
        let untrusted = ExtensionEntry {
            id: "docker".parse()?,
            source: ExtensionSource::Folder(folder.clone()),
            enabled,
            trusted: None,
        };
        let fingerprint = inspect(&folder, untrusted.clone())?.fingerprint;
        let entry = ExtensionEntry {
            trusted: trusted.then_some(fingerprint),
            ..untrusted
        };
        let inspected = inspect(&folder, entry.clone())?;
        Ok(ExtensionRecord {
            entry,
            loaded: Loaded::Ready(Box::new(inspected)),
            log: ProgramLog::default(),
        })
    }

    fn machine(
        id: &str,
        label: &str,
        state: MachineState,
    ) -> Result<Machine, Box<dyn std::error::Error>> {
        Ok(Machine {
            id: id.parse()?,
            label: label.to_owned(),
            description: None,
            state,
        })
    }

    #[test]
    fn a_machine_is_reachable_only_through_an_enabled_trusted_extension_offering_its_kind(
    ) -> TestResult {
        let container: HostKindId = "container".parse()?;
        assert_eq!(
            reach_of(Some(&record(true, true)?), &container),
            Reach::Reachable
        );
        assert_eq!(
            reach_of(Some(&record(false, true)?), &container),
            Reach::Disabled
        );
        assert_eq!(
            reach_of(Some(&record(true, false)?), &container),
            Reach::Untrusted
        );
        assert_eq!(reach_of(None, &container), Reach::Missing);
        assert_eq!(
            reach_of(Some(&record(true, true)?), &"pod".parse()?),
            Reach::NoHostKind
        );
        assert!(Reach::Disabled
            .explain("Docker")
            .starts_with("Docker is disabled"));
        Ok(())
    }

    #[test]
    fn only_usable_extensions_offer_their_host_kinds() -> TestResult {
        let offers = host_kind_offers(&[record(true, true)?, record(false, true)?]);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].0, "New Docker container environment…");
        assert_eq!(offers[0].1.kind.as_ref(), "container");
        Ok(())
    }

    #[test]
    fn running_machines_come_first_and_stopped_ones_carry_a_hint() -> TestResult {
        let choices = machine_choices(vec![
            machine("b", "worker", MachineState::Stopped)?,
            machine("c", "web", MachineState::Running)?,
            machine("a", "api", MachineState::Running)?,
        ]);
        let labels: Vec<&str> = choices
            .iter()
            .map(|choice| choice.machine.label.as_str())
            .collect();
        assert_eq!(labels, ["api", "web", "worker"]);
        assert!(choices[0].available());
        assert_eq!(choices[2].hint, Some(STOPPED_HINT));
        Ok(())
    }

    #[test]
    fn a_connect_answer_becomes_the_links_channel() {
        let command = TerminalCommand {
            program: "docker".to_owned(),
            args: vec![
                "exec".to_owned(),
                "-i".to_owned(),
                "web".to_owned(),
                "sh".to_owned(),
            ],
            env: BTreeMap::from([(
                "DOCKER_HOST".to_owned(),
                "unix:///run/docker.sock".to_owned(),
            )]),
        };
        let channel = machine_channel(command);
        assert_eq!(channel.program, "docker");
        assert_eq!(channel.args, ["exec", "-i", "web", "sh"]);
        assert_eq!(
            channel.env,
            [(
                "DOCKER_HOST".to_owned(),
                "unix:///run/docker.sock".to_owned()
            )]
        );
    }
}
