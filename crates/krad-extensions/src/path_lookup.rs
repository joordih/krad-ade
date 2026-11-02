use std::env;
use std::path::PathBuf;

use std::env::consts::EXE_SUFFIX;

const PATH_VARIABLE: &str = "PATH";

pub(crate) fn find_on_path(name: &str) -> Option<PathBuf> {
    let file = format!("{name}{EXE_SUFFIX}");
    let paths = env::var_os(PATH_VARIABLE)?;
    env::split_paths(&paths)
        .map(|folder| folder.join(&file))
        .find(|candidate| candidate.is_file())
}
