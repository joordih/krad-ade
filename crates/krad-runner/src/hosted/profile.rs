use krad_domain::{EnvironmentData, EnvironmentName, Pairing, RuntimeState};
use krad_runtime::{
    BusSetup, EnvironmentsSetup, EventsSetup, HostTools, KraddiesSetup, OwnRunner, RuntimeSetup,
    SettingsSetup,
};
use krad_store::{
    open_layout_in, read_settings_in, settings_file_in, ChatImages, LayoutFile, LayoutOpening,
    OpenedLayout, Secrets,
};
use std::path::{Path, PathBuf};

const RUNTIME_DIR: &str = "runtime";
const EVENTS_DIR: &str = "events";
pub const RUNNER_ENVIRONMENT: &str = "runner";

pub struct OwnDaemon {
    pub cli: PathBuf,
    pub pairing: Pairing,
}

pub struct RunnerProfile {
    pub root: PathBuf,
    pub secrets: Secrets,
    pub cli_dir: Option<PathBuf>,
    pub daemon: Option<OwnDaemon>,
}

fn with_one_local_environment(mut state: RuntimeState) -> RuntimeState {
    if state.environments.is_empty() {
        let name = EnvironmentName::new(RUNNER_ENVIRONMENT.to_owned());
        state.environments.push(EnvironmentData::empty(name));
    }
    state
}

fn opened_state(dir: &Path) -> (RuntimeState, Option<PathBuf>) {
    let OpenedLayout { layout, opening } = open_layout_in(dir);
    let file = match opening {
        LayoutOpening::StateUnreadable { error } => {
            tracing::error!(%error, "the runner's state is unreadable; it will not be written");
            None
        }
        _ => Some(LayoutFile::State.path_in(dir)),
    };
    (with_one_local_environment(layout.state), file)
}

impl RunnerProfile {
    #[must_use]
    pub fn setup(self) -> RuntimeSetup {
        let dir = self.root.join(RUNTIME_DIR);
        let (state, file) = opened_state(&dir);
        let own_runner = self
            .daemon
            .zip(state.environments.first())
            .map(|(daemon, first)| OwnRunner {
                cli: daemon.cli,
                pairing: daemon.pairing,
                environment: first.name.clone(),
            });
        RuntimeSetup {
            bus: Some(BusSetup {
                cli_dir: self.cli_dir,
                config_dir: Some(dir.clone()),
            }),
            events: Some(EventsSetup {
                journal: Some(dir.join(EVENTS_DIR)),
            }),
            environments: EnvironmentsSetup { state, file },
            settings: SettingsSetup {
                settings: read_settings_in(&dir),
                file: Some(settings_file_in(&dir)),
            },
            kraddies: KraddiesSetup::default(),
            secrets: self.secrets,
            chat_images: Some(ChatImages::new(dir)),
            host_tools: HostTools::default(),
            own_runner,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_runner_runs_one_local_environment_kept_in_its_own_folder() {
        let root = std::env::temp_dir().join(format!("krad-profile-{}", std::process::id()));
        let setup = RunnerProfile {
            root: root.clone(),
            secrets: Secrets::in_memory(),
            cli_dir: None,
            daemon: None,
        }
        .setup();
        let names: Vec<&str> = setup
            .environments
            .state
            .environments
            .iter()
            .map(|environment| environment.name.as_str())
            .collect();
        assert_eq!(names, [RUNNER_ENVIRONMENT]);
        assert!(setup.chat_images.is_some());
        assert!(setup
            .environments
            .state
            .environments
            .iter()
            .all(|environment| environment.host.is_local()));
        assert_eq!(
            setup.environments.file,
            Some(LayoutFile::State.path_in(&root.join(RUNTIME_DIR)))
        );
        std::fs::remove_dir_all(root).ok();
    }
}
