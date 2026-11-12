#[cfg(unix)]
mod attach;
#[cfg(unix)]
mod cli;
#[cfg(unix)]
mod clients;
mod commands;
#[cfg(unix)]
mod daemon;
mod events;
mod header;
#[cfg(unix)]
mod journal;
#[cfg(unix)]
mod listing;
#[cfg(unix)]
mod process;
#[cfg(unix)]
mod pty;
mod service;
mod status;
#[cfg(all(test, unix))]
mod tests;
#[cfg(unix)]
mod wire;

#[cfg(unix)]
pub use attach::{attach, list, resize, stop};
#[cfg(unix)]
pub use cli::run_runner_verb;
#[cfg(unix)]
pub use clients::paired_installs;
pub use commands::{
    attach_terminal_command, authorize, pair_command, resize_command, run_authorized,
    status_command, stop_command, unpair_command, TerminalAttach,
};
#[cfg(unix)]
pub use daemon::{runner_socket, RunnerDaemon};
pub use events::{events_file, take_events_command, webhooks_dir, webhooks_file};
#[cfg(unix)]
pub use events::{home_runner_dir, queue_events};
pub use header::{InvalidJournalHeader, JournalHeader, PastEndNotice};
pub use service::{
    install_command, install_script, service_unit, uninstall_command, uninstall_script,
    InstallReport, Linger, ServiceState, UnreportedInstall, LINGER_UNAVAILABLE_MARKER,
    READY_MARKER, SERVICE_NAME, UPDATE_PENDING_MARKER,
};
pub use status::{parse_status, CloudProcessStatus, InvalidStatusLine};
#[cfg(unix)]
pub use wire::CloudLaunch;

pub const RUNNER_VERB: &str = "cloud-runner";
pub const ATTACH_VERB: &str = "cloud-attach";
pub const STOP_VERB: &str = "cloud-stop";
pub const RESIZE_VERB: &str = "cloud-resize";
pub const STATUS_VERB: &str = "cloud-status";
pub const PAIR_VERB: &str = "cloud-pair";
pub const UNPAIR_VERB: &str = "cloud-unpair";
pub const INSTALL_ENV: &str = "KRAD_ADE_CLOUD_INSTALL";
pub const TOKEN_ENV: &str = "KRAD_ADE_CLOUD_TOKEN";
pub const PROCESS_ENV: &str = "KRAD_ADE_CLOUD_PROCESS";
