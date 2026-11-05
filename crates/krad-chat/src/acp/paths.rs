use krad_domain::wsl_windows_path;
use krad_host::HostLink;
use std::path::{Path, PathBuf};

use crate::process::Runner;

const SHOWN_SEPARATOR: &str = "/";

#[derive(Clone, Debug, PartialEq, Eq)]
enum Place {
    Host,
    Wsl { distro: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPaths {
    place: Place,
    agent_cwd: PathBuf,
    app_cwd: PathBuf,
}

impl AgentPaths {
    #[must_use]
    pub fn on_host(cwd: PathBuf) -> Self {
        Self {
            place: Place::Host,
            agent_cwd: cwd.clone(),
            app_cwd: cwd,
        }
    }

    #[must_use]
    pub fn of(runner: &Runner, app_cwd: &Path) -> Self {
        let agent_cwd = PathBuf::from(runner.served_directory(app_cwd));
        let place = match runner {
            Runner::Wsl { distro, .. } => Place::Wsl {
                distro: distro.clone(),
            },
            Runner::Native(_) | Runner::Ssh { .. } => Place::Host,
        };
        Self {
            place,
            agent_cwd,
            app_cwd: app_cwd.to_path_buf(),
        }
    }

    pub(crate) fn agent_cwd(&self) -> &Path {
        &self.agent_cwd
    }

    pub(crate) fn wsl_distro(&self) -> Option<&str> {
        match &self.place {
            Place::Host => None,
            Place::Wsl { distro } => Some(distro),
        }
    }

    pub(crate) fn is_absolute(&self, host: &HostLink, path: &Path) -> bool {
        match self.place {
            Place::Host => path.is_absolute() || (!host.is_local() && path.has_root()),
            Place::Wsl { .. } => path.has_root(),
        }
    }

    pub(crate) fn opened(&self, path: &Path) -> PathBuf {
        let Place::Wsl { distro } = &self.place else {
            return path.to_path_buf();
        };
        match path.strip_prefix(&self.agent_cwd) {
            Ok(inside) => inside
                .components()
                .fold(self.app_cwd.clone(), |opened, part| opened.join(part)),
            Err(_) => wsl_windows_path(distro, &path.to_string_lossy()),
        }
    }

    pub(crate) fn shown(&self, path: &Path) -> String {
        match path.strip_prefix(&self.agent_cwd) {
            Ok(inside) if !inside.as_os_str().is_empty() => inside
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join(SHOWN_SEPARATOR),
            _ => self.opened(path).to_string_lossy().into_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_wsl() -> AgentPaths {
        AgentPaths::of(
            &Runner::Wsl {
                distro: "Ubuntu".to_owned(),
                program: "opencode".to_owned(),
            },
            Path::new(r"\\wsl$\Ubuntu\home\dev\api"),
        )
    }

    #[test]
    fn a_wsl_agent_s_paths_open_under_the_group_s_own_folder() {
        let paths = in_wsl();
        assert_eq!(paths.agent_cwd(), Path::new("/home/dev/api"));
        assert_eq!(
            paths.opened(Path::new("/home/dev/api/src/lib.rs")),
            Path::new(r"\\wsl$\Ubuntu\home\dev\api")
                .join("src")
                .join("lib.rs")
        );
        assert_eq!(
            paths.opened(Path::new("/etc/hosts")),
            PathBuf::from(r"\\wsl.localhost\Ubuntu\etc\hosts")
        );
        assert_eq!(
            paths.opened(Path::new("/mnt/c/Users/dev/spec.md")),
            PathBuf::from(r"C:\Users\dev\spec.md")
        );
    }

    #[test]
    fn a_wsl_agent_s_paths_show_relative_to_its_folder() {
        let paths = in_wsl();
        assert_eq!(
            paths.shown(Path::new("/home/dev/api/src/lib.rs")),
            "src/lib.rs"
        );
        assert_eq!(
            paths.shown(Path::new("/mnt/c/Users/dev/spec.md")),
            r"C:\Users\dev\spec.md"
        );
        assert!(paths.is_absolute(&HostLink::Local, Path::new("/home/dev/api")));
        assert!(!paths.is_absolute(&HostLink::Local, Path::new("src/lib.rs")));
        assert_eq!(paths.wsl_distro(), Some("Ubuntu"));
    }

    #[test]
    fn an_agent_on_its_host_opens_the_paths_it_names() {
        let paths = AgentPaths::on_host(PathBuf::from("/w"));
        assert_eq!(paths.opened(Path::new("/w/a.rs")), Path::new("/w/a.rs"));
        assert_eq!(paths.shown(Path::new("/w/src/a.rs")), "src/a.rs");
        assert_eq!(paths.shown(Path::new("/elsewhere/b.rs")), "/elsewhere/b.rs");
        assert_eq!(paths.wsl_distro(), None);
    }
}
