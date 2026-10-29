mod cli;
mod client;
mod env;
mod error;
mod handle;
mod hook;
mod mailbox;
mod protocol;
mod server;
mod statusline;
mod token;

pub use cli::run_cli;
pub use client::call;
pub use env::{
    guide, orchestrator_guide, terminal_env, CLI_PROGRAM, ENV_ENDPOINT, ENV_TERMINAL, ENV_TOKEN,
};
pub use error::{AgentsError, HandleError};
pub use handle::{Caller, ChatHandle, LeadHandle, TerminalHandle};
pub use mailbox::{notice_text, Mailbox};
pub use protocol::{
    lead_refusal, AgentInfo, AgentStatus, BusToken, Envelope, Message, Request, Response,
    MAX_WAIT_SECS,
};
pub use server::{serve, Endpoint, Incoming};
pub use token::new_token;
