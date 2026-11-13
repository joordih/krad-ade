use krad_agents::{terminal_env, Caller};
use std::path::PathBuf;

use crate::bus::BusAccess;

const PATH_ENV: &str = "PATH";
const WSLENV_VAR: &str = "WSLENV";
const WSLENV_SEPARATOR: &str = ":";

#[must_use]
pub fn local_caller_vars(bus: &BusAccess, caller: &Caller, in_wsl: bool) -> Vec<(String, String)> {
    let mut vars = terminal_env(&bus.endpoint, caller);
    let mut search: Vec<PathBuf> = bus.cli_dir.iter().cloned().collect();
    search.extend(
        std::env::var_os(PATH_ENV)
            .iter()
            .flat_map(std::env::split_paths),
    );
    if let Ok(path) = std::env::join_paths(search) {
        vars.push((PATH_ENV.to_owned(), path.to_string_lossy().into_owned()));
    }
    if in_wsl {
        let shared: Vec<String> = vars
            .iter()
            .filter(|(name, _)| name != PATH_ENV)
            .map(|(name, _)| name.clone())
            .collect();
        vars.push((WSLENV_VAR.to_owned(), shared.join(WSLENV_SEPARATOR)));
    }
    vars
}
