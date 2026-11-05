use super::*;
use krad_domain::Host;

use crate::command::HostCommand;
use crate::context::HostContext;
use crate::error::HostError;
use crate::link::HostLink;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn web() -> ExtensionMachine {
    ExtensionMachine {
        extension: "docker".to_owned(),
        kind: "container".to_owned(),
        id: "4f2a9c".to_owned(),
        label: "web".to_owned(),
    }
}

fn machine_link() -> Result<std::sync::Arc<crate::link::SshLink>, &'static str> {
    let host = Host::Extension { machine: web() };
    match HostLink::new(&host, &HostContext::default()) {
        HostLink::Ssh(ssh) => Ok(ssh),
        HostLink::Local => Err("an extension machine is a remote link"),
    }
}

#[test]
fn a_channel_never_prints_its_environment_values() {
    let channel = MachineChannel {
        program: "docker".to_owned(),
        args: vec![
            "exec".to_owned(),
            "-i".to_owned(),
            "web".to_owned(),
            "sh".to_owned(),
        ],
        env: vec![("DOCKER_TOKEN".to_owned(), "hunter2".to_owned())],
    };
    let printed = format!("{channel:?}");
    assert!(printed.contains("DOCKER_TOKEN"));
    assert!(!printed.contains("hunter2"));
}

#[test]
fn a_machine_link_names_its_machine_and_is_never_ready_before_it_connects() -> TestResult {
    let ssh = machine_link()?;
    let link = HostLink::Ssh(std::sync::Arc::clone(&ssh));
    assert_eq!(link.host(), Host::Extension { machine: web() });
    assert!(!link.is_ready());
    assert_eq!(ssh.machine(), Some(&web()));
    Ok(())
}

#[test]
fn a_machine_without_its_channel_does_not_connect() -> TestResult {
    let ssh = machine_link()?;
    assert!(matches!(ssh.connect(), Err(HostError::NoMachineChannel)));
    Ok(())
}

#[test]
fn a_machine_never_falls_back_to_ssh() -> TestResult {
    let ssh = machine_link()?;
    let link = HostLink::Ssh(std::sync::Arc::clone(&ssh));
    let command = HostCommand::new("ls");
    assert!(matches!(command.build(&link), Err(HostError::NotConnected)));
    assert!(matches!(
        command.build_terminal(&link),
        Err(HostError::NotConnected)
    ));
    assert!(matches!(
        ssh.forward_local(80),
        Err(HostError::NotConnected)
    ));
    assert!(matches!(
        ssh.forward_remote(80),
        Err(HostError::NotConnected)
    ));
    Ok(())
}
