use std::path::{Path, PathBuf};

#[cfg(unix)]
mod attach;
#[cfg(unix)]
mod cli;
mod commands;
#[cfg(unix)]
mod connect;
mod events;
#[cfg(unix)]
mod folder;
mod header;
mod service;
mod status;
#[cfg(unix)]
pub mod wire;

#[cfg(unix)]
pub use attach::{attach, list, resize, stop};
#[cfg(unix)]
pub use cli::{report as report_verb_failure, run_client_verb, runner_root, EXIT_FAILED, EXIT_OK};
pub use commands::{
    attach_terminal_command, authorize, connect_command, pair_command, resize_command,
    run_authorized, status_command, stop_command, unpair_command, TerminalAttach,
};
#[cfg(unix)]
pub use connect::bridge;
pub use events::{events_file, take_events_command, webhooks_dir, webhooks_file};
#[cfg(unix)]
pub use events::{home_runner_dir, queue_events};
#[cfg(unix)]
pub use folder::{clients_dir, ensure_private_dir, paired_installs, pairing_file};
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
pub const CONNECT_VERB: &str = "cloud-connect";
pub const SOCKET_FILE: &str = "daemon.sock";
pub const RUNTIME_SOCKET_FILE: &str = "runtime.sock";

#[must_use]
pub fn runner_socket(root: &Path) -> PathBuf {
    root.join(SOCKET_FILE)
}

#[must_use]
pub fn runtime_socket(root: &Path) -> PathBuf {
    root.join(RUNTIME_SOCKET_FILE)
}
