use std::path::{Component, Path};

const MIN_REMOVABLE_DEPTH: usize = 2;
const PROTECTED_TREES: [&str; 3] = ["/proc", "/sys", "/dev"];
const PROTECTED_FOLDERS: [&str; 16] = [
    "/usr/bin",
    "/usr/lib",
    "/usr/lib64",
    "/usr/local",
    "/usr/sbin",
    "/usr/share",
    "/var/lib",
    "/var/log",
    "/var/cache",
    "/var/spool",
    "/etc/ssh",
    "/etc/systemd",
    "/run/user",
    "/home/root",
    "/opt/conda",
    "/snap/bin",
];

pub(crate) fn is_removable(path: &Path, home: Option<&Path>) -> bool {
    if !path.has_root() {
        return false;
    }
    let mut depth = 0;
    for component in path.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::RootDir => {}
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => return false,
        }
    }
    if depth < MIN_REMOVABLE_DEPTH {
        return false;
    }
    if PROTECTED_TREES
        .iter()
        .any(|tree| path.starts_with(Path::new(tree)))
        || PROTECTED_FOLDERS
            .iter()
            .any(|folder| path == Path::new(folder))
    {
        return false;
    }
    home.is_none_or(|home| !home.starts_with(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_below_the_home_or_a_project_can_go() {
        let home = Some(Path::new("/home/me"));
        assert!(is_removable(
            Path::new("/home/me/projects/app/node_modules"),
            home
        ));
        assert!(is_removable(Path::new("/home/me/scratch"), home));
        assert!(is_removable(Path::new("/srv/app/build"), home));
    }

    #[test]
    fn the_root_the_home_its_parents_and_system_folders_never_go() {
        let home = Some(Path::new("/home/me"));
        for path in [
            "/", "/home", "/home/me", "/usr", "/usr/lib", "/proc/1", "/dev/shm",
        ] {
            assert!(!is_removable(Path::new(path), home), "{path}");
        }
    }

    #[test]
    fn relative_and_dotted_paths_never_go() {
        let home = Some(Path::new("/home/me"));
        assert!(!is_removable(Path::new("/home/me/../.."), home));
        assert!(!is_removable(Path::new("work/app"), home));
    }
}
