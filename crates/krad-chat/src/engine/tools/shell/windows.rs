use krad_host::hide_console_window;
use std::process::{Child, Command};

const SHELL_PROGRAM: &str = "cmd";
const COMMAND_FLAG: &str = "/C";

pub(super) const fn own_group(_command: &mut Command) {}

pub(super) fn command(line: &str) -> Command {
    let mut command = Command::new(SHELL_PROGRAM);
    command.arg(COMMAND_FLAG).arg(line);
    hide_console_window(&mut command);
    command
}

pub(super) fn kill(child: &mut Child) {
    child.kill().unwrap_or_default();
}
