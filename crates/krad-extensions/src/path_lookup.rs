use std::env;
use std::path::{Path, PathBuf};

use std::env::consts::EXE_SUFFIX;

const PATH_VARIABLE: &str = "PATH";

pub(crate) fn find_on_path(name: &str) -> Option<PathBuf> {
    let file = format!("{name}{EXE_SUFFIX}");
    let paths = env::var_os(PATH_VARIABLE)?;
    env::split_paths(&paths)
        .map(|folder| folder.join(&file))
        .find(|candidate| candidate.is_file())
}

pub(crate) fn segment_in(root: &Path, name: &str) -> Option<PathBuf> {
    krad_domain::check_id(name).ok().map(|()| root.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_safe_segment_is_joined() {
        let root = Path::new("/config/server-runtimes");
        assert_eq!(segment_in(root, "node"), Some(root.join("node")));
        for unsafe_name in ["..", ".", "../x", "a/b", "/etc", ".hidden"] {
            assert_eq!(segment_in(root, unsafe_name), None, "{unsafe_name}");
        }
    }
}
