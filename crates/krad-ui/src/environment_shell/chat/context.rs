use krad_domain::{parse_wsl_path, wsl_linux_path, TerminalGroup};
use std::path::Path;

use super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn chat_group(&self) -> Option<&TerminalGroup> {
        self.active_group()
    }

    pub(in crate::environment_shell) fn chat_distro(&self) -> Option<String> {
        self.chat_group()
            .and_then(|group| parse_wsl_path(&group.root))
            .map(|inside| inside.distro)
    }

    pub(in crate::environment_shell) fn harness_path(&self, path: &Path) -> String {
        self.chat_distro()
            .and_then(|distro| wsl_linux_path(&distro, path))
            .unwrap_or_else(|| path.to_string_lossy().into_owned())
    }
}
