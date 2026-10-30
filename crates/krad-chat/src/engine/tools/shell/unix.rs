use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command};

const SHELL_VAR: &str = "SHELL";
const FALLBACK_SHELL: &str = "/bin/sh";
const LOGIN_COMMAND_FLAG: &str = "-lc";
const KILL_PROGRAM: &str = "kill";
const KILL_SIGNAL: &str = "-KILL";
const OWN_GROUP: i32 = 0;

pub(super) fn command(line: &str) -> Command {
    let shell = std::env::var(SHELL_VAR)
        .ok()
        .filter(|shell| !shell.trim().is_empty())
        .unwrap_or_else(|| FALLBACK_SHELL.to_owned());
    let mut command = Command::new(shell);
    command
        .arg(LOGIN_COMMAND_FLAG)
        .arg(line)
        .process_group(OWN_GROUP);
    command
}

pub(super) fn kill(child: &mut Child) {
    let group = format!("-{}", child.id());
    Command::new(KILL_PROGRAM)
        .args([KILL_SIGNAL, "--", &group])
        .status()
        .map_or_default(drop);
    child.kill().unwrap_or_default();
}
