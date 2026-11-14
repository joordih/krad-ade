use krad_host::runner::EXIT_OK;
use krad_runtime::Runtime;
use krad_store::Secrets;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;

use crate::daemon::RunnerDaemon;
use crate::hosted::{bind_runtime_socket, RunnerProfile, RuntimeServer};

const PROCESSES_THREAD: &str = "krad-runner-processes";

fn cli_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .inspect_err(|error| tracing::debug!(%error, "the runner does not know its own program"))
        .ok()?
        .parent()
        .map(Path::to_path_buf)
}

pub(crate) fn serve(root: &Path) -> io::Result<i32> {
    let daemon = RunnerDaemon::new(root);
    let processes = daemon.bind()?;
    let runtime = bind_runtime_socket(root)?;
    let handle = Runtime::start(
        RunnerProfile {
            root: root.to_owned(),
            secrets: Secrets::system(),
            cli_dir: cli_dir(),
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
