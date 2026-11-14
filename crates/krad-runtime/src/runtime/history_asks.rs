use krad_chat::{
    claude_conversation_file, read_claude_transcript, read_codex_transcript,
    read_engine_transcript, read_opencode_transcript, OpencodeServer, Runner,
};
use krad_domain::{AgentPreset, EnvironmentName, ResumeId};
use krad_host::HostLink;
use krad_protocol::{
    Answer, ErrorCode, HarnessSpot, HistoryAsk, HistoryCursor, HistoryPart, Reply, RequestId,
};
use krad_repo::workplace_home;
use krad_store::{acp_transcript_file, gateway_sessions_dir, read_acp_transcript};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{refused, Runtime};
use crate::environments::UNKNOWN_ENVIRONMENT;

const NO_HOME_TEXT: &str = "Cannot find the home folder Claude Code keeps conversations in";
const NO_CONVERSATION_TEXT: &str = "Cannot find the conversation file of this chat";
const NO_HISTORY_TEXT: &str = "this chat keeps no history to read";
const NOT_READY_TEXT: &str = "the chat's harness cannot be reached yet";

enum HistorySource {
    Claude {
        link: HostLink,
        root: Option<PathBuf>,
        cursor: HistoryCursor,
    },
    Gateway {
        sessions: PathBuf,
        cwd: PathBuf,
    },
    Acp {
        file: PathBuf,
    },
    Codex {
        runner: Runner,
        cwd: PathBuf,
    },
    Opencode {
        server: Arc<OpencodeServer>,
    },
}

fn read_claude(
    link: &HostLink,
    resume: &ResumeId,
    root: Option<&Path>,
    mut cursor: HistoryCursor,
) -> Result<HistoryPart, String> {
    if cursor.home.is_none() {
        cursor.home = workplace_home(link, root).map_err(|error| error.to_string())?;
    }
    let home = cursor.home.clone().ok_or_else(|| NO_HOME_TEXT.to_owned())?;
    if cursor.file.is_none() {
        cursor.file = claude_conversation_file(link, &home, resume);
    }
    let file = cursor
        .file
        .clone()
        .ok_or_else(|| NO_CONVERSATION_TEXT.to_owned())?;
    let chunk =
        read_claude_transcript(link, &file, cursor.next).map_err(|error| error.to_string())?;
    cursor.next = chunk.next;
    Ok(HistoryPart::Appended {
        entries: chunk.entries,
        cursor,
    })
}

impl HistorySource {
    #[tracing::instrument(skip_all)]
    fn read(self, resume: &ResumeId) -> Result<HistoryPart, String> {
        let whole = |entries| HistoryPart::Whole { entries };
        match self {
            Self::Claude { link, root, cursor } => {
                read_claude(&link, resume, root.as_deref(), cursor)
            }
            Self::Gateway { sessions, cwd } => read_engine_transcript(&sessions, &cwd, resume)
                .map(whole)
                .map_err(|error| error.to_string()),
            Self::Acp { file } => read_acp_transcript(&file)
                .map(whole)
                .map_err(|error| error.to_string()),
            Self::Codex { runner, cwd } => read_codex_transcript(&runner, &cwd, resume)
                .map(whole)
                .map_err(|error| error.to_string()),
            Self::Opencode { server } => read_opencode_transcript(&server, resume)
                .map(whole)
                .map_err(|error| error.to_string()),
        }
    }
}

impl Runtime {
    pub(super) fn ask_history(
        &self,
        id: RequestId,
        environment: &EnvironmentName,
        ask: HistoryAsk,
    ) -> Reply {
        let source = match self.history_source(environment, &ask) {
            Ok(source) => source,
            Err(code) => {
                let text = match code {
                    ErrorCode::NotFound => UNKNOWN_ENVIRONMENT,
                    ErrorCode::Unsupported => NO_HISTORY_TEXT,
                    _ => NOT_READY_TEXT,
                };
                return refused(code, text);
            }
        };
        let resume = ask.resume;
        self.answer_in_job(id, move || match source.read(&resume) {
            Ok(part) => Answer::History(Box::new(part)),
            Err(message) => Answer::Failed {
                code: ErrorCode::Failed,
                message,
            },
        })
    }

    fn history_source(
        &self,
        environment: &EnvironmentName,
        ask: &HistoryAsk,
    ) -> Result<HistorySource, ErrorCode> {
        let spot = HarnessSpot::at(environment.clone(), ask.root.clone());
        let spotted = self.spotted(&spot).ok_or(ErrorCode::NotFound)?;
        match ask.harness {
            AgentPreset::ClaudeCode => Ok(HistorySource::Claude {
                link: spotted.link.clone(),
                root: ask.root.clone(),
                cursor: ask.cursor.clone(),
            }),
            AgentPreset::Gateway => Ok(HistorySource::Gateway {
                sessions: gateway_sessions_dir().ok_or(ErrorCode::Unavailable)?,
                cwd: ask
                    .root
                    .clone()
                    .or_else(|| spotted.home())
                    .unwrap_or_default(),
            }),
            AgentPreset::Acp(_) => Ok(HistorySource::Acp {
                file: acp_transcript_file(environment, ask.chat).ok_or(ErrorCode::Unavailable)?,
            }),
            AgentPreset::Codex => {
                let (runner, cwd) = self
                    .spot_runner(&spotted, AgentPreset::Codex)
                    .map_err(|_| ErrorCode::Unavailable)?;
                Ok(HistorySource::Codex { runner, cwd })
            }
            AgentPreset::Opencode => Ok(HistorySource::Opencode {
                server: self
                    .opencode
                    .server(&spotted.link.host())
                    .ok_or(ErrorCode::Unavailable)?,
            }),
            AgentPreset::Shell => Err(ErrorCode::Unsupported),
        }
    }
}
