use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Child, Command, ExitStatus};

use crate::acp::terminals::TerminalExitStatus;

const KILL_PROGRAM: &str = "kill";
const KILL_SIGNAL: &str = "-KILL";
const END_OF_OPTIONS: &str = "--";
const OWN_GROUP: i32 = 0;
const UNNAMED_SIGNAL_PREFIX: &str = "SIG";
const SIGNAL_NAMES: [(i32, &str); 9] = [
    (1, "SIGHUP"),
    (2, "SIGINT"),
    (3, "SIGQUIT"),
    (6, "SIGABRT"),
    (9, "SIGKILL"),
    (11, "SIGSEGV"),
    (13, "SIGPIPE"),
    (14, "SIGALRM"),
    (15, "SIGTERM"),
];

pub(super) fn own_group(command: &mut Command) {
    command.process_group(OWN_GROUP);
}

pub(super) fn kill(child: &mut Child) {
    let group = format!("-{}", child.id());
    Command::new(KILL_PROGRAM)
        .args([KILL_SIGNAL, END_OF_OPTIONS, &group])
        .status()
        .map_or_default(drop);
    child.kill().unwrap_or_default();
}

pub(super) fn exit_status(status: ExitStatus) -> TerminalExitStatus {
    TerminalExitStatus {
        exit_code: status.code().and_then(|code| u32::try_from(code).ok()),
        signal: status.signal().map(signal_name),
    }
}

fn signal_name(number: i32) -> String {
    SIGNAL_NAMES
        .iter()
        .find(|(known, _)| *known == number)
        .map_or_else(
            || format!("{UNNAMED_SIGNAL_PREFIX}{number}"),
            |(_, name)| (*name).to_owned(),
        )
}
