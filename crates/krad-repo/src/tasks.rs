use krad_domain::{
    cargo_tasks, just_tasks, make_tasks, package_script_tasks, NodePackageManager, ProjectTask,
    CARGO_MANIFEST, JUSTFILE_NAMES, MAKEFILE_NAMES, PACKAGE_MANIFEST,
};
use krad_host::HostLink;
use std::path::{Path, PathBuf};

use crate::files::read_text;
use crate::RepoError;

const MAX_TASK_FILE_BYTES: u64 = 512 * 1024;

#[derive(Debug, Default)]
pub struct TaskDetection {
    pub tasks: Vec<ProjectTask>,
    pub unreadable: Vec<(PathBuf, RepoError)>,
}

#[must_use]
pub fn detect_tasks(host: &HostLink, root: &Path) -> TaskDetection {
    let mut detection = TaskDetection::default();
    let manager = NodePackageManager::from_lockfiles(|file| host.exists(&root.join(file)));
    detection.read(host, &root.join(PACKAGE_MANIFEST), |text| {
        package_script_tasks(text, manager).map_err(|error| RepoError::Format(error.to_string()))
    });
    if host.exists(&root.join(CARGO_MANIFEST)) {
        detection.tasks.extend(cargo_tasks());
    }
    if let Some(justfile) = first_present(host, root, &JUSTFILE_NAMES) {
        detection.read(host, &justfile, |text| Ok(just_tasks(text)));
    }
    if let Some(makefile) = first_present(host, root, &MAKEFILE_NAMES) {
        detection.read(host, &makefile, |text| Ok(make_tasks(text)));
    }
    detection
}

fn first_present(host: &HostLink, root: &Path, names: &[&str]) -> Option<PathBuf> {
    names
        .iter()
        .map(|name| root.join(name))
        .find(|path| host.exists(path))
}

impl TaskDetection {
    fn read(
        &mut self,
        host: &HostLink,
        path: &Path,
        parse: impl FnOnce(&str) -> Result<Vec<ProjectTask>, RepoError>,
    ) {
        let parsed = read_text(host, path, MAX_TASK_FILE_BYTES).and_then(|file| parse(&file.text));
        match parsed {
            Ok(tasks) => self.tasks.extend(tasks),
            Err(error) if error.is_not_found() => {}
            Err(error) => self.unreadable.push((path.to_path_buf(), error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::fs;

    #[test]
    fn tasks_come_from_every_runner_the_folder_has() -> Result<(), Box<dyn Error>> {
        let dir = std::env::temp_dir().join(format!("krad-tasks-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#)?;
        fs::write(dir.join("yarn.lock"), "")?;
        fs::write(dir.join("Cargo.toml"), "[package]\n")?;
        fs::write(dir.join("justfile"), "fmt:\n    cargo fmt\n")?;
        fs::write(dir.join("Makefile"), "install:\n\tcp app /usr/bin\n")?;

        let detection = detect_tasks(&HostLink::Local, &dir);
        fs::write(dir.join("package.json"), "{ not json")?;
        let broken = detect_tasks(&HostLink::Local, &dir);
        fs::remove_dir_all(&dir)?;

        let lines: Vec<String> = detection.tasks.iter().map(ToString::to_string).collect();
        assert_eq!(
            lines,
            [
                "yarn run dev",
                "cargo build",
                "cargo test",
                "cargo run",
                "just fmt",
                "make install"
            ]
        );
        assert!(detection.unreadable.is_empty());
        assert_eq!(broken.unreadable.len(), 1);
        assert_eq!(broken.tasks.len(), 5);
        Ok(())
    }

    #[test]
    fn a_folder_without_runners_has_no_tasks() {
        let detection = detect_tasks(&HostLink::Local, Path::new("/nonexistent-krad-folder"));
        assert_eq!(detection.tasks, []);
        assert!(detection.unreadable.is_empty());
    }
}
