use std::process::{Child, Command, ExitStatus};

use crate::acp::terminals::TerminalExitStatus;

pub(super) const fn own_group(_command: &mut Command) {}

pub(super) fn kill(child: &mut Child) {
    child.kill().unwrap_or_default();
}

pub(super) fn exit_status(status: ExitStatus) -> TerminalExitStatus {
    TerminalExitStatus {
        exit_code: status
            .code()
            .map(|code| u32::from_ne_bytes(code.to_ne_bytes())),
        signal: None,
    }
}
