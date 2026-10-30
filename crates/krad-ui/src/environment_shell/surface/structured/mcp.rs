use gpui_kit::{AnyElement, Context};
use krad_domain::{EnvironmentName, McpProjectFile, McpProjectTrust, McpServer, TerminalId};
use std::path::{Path, PathBuf};

use super::super::super::mcp::{McpLaunch, McpTrust};
use super::super::super::EnvironmentShell;
use super::super::{Backend, SurfaceChat};
use super::structured_mut;

const READ_FAILED_LEAD: &str = "The project's MCP file could not be read: ";

#[derive(Debug, Default)]
pub(in crate::environment_shell) enum McpPrep {
    #[default]
    Unread,
    Reading,
    Asking(McpProjectFile),
    Launching,
    Ready(McpLaunch),
}

impl EnvironmentShell {
    pub(super) fn gateway_mcp(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        folder: &Path,
        cx: &mut Context<Self>,
    ) -> Option<McpLaunch> {
        let structured = structured_mut(self, env, id)?;
        match std::mem::take(&mut structured.mcp) {
            McpPrep::Ready(launch) => Some(launch),
            McpPrep::Unread => {
                structured.mcp = McpPrep::Reading;
                Self::read_project_mcp(env.clone(), id, folder.to_path_buf(), cx);
                None
            }
            waiting @ (McpPrep::Reading | McpPrep::Asking(_) | McpPrep::Launching) => {
                structured.mcp = waiting;
                None
            }
        }
    }

    fn read_project_mcp(
        env: EnvironmentName,
        id: TerminalId,
        folder: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let read = Self::load_project_mcp(folder.clone(), cx);
        cx.spawn(async move |this, cx| {
            let file = read.await;
            this.update(cx, |shell, cx| {
                shell.project_mcp_read(&env, id, &folder, file, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn project_mcp_read(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        folder: &Path,
        file: Result<McpProjectFile, String>,
        cx: &mut Context<Self>,
    ) {
        let file = match file {
            Ok(file) => file,
            Err(error) => {
                if let Some(structured) = structured_mut(self, env, id) {
                    structured
                        .notices
                        .push(format!("{READ_FAILED_LEAD}{error}"));
                }
                return self.launch_mcp(env, id, Vec::new(), cx);
            }
        };
        match self.mcp_trust_for(folder, file) {
            McpTrust::Trusted(servers) => self.launch_mcp(env, id, servers, cx),
            McpTrust::Ignored => self.launch_mcp(env, id, Vec::new(), cx),
            McpTrust::Unknown(file) => {
                if let Some(structured) = structured_mut(self, env, id) {
                    structured.mcp = McpPrep::Asking(file);
                }
                cx.notify();
            }
        }
    }

    fn launch_mcp(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        project: Vec<McpServer>,
        cx: &mut Context<Self>,
    ) {
        let Some(structured) = structured_mut(self, env, id) else {
            return;
        };
        structured.mcp = McpPrep::Launching;
        let launch = self.mcp_launch(project, cx);
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let launch = launch.await;
            this.update(cx, |shell, cx| {
                if let Some(structured) = structured_mut(shell, &env, id) {
                    structured.mcp = McpPrep::Ready(launch);
                }
                shell.flush_structured(&env, id, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn mcp_trust_chosen(&mut self, folder: &Path, trust: McpProjectTrust, cx: &mut Context<Self>) {
        let asking: Vec<(EnvironmentName, TerminalId)> = self
            .envs
            .iter()
            .flat_map(|(env, live)| {
                live.chats
                    .iter()
                    .filter(|(_, chat)| {
                        chat.structured()
                            .is_some_and(|structured| matches!(structured.mcp, McpPrep::Asking(_)))
                    })
                    .map(move |(id, _)| (env.clone(), *id))
            })
            .filter(|(env, id)| self.chat_folder(env, *id) == folder)
            .collect();
        for (env, id) in asking {
            let Some(structured) = structured_mut(self, &env, id) else {
                continue;
            };
            let McpPrep::Asking(file) = std::mem::take(&mut structured.mcp) else {
                continue;
            };
            let servers = match trust {
                McpProjectTrust::Trusted => file.servers,
                McpProjectTrust::Ignored => Vec::new(),
            };
            self.launch_mcp(&env, id, servers, cx);
        }
    }

    pub(in crate::environment_shell::surface) fn mcp_tray(
        &self,
        id: TerminalId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let Backend::Structured(structured) = &chat.backend else {
            return None;
        };
        let McpPrep::Asking(file) = &structured.mcp else {
            return None;
        };
        let folder = self.chat_folder(&self.active, id);
        let chosen = folder.clone();
        Some(Self::mcp_trust_tray(
            &folder,
            file,
            move |this, trust, _, cx| this.mcp_trust_chosen(&chosen, trust, cx),
            cx,
        ))
    }
}
