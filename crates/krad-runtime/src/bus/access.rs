use krad_agents::{
    serve, AgentsError, Endpoint, Incoming, RequestSink, SinkClosed, Timestamp, CLI_PROGRAM,
};
use krad_domain::claude_hook_settings;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;

use crate::input::Input;

const CLAUDE_SETTINGS_FILE: &str = "claude-hooks.json";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BusSetup {
    pub cli_dir: Option<PathBuf>,
    pub config_dir: Option<PathBuf>,
}

#[derive(Debug)]
pub struct BusAccess {
    pub endpoint: Endpoint,
    pub cli_dir: Option<PathBuf>,
    pub claude_settings: Option<PathBuf>,
    pub(crate) config_dir: Option<PathBuf>,
    pub(crate) started_at: Timestamp,
}

#[derive(Clone)]
struct RuntimeSink {
    inbox: SyncSender<Input>,
    closed: Arc<AtomicBool>,
}

impl RequestSink for RuntimeSink {
    fn deliver(&self, incoming: Incoming) -> Result<(), SinkClosed> {
        if self.is_closed() {
            return Err(SinkClosed);
        }
        self.inbox
            .send(Input::Bus(incoming))
            .map_err(|_| SinkClosed)
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

#[tracing::instrument(skip_all)]
pub(crate) fn open(
    setup: BusSetup,
    inbox: SyncSender<Input>,
    closed: Arc<AtomicBool>,
) -> Result<BusAccess, AgentsError> {
    let endpoint = serve(RuntimeSink { inbox, closed })?;
    let claude_settings = setup.config_dir.as_deref().and_then(write_claude_settings);
    tracing::info!(address = %endpoint.address, "agent bus serving");
    Ok(BusAccess {
        endpoint,
        cli_dir: setup.cli_dir,
        claude_settings,
        config_dir: setup.config_dir,
        started_at: Timestamp::now(),
    })
}

fn write_claude_settings(config_dir: &Path) -> Option<PathBuf> {
    let path = config_dir.join(CLAUDE_SETTINGS_FILE);
    let written = std::fs::create_dir_all(config_dir)
        .and_then(|()| std::fs::write(&path, claude_hook_settings(CLI_PROGRAM)));
    match written {
        Ok(()) => Some(path),
        Err(error) => {
            tracing::warn!(%error, "Claude Code hook settings not written");
            None
        }
    }
}
