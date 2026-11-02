use super::super::channel::Channel;
use super::super::wire::TerminalSize;

const ONLY_ON_LINUX: &str = "terminals open only on Linux hosts";

pub(in crate::mux) fn run(channel: &Channel, script: &str, size: TerminalSize) {
    tracing::debug!(
        bytes = script.len(),
        cols = size.cols,
        rows = size.rows,
        "{ONLY_ON_LINUX}"
    );
    channel.refuse(ONLY_ON_LINUX);
}
