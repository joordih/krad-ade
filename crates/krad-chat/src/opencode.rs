mod ask;
mod catalog;
mod chat;
mod events;
mod history;
mod prompt;
mod server;
mod tools;
mod watch;

pub use catalog::opencode_models;
pub use chat::{new_opencode_session_id, prepare_opencode_session};
pub(crate) use chat::{spawn_opencode, OpencodeChat, Order};
pub use history::read_opencode_transcript;
pub use server::{OpencodeError, OpencodeServer};
pub use watch::watch_opencode_session;
