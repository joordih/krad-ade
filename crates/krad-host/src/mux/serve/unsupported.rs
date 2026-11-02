use std::path::PathBuf;

use super::super::channel::Channel;

const ONLY_ON_LINUX: &str = "commands run only on Linux hosts";

pub(crate) enum Launch {
    Script(String),
    Program(PathBuf, Vec<String>),
}

pub(super) fn run(channel: &Channel, launch: &Launch) {
    match launch {
        Launch::Script(script) => tracing::debug!(bytes = script.len(), "{ONLY_ON_LINUX}"),
        Launch::Program(program, args) => {
            tracing::debug!(
                program = %program.display(),
                args = args.len(),
                "{ONLY_ON_LINUX}"
            );
        }
    }
    channel.refuse(ONLY_ON_LINUX);
}
