type TestResult = Result<(), Box<dyn std::error::Error>>;

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::super::bootstrap::{self, Greeting};
use super::super::MuxError;
use super::{MuxCarrier, MuxTools};
use crate::command::transport;
use crate::error::HostError;
use crate::layout::{APP_DIR, BIN_DIR, HOST_CLI_BUILD, HOST_CLI_PROGRAM};
use crate::machine::MachineChannel;
use crate::platform::HostPlatform;
use crate::ssh::SshArgs;

const SHELLS: [&str; 2] = ["/bin/sh", "/bin/dash"];

fn temp_home(name: &str) -> std::io::Result<PathBuf> {
    let home = std::env::temp_dir().join(format!("krad-machine-{name}-{}", std::process::id()));
    if home.exists() {
        fs::remove_dir_all(&home)?;
    }
    fs::create_dir_all(&home)?;
    Ok(home)
}

fn args() -> Result<SshArgs, Box<dyn std::error::Error>> {
    Ok(SshArgs::new("machine".parse()?, None))
}

fn tools() -> MuxTools {
    MuxTools {
        local_cli: Some(PathBuf::from("/bin/true")),
        ..MuxTools::default()
    }
}

#[test]
fn a_shell_on_stdin_runs_the_bootstrap_uploads_the_cli_and_starts_serve() -> TestResult {
    for shell in SHELLS {
        if !Path::new(shell).exists() {
            continue;
        }
        let home = temp_home(&shell.replace('/', "-"))?;
        let cli = home.join("fake-cli");
        fs::write(
            &cli,
            format!("#!/bin/sh\n[ \"$1\" = --version ] && echo {HOST_CLI_BUILD} && exit 0\necho served \"$1\"\n"),
        )?;
        let mut child = Command::new(shell)
            .env("HOME", &home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        let mut stdout = BufReader::new(child.stdout.take().ok_or("no stdout")?);
        let script = transport::inline(&bootstrap::script()).ok_or("the bootstrap fits inline")?;
        stdin.write_all(format!("{script}\n").as_bytes())?;
        stdin.flush()?;
        let mut greeting = String::new();
        stdout.read_line(&mut greeting)?;
        assert!(
            greeting.starts_with("KRAD-MUX missing "),
            "{shell}: {greeting}"
        );
        let linux = Greeting {
            platform: HostPlatform::LinuxX86_64,
            cli_ready: false,
        };
        bootstrap::answer(&mut stdin, &linux, &[(HostPlatform::LinuxX86_64, cli)])?;
        let mut rest = String::new();
        stdout.read_to_string(&mut rest)?;
        assert_eq!(rest, "served serve\n", "{shell}");
        assert!(child.wait()?.success());
        let installed = home
            .join(APP_DIR)
            .join(HOST_CLI_BUILD)
            .join(BIN_DIR)
            .join(HOST_CLI_PROGRAM);
        assert!(installed.is_file(), "{shell}");
        fs::remove_dir_all(&home)?;
    }
    Ok(())
}

#[test]
fn a_channel_that_fails_says_why() -> TestResult {
    let channel = MachineChannel {
        program: "/bin/sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            "echo 'Error: No such container: web' >&2; exit 1".to_owned(),
        ],
        env: Vec::new(),
    };
    let failed = MuxCarrier::through(&channel, &args()?, &tools());
    let Err(HostError::MachineChannel(reason)) = failed else {
        return Err("the channel should fail with its own message".into());
    };
    assert_eq!(reason, "Error: No such container: web");
    Ok(())
}

#[test]
fn a_machine_without_a_matching_cli_is_refused() -> TestResult {
    let home = temp_home("no-cli")?;
    let channel = MachineChannel {
        program: "/bin/sh".to_owned(),
        args: Vec::new(),
        env: vec![("HOME".to_owned(), home.to_string_lossy().into_owned())],
    };
    let refused = MuxCarrier::through(&channel, &args()?, &tools());
    assert!(matches!(
        refused,
        Err(HostError::UnsupportedPlatform(_) | HostError::Mux(MuxError::NoHostCli(_)))
    ));
    fs::remove_dir_all(&home)?;
    Ok(())
}
