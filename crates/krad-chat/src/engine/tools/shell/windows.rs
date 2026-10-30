use std::process::{Child, Command};

const SHELL_PROGRAM: &str = "cmd";
const COMMAND_FLAG: &str = "/C";

pub(super) fn command(line: &str) -> Command {
    let mut command = Command::new(SHELL_PROGRAM);
    command.arg(COMMAND_FLAG).arg(line);
    command
}

pub(super) fn kill(child: &mut Child) {
    child.kill().unwrap_or_default();
}
