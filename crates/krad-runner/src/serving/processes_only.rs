use krad_host::runner::EXIT_OK;
use std::io;
use std::path::Path;

use crate::daemon::RunnerDaemon;

pub(crate) fn serve(root: &Path) -> io::Result<i32> {
    let daemon = RunnerDaemon::new(root);
    let listener = daemon.bind()?;
    tracing::info!("cloud runner listening");
    daemon.serve(&listener);
    Ok(EXIT_OK)
}
