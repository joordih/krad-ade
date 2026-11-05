use krad_domain::{AcpAuthMethod, AcpSignIn};
use std::time::Duration;

use super::choices::Choices;
use super::servers::handed_servers;
use super::Launch;
use crate::acp::connection::{AcpConnection, AcpError};
use crate::acp::ids::SessionId;
use crate::acp::rpc::ErrorCode;
use crate::acp::setup::{
    AgentCapabilities, AuthCapabilities, AuthMethod, ClientCapabilities, FileSystemCapabilities,
    Implementation, InitializeRequest, LoadSessionRequest, NewSessionRequest, ProtocolVersion,
};
use crate::live::LiveSession;
use crate::refused::ResumeRefused;

pub(super) const SETUP_TIMEOUT: Duration = Duration::from_mins(2);
const CLIENT_NAME: &str = "krad-ade";
const CLIENT_TITLE: &str = "Krad ADE";
const CANNOT_LOAD_TEXT: &str =
    "cannot reopen its saved sessions, so this message starts a new one; the earlier messages stay above";
const CANNOT_FORK_TEXT: &str = "cannot fork or cut its sessions, so this message starts a new one";
const LOAD_REFUSED_TEXT: &str = "could not reopen the saved session";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Opening {
    New,
    Load(SessionId),
}

#[derive(Debug)]
pub(super) struct Opened {
    pub(super) id: SessionId,
    pub(super) choices: Choices,
    pub(super) loaded: bool,
    pub(super) skipped_servers: Vec<String>,
}

#[derive(Debug)]
pub(super) enum OpenFailure {
    SignIn,
    Refused(ResumeRefused),
    Failed(AcpError),
}

pub(super) fn initialize_request() -> InitializeRequest {
    InitializeRequest {
        protocol_version: ProtocolVersion::V1,
        client_capabilities: ClientCapabilities {
            fs: FileSystemCapabilities {
                read_text_file: true,
                write_text_file: true,
            },
            terminal: true,
            auth: AuthCapabilities { terminal: true },
        },
        client_info: Some(Implementation {
            name: CLIENT_NAME.to_owned(),
            title: Some(CLIENT_TITLE.to_owned()),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }),
    }
}

pub(super) fn opening_for(
    session: &LiveSession,
    agent: &str,
    capabilities: AgentCapabilities,
) -> Result<Opening, ResumeRefused> {
    match session {
        LiveSession::Start(_) => Ok(Opening::New),
        LiveSession::Resume(id) if capabilities.load_session => {
            Ok(Opening::Load(SessionId(id.as_str().to_owned())))
        }
        LiveSession::Resume(_) => Err(ResumeRefused::MissingConversation(format!(
            "{agent} {CANNOT_LOAD_TEXT}"
        ))),
        LiveSession::Fork(_) | LiveSession::Cut(_) => Err(ResumeRefused::MissingConversation(
            format!("{agent} {CANNOT_FORK_TEXT}"),
        )),
    }
}

fn failure(error: AcpError, opening: &Opening, agent: &str) -> OpenFailure {
    match error {
        AcpError::Refused(refusal) if refusal.code == ErrorCode::AUTH_REQUIRED => {
            OpenFailure::SignIn
        }
        AcpError::Refused(refusal) if matches!(opening, Opening::Load(_)) => OpenFailure::Refused(
            ResumeRefused::MissingConversation(format!("{agent} {LOAD_REFUSED_TEXT}: {refusal}")),
        ),
        error => OpenFailure::Failed(error),
    }
}

pub(super) fn open(
    connection: &AcpConnection,
    launch: &Launch,
    capabilities: AgentCapabilities,
    opening: &Opening,
) -> Result<Opened, OpenFailure> {
    let handed = handed_servers(&launch.mcp, capabilities.mcp_capabilities);
    let opened = match opening {
        Opening::New => connection
            .new_session(&NewSessionRequest {
                cwd: launch.paths.agent_cwd().to_path_buf(),
                mcp_servers: handed.servers,
            })
            .wait_timeout(SETUP_TIMEOUT)
            .map(|started| Opened {
                choices: Choices::reported(started.modes, started.models, &started.config_options),
                id: started.session_id,
                loaded: false,
                skipped_servers: Vec::new(),
            }),
        Opening::Load(id) => connection
            .load_session(&LoadSessionRequest {
                session_id: id.clone(),
                cwd: launch.paths.agent_cwd().to_path_buf(),
                mcp_servers: handed.servers,
            })
            .wait_timeout(SETUP_TIMEOUT)
            .map(|loaded| Opened {
                choices: Choices::reported(loaded.modes, loaded.models, &loaded.config_options),
                id: id.clone(),
                loaded: true,
                skipped_servers: Vec::new(),
            }),
    };
    opened
        .map(|opened| Opened {
            skipped_servers: handed.skipped,
            ..opened
        })
        .map_err(|error| failure(error, opening, &launch.agent))
}

pub(super) fn auth_methods(methods: &[AuthMethod]) -> Vec<AcpAuthMethod> {
    methods
        .iter()
        .map(|method| match method {
            AuthMethod::Agent(method) => AcpAuthMethod {
                id: method.id.clone(),
                name: method.name.clone(),
                description: method.description.clone(),
                sign_in: AcpSignIn::ByAgent,
            },
            AuthMethod::Terminal(method) => AcpAuthMethod {
                id: method.id.clone(),
                name: method.name.clone(),
                description: method.description.clone(),
                sign_in: AcpSignIn::InTerminal {
                    args: method.args.clone(),
                    env: method.env.clone(),
                },
            },
        })
        .collect()
}
