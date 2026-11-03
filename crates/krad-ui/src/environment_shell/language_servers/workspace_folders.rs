use std::path::{Path, PathBuf};

use super::super::{EnvironmentShell, LiveEnv};
use super::{ServerKey, ServerRoot};
use krad_domain::TerminalId;

impl EnvironmentShell {
    pub(super) fn document_folder(live: &LiveEnv, id: TerminalId, path: &Path) -> Option<PathBuf> {
        live.data
            .group_of(id)
            .map(|group| group.root.clone())
            .or_else(|| path.parent().map(Path::to_path_buf))
    }

    pub(super) fn register_folder(&mut self, key: &ServerKey, folder: PathBuf) {
        let added = self
            .language_servers
            .folders
            .entry(key.clone())
            .or_default()
            .insert(folder.clone());
        if !added {
            return;
        }
        let Some(running) = self.language_servers.running(key) else {
            return;
        };
        if let Err(error) = running
            .server
            .client
            .change_workspace_folders(&[folder], &[])
        {
            tracing::debug!(%error, "a language server did not take a new workspace folder");
        }
    }

    pub(super) fn server_folders(&self, key: &ServerKey) -> Vec<PathBuf> {
        match &key.root {
            ServerRoot::Folder(folder) => vec![folder.clone()],
            ServerRoot::Host => self
                .language_servers
                .folders
                .get(key)
                .map_or_else(Vec::new, |folders| folders.iter().cloned().collect()),
        }
    }

    pub(super) fn announce_late_folders(&self, key: &ServerKey, announced: &[PathBuf]) {
        let late: Vec<PathBuf> = self
            .server_folders(key)
            .into_iter()
            .filter(|folder| !announced.contains(folder))
            .collect();
        let Some(running) = self
            .language_servers
            .running(key)
            .filter(|_| !late.is_empty())
        else {
            return;
        };
        if let Err(error) = running.server.client.change_workspace_folders(&late, &[]) {
            tracing::debug!(%error, "a language server did not take the folders added while it started");
        }
    }

    pub(super) fn server_directory(&self, key: &ServerKey) -> Option<PathBuf> {
        self.server_folders(key).into_iter().next()
    }
}
