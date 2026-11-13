use krad_domain::{AgentPreset, McpProjectFile, McpProjectTrust, McpServer};
use krad_protocol::{AgentRef, ErrorCode, McpTrustAsk, Reply};
use std::path::PathBuf;

use super::backend::ChatLaunch;
use super::brief::ChatRole;
use super::chat::Chat;
use super::resolve::{chat_folder, in_wsl};
use super::{ChatEffect, ChatWork, Chats};
use crate::jobs::JobDone;
use crate::mcp::{code_tools_server, mcp_servers, read_project_mcp, trust_of, trusted_servers};
use crate::mcp::{with_secrets, McpLaunch, McpTrust};
use crate::runtime::refused;

const READ_FAILED_LEAD: &str = "The project's MCP file could not be read: ";
const NOTHING_ASKED: &str = "this chat is not asking about its project's MCP servers";
const JOB_FAILED: &str = "Could not prepare the MCP servers";

#[derive(Debug, Default)]
pub(super) enum McpPrep {
    #[default]
    Unread,
    Reading,
    Asking {
        folder: PathBuf,
        file: McpProjectFile,
    },
    Launching,
    Ready(McpLaunch),
}

impl McpPrep {
    pub(super) fn asked(&self) -> Option<McpTrustAsk> {
        match self {
            Self::Asking { folder, file } => Some(McpTrustAsk::new(folder.clone(), file)),
            Self::Unread | Self::Reading | Self::Launching | Self::Ready(_) => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct McpRead {
    chat: AgentRef,
    opened: u64,
    folder: PathBuf,
    file: Result<McpProjectFile, String>,
}

#[derive(Debug)]
pub(crate) struct McpLaunched {
    chat: AgentRef,
    opened: u64,
    launch: McpLaunch,
}

fn needs_mcp(chat: &Chat) -> bool {
    if chat.brief.role == ChatRole::Digest {
        return false;
    }
    match chat.harness {
        AgentPreset::Gateway => chat.prepared.is_none(),
        AgentPreset::Acp(_) => true,
        AgentPreset::ClaudeCode
        | AgentPreset::Codex
        | AgentPreset::Opencode
        | AgentPreset::Shell => false,
    }
}

fn folder_of(chat: &Chat) -> PathBuf {
    match &chat.prepared {
        Some(ChatLaunch::Acp(config)) => config.cwd.clone(),
        Some(_) | None => chat_folder(&chat.place),
    }
}

pub(super) fn ready_mcp(chat: &mut Chat, work: &ChatWork<'_>) -> Option<McpLaunch> {
    if !needs_mcp(chat) {
        return Some(McpLaunch::default());
    }
    match std::mem::take(&mut chat.mcp) {
        McpPrep::Ready(launch) => {
            chat.mcp = McpPrep::Ready(launch.clone());
            Some(launch)
        }
        McpPrep::Unread => {
            chat.mcp = McpPrep::Reading;
            let (chat_id, opened, folder) = (chat.id.clone(), chat.opened, folder_of(chat));
            let link = chat.place.link.clone();
            let started = work.jobs.run(move || {
                let file = read_project_mcp(&link, &folder);
                JobDone::McpRead(McpRead {
                    chat: chat_id,
                    opened,
                    folder,
                    file,
                })
            });
            if let Err(error) = started {
                tracing::warn!(%error, "could not read the project's MCP file");
                chat.mcp = McpPrep::Ready(McpLaunch::default());
            }
            None
        }
        waiting @ (McpPrep::Reading | McpPrep::Asking { .. } | McpPrep::Launching) => {
            chat.mcp = waiting;
            None
        }
    }
}

fn launch(chat: &mut Chat, project: Vec<McpServer>, work: &ChatWork<'_>) {
    chat.mcp = McpPrep::Launching;
    let tools = work
        .facts
        .tools_offered()
        .then(|| {
            work.facts
                .bus_vars(&chat.id, &chat.place, in_wsl(&chat.place))
        })
        .and_then(code_tools_server);
    let project = project.into_iter().chain(tools).collect();
    let servers = mcp_servers(
        &work.facts.settings.mcp,
        chat.brief.mcp_servers.as_deref(),
        project,
    );
    let (chat_id, opened) = (chat.id.clone(), chat.opened);
    let secrets = work.facts.secrets.clone();
    let started = work.jobs.run(move || {
        JobDone::McpLaunched(McpLaunched {
            chat: chat_id,
            opened,
            launch: with_secrets(servers, &secrets),
        })
    });
    if let Err(error) = started {
        chat.mcp = McpPrep::Ready(McpLaunch {
            starts: Vec::new(),
            problems: vec![format!("{JOB_FAILED}: {error}")],
        });
    }
}

impl Chats {
    pub(crate) fn mcp_read(&mut self, read: McpRead, work: &mut ChatWork<'_>) {
        let Some(chat) = self
            .open
            .get_mut(&read.chat)
            .filter(|chat| chat.opened == read.opened && matches!(chat.mcp, McpPrep::Reading))
        else {
            return;
        };
        match read.file {
            Err(error) => {
                chat.start_notices
                    .push(format!("{READ_FAILED_LEAD}{error}"));
                launch(chat, Vec::new(), work);
            }
            Ok(file) => {
                let stored = work.facts.settings.mcp.project_trust(&read.folder);
                match trust_of(stored, file) {
                    McpTrust::Trusted(servers) => launch(chat, servers, work),
                    McpTrust::Ignored => launch(chat, Vec::new(), work),
                    McpTrust::Unknown(file) => {
                        chat.mcp = McpPrep::Asking {
                            folder: read.folder,
                            file,
                        };
                    }
                }
            }
        }
        self.flush(&read.chat, work);
    }

    pub(crate) fn mcp_launched(&mut self, launched: McpLaunched, work: &mut ChatWork<'_>) {
        let Some(chat) = self.open.get_mut(&launched.chat).filter(|chat| {
            chat.opened == launched.opened && matches!(chat.mcp, McpPrep::Launching)
        }) else {
            return;
        };
        chat.mcp = McpPrep::Ready(launched.launch);
        self.flush(&launched.chat, work);
    }

    pub(super) fn trust_mcp(
        &mut self,
        id: &AgentRef,
        trust: McpProjectTrust,
        work: &mut ChatWork<'_>,
    ) -> Reply {
        let Some(McpPrep::Asking { folder, .. }) = self.open.get(id).map(|chat| &chat.mcp) else {
            return refused(ErrorCode::AlreadyAnswered, NOTHING_ASKED);
        };
        let folder = folder.clone();
        work.effects.push(ChatEffect::McpTrust {
            folder: folder.clone(),
            trust,
        });
        let asking: Vec<AgentRef> = self
            .open
            .values()
            .filter(|chat| matches!(&chat.mcp, McpPrep::Asking { folder: asked, .. } if *asked == folder))
            .map(|chat| chat.id.clone())
            .collect();
        for other in asking {
            let Some(chat) = self.open.get_mut(&other) else {
                continue;
            };
            let McpPrep::Asking { file, .. } = std::mem::take(&mut chat.mcp) else {
                continue;
            };
            launch(chat, trusted_servers(file, trust), work);
            if other != *id {
                self.flush(&other, work);
            }
        }
        Reply::Accepted
    }
}
