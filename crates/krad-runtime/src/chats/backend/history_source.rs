use krad_chat::{ChatBackend, Runner};
use krad_domain::AgentPreset;
use krad_host::HostLink;
use krad_store::gateway_sessions_dir;
use std::path::PathBuf;
use std::sync::Arc;

use super::super::chat::Chat;
use super::super::resolve::{chat_folder, LaunchFacts};
use super::ChatLaunch;

pub(in crate::chats) enum HistorySource {
    Claude {
        link: HostLink,
        root: Option<PathBuf>,
    },
    Codex {
        runner: Runner,
        cwd: PathBuf,
    },
    Gateway {
        sessions: PathBuf,
        cwd: PathBuf,
    },
    OpencodeOn(Runner, PathBuf),
    Opencode(Arc<krad_chat::OpencodeServer>),
    Acp(PathBuf),
    Unread,
}

impl HistorySource {
    pub(in crate::chats) fn of(chat: &Chat, facts: &LaunchFacts<'_>) -> Self {
        let Some(launch) = &chat.prepared else {
            return Self::briefed(chat, facts);
        };
        let config = match launch {
            ChatLaunch::Gateway(config) => &**config,
            ChatLaunch::Opencode(config) => {
                return Self::OpencodeOn(config.runner.clone(), config.cwd.clone())
            }
            ChatLaunch::Acp(_) => {
                return krad_store::acp_transcript_file(&chat.id.environment, chat.id.terminal)
                    .map_or(Self::Unread, Self::Acp);
            }
            ChatLaunch::Backend(backend) => match &**backend {
                ChatBackend::Claude(_) => return Self::claude(chat),
                ChatBackend::Codex(config) => {
                    return Self::Codex {
                        runner: config.runner.clone(),
                        cwd: config.cwd.clone(),
                    }
                }
                ChatBackend::Gateway(_, config) => config,
                ChatBackend::Opencode(server, _) => return Self::Opencode(Arc::clone(server)),
                ChatBackend::Acp(_) => return Self::Unread,
            },
        };
        Self::Gateway {
            sessions: config.sessions.clone(),
            cwd: config.cwd.clone(),
        }
    }

    fn claude(chat: &Chat) -> Self {
        Self::Claude {
            link: chat.place.link.clone(),
            root: chat.place.root.clone(),
        }
    }

    fn briefed(chat: &Chat, facts: &LaunchFacts<'_>) -> Self {
        let runner = || facts.history_runner(&chat.id, chat.harness, &chat.place);
        match chat.harness {
            AgentPreset::ClaudeCode => Self::claude(chat),
            AgentPreset::Codex => {
                runner().map_or(Self::Unread, |(runner, cwd)| Self::Codex { runner, cwd })
            }
            AgentPreset::Opencode => {
                runner().map_or(Self::Unread, |(runner, cwd)| Self::OpencodeOn(runner, cwd))
            }
            AgentPreset::Gateway => {
                gateway_sessions_dir().map_or(Self::Unread, |sessions| Self::Gateway {
                    sessions,
                    cwd: chat_folder(&chat.place),
                })
            }
            AgentPreset::Acp(_) => {
                krad_store::acp_transcript_file(&chat.id.environment, chat.id.terminal)
                    .map_or(Self::Unread, Self::Acp)
            }
            AgentPreset::Shell => Self::Unread,
        }
    }
}
