use std::io::{Read, Write};
use std::time::Duration;

use crate::command::{HostCommand, ScriptContext};
use crate::error::HostError;
use crate::link::HostLink;
use crate::mux::{Channel, OpenRequest, Stream, TerminalSize};

const OPEN_WAIT: Duration = Duration::from_secs(30);

pub struct RemoteTerminal {
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    pub control: TerminalControl,
}

#[derive(Clone)]
pub struct TerminalControl {
    channel: Channel,
}

impl TerminalControl {
    pub fn resize(&self, cols: u16, rows: u16) {
        self.channel.resize(TerminalSize { cols, rows });
    }

    pub fn kill(&self) {
        self.channel.kill();
    }

    #[must_use]
    pub fn wait(&self) -> Option<i32> {
        self.channel.wait_exit(None).ok().flatten()
    }
}

impl HostCommand {
    /// # Errors
    ///
    /// Returns [`HostError`] when the host refuses to open the terminal or the session ends.
    pub fn open_terminal(
        &self,
        link: &HostLink,
        cols: u16,
        rows: u16,
    ) -> Result<Option<RemoteTerminal>, HostError> {
        let HostLink::Ssh(ssh) = link else {
            return Ok(None);
        };
        let Some(carrier) = ssh.multiplexed() else {
            return Ok(None);
        };
        let login_env = ssh.login_env_file();
        let script = self.script_with(ScriptContext {
            login_env: login_env.as_deref(),
        });
        let request = OpenRequest::Pty {
            script,
            size: TerminalSize { cols, rows },
        };
        let (channel, _) = carrier.open(request, OPEN_WAIT)?;
        Ok(Some(RemoteTerminal {
            reader: Box::new(channel.reader(Stream::Main)),
            writer: Box::new(channel.writer(Stream::Main)),
            control: TerminalControl { channel },
        }))
    }
}
