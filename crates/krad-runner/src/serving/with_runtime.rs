use krad_domain::{DeviceName, InstallId, Pairing, PairingPermissions};
use krad_host::runner::EXIT_OK;
use krad_runtime::Runtime;
use krad_store::Secrets;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;

use crate::daemon::RunnerDaemon;
use crate::hosted::{bind_runtime_socket, OwnDaemon, RunnerProfile, RuntimeServer};
use crate::pairings::pair;

const PROCESSES_THREAD: &str = "krad-runner-processes";
const OWN_INSTALL: &str = "krad-runner-own-runtime";
const OWN_DEVICE: &str = "This runner's runtime";
const BAD_OWN_NAME: &str = "the runner's own pairing has an invalid name";

fn own_cli() -> Option<PathBuf> {
    std::env::current_exe()
        .inspect_err(|error| tracing::debug!(%error, "the runner does not know its own program"))
        .ok()
}

fn cli_dir() -> Option<PathBuf> {
    own_cli()?.parent().map(Path::to_path_buf)
}

fn own_daemon(root: &Path) -> io::Result<Option<OwnDaemon>> {
    let Some(cli) = own_cli() else {
        return Ok(None);
    };
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, BAD_OWN_NAME);
    let install: InstallId = OWN_INSTALL.parse().map_err(|_| invalid())?;
    let device: DeviceName = OWN_DEVICE.parse().map_err(|_| invalid())?;
    let token = pair(root, &install, device, PairingPermissions::all())?;
    Ok(Some(OwnDaemon {
        cli,
        pairing: Pairing { install, token },
    }))
}

pub(crate) fn serve(root: &Path) -> io::Result<i32> {
    serve_runner(root, Secrets::system())
}

/// # Errors
///
/// Fails when the runner's sockets cannot be bound or its runtime cannot start.
pub fn serve_runner(root: &Path, secrets: Secrets) -> io::Result<i32> {
    let daemon = RunnerDaemon::new(root);
    let processes = daemon.bind()?;
    let runtime = bind_runtime_socket(root)?;
    let handle = Runtime::start(
        RunnerProfile {
            root: root.to_owned(),
            secrets,
            cli_dir: cli_dir(),
            daemon: own_daemon(root)?,
        }
        .setup(),
    )?;
    thread::Builder::new()
        .name(PROCESSES_THREAD.to_owned())
        .spawn(move || daemon.serve(&processes))?;
    tracing::info!("cloud runner listening with its runtime");
    RuntimeServer::new(handle, root).serve(&runtime);
    Ok(EXIT_OK)
}
