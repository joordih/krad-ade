use krad_host::runner::EXIT_OK;
use std::io;
use std::path::Path;

use crate::daemon::RunnerDaemon;

pub enum Transport {}

pub(crate) fn serve(root: &Path, transports: Vec<Transport>) -> io::Result<i32> {
    if let Some(transport) = transports.into_iter().next() {
        match transport {}
    }
    let daemon = RunnerDaemon::new(root);
    let listener = daemon.bind()?;
    tracing::info!("cloud runner listening");
    daemon.serve(&listener);
    Ok(EXIT_OK)
}
