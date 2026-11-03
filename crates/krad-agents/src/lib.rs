mod cli;
mod client;
mod code;
mod env;
mod error;
mod handle;
mod hook;
mod host_verbs;
mod mailbox;
mod mcp;
mod protocol;
mod server;
mod statusline;
mod token;
mod toolchains;

pub use cli::{run_cli, run_invoked};
pub use client::call;
pub use code::{CodeAnswer, CodeError, CodeLine, CodePosition, CodeRequest, CodeTool};
pub use env::{
    guide, member_token, orchestrator_guide, terminal_env, token_file_env, CLI_PROGRAM,
    ENV_ENDPOINT, ENV_TERMINAL, ENV_TOKEN, ENV_TOKEN_FILE,
};
pub use error::{AgentsError, HandleError};
pub use handle::{Caller, LeadHandle, TerminalHandle};
pub use mailbox::{notice_text, Mailbox};
pub use protocol::{
    lead_refusal, AgentInfo, AgentStatus, BusToken, Envelope, Message, Request, Response,
    TerminalState, MAX_WAIT,
};
pub use server::{serve, Endpoint, Incoming};
pub use token::new_token;
