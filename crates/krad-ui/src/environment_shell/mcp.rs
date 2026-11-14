mod tray;

use gpui_kit::Context;
use krad_domain::McpProjectTrust;
use std::path::Path;

use super::EnvironmentShell;

impl EnvironmentShell {
    pub(super) fn set_mcp_project_trust(
        &mut self,
        folder: &Path,
        trust: McpProjectTrust,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(cx, |settings| settings.mcp.set_project_trust(folder, trust));
    }

    pub(super) fn forget_mcp_project(&mut self, folder: &Path, cx: &mut Context<Self>) {
        self.edit_settings(cx, |settings| settings.mcp.forget_project(folder));
    }
}
