#![cfg(unix)]

mod assets;
mod config;
mod cookie;
mod guard;
mod http;
mod listener;
mod reach;
mod routes;
mod socket;
mod tls;
mod verb;

pub use config::{
    read_web_access, turn_off_web_access, web_access_file, write_web_access, TlsFiles, WebAccess,
    WebAccessError,
};
pub use listener::{bind_web_access, start_web_access, web_transport, WebListener};
pub use reach::{is_loopback, is_private_network};
pub use socket::WebStream;
pub use verb::{run_web_access_verb, WEB_ACCESS_VERB};
