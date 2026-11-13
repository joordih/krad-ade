mod tray;

use gpui_kit::{AppContext as _, Context, Task};
use krad_domain::{
    EnvironmentName, McpProjectFile, McpProjectTrust, McpServer, McpServerName, TerminalId,
};
use krad_host::HostLink;
use krad_runtime::{mcp_servers, read_project_mcp, trust_of, with_secrets, McpLaunch, McpTrust};
use std::path::{Path, PathBuf};

use super::EnvironmentShell;

impl EnvironmentShell {
    pub(super) fn load_project_mcp(
        link: HostLink,
        folder: PathBuf,
        cx: &mut Context<Self>,
    ) -> Task<Result<McpProjectFile, String>> {
        cx.background_spawn(async move { read_project_mcp(&link, &folder) })
    }

    pub(super) fn mcp_trust_for(&self, folder: &Path, file: McpProjectFile) -> McpTrust {
        trust_of(self.settings.mcp.project_trust(folder), file)
    }

    pub(super) fn set_mcp_project_trust(
        &mut self,
        folder: &Path,
        trust: McpProjectTrust,
        cx: &mut Context<Self>,
    ) {
        self.settings.mcp.set_project_trust(folder, trust);
        self.save_settings(cx);
        cx.notify();
    }

    pub(super) fn forget_mcp_project(&mut self, folder: &Path, cx: &mut Context<Self>) {
        self.settings.mcp.forget_project(folder);
        self.save_settings(cx);
        cx.notify();
    }

    pub(super) fn code_tools_server(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<McpServer> {
        if !self.agent_tools_offered() {
            return None;
        }
        krad_runtime::code_tools_server(self.bus_vars(env, id))
    }

    pub(super) fn mcp_launch(
        &self,
        project: Vec<McpServer>,
        chosen: Option<&[McpServerName]>,
        cx: &mut Context<Self>,
    ) -> Task<McpLaunch> {
        let servers = mcp_servers(&self.settings.mcp, chosen, project);
        cx.background_spawn(async move { with_secrets(servers) })
    }
}
