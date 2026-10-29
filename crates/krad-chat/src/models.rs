use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;

use crate::command::{BusVars, LiveSession};
use crate::control::{Approvals, LiveEvent, ModelOffer};
use crate::error::ChatError;
use crate::live::{new_session_id, spawn_live_chat, LiveConfig};
use crate::process::Runner;

const CATALOG_WAIT: Duration = Duration::from_secs(30);

/// # Errors
///
/// Fails when Claude Code cannot be started.
pub fn claude_models(runner: &Runner, cwd: &Path) -> Result<Vec<ModelOffer>, ChatError> {
    let config = LiveConfig {
        runner: runner.clone(),
        model: None,
        cwd: cwd.to_path_buf(),
        extra_dirs: Vec::new(),
        orchestration: None,
        system_prompt: None,
        effort: None,
        permission_mode: None,
        bus_vars: BusVars::default(),
        approvals: Approvals::Asked,
    };
    let (sender, receiver) = channel();
    let chat = spawn_live_chat(
        config,
        &LiveSession::Start(new_session_id()),
        move |event| {
            if let LiveEvent::Ready(catalog) = event {
                sender.send(catalog.models).unwrap_or_default();
            }
        },
    )?;
    let models = receiver.recv_timeout(CATALOG_WAIT).unwrap_or_default();
    chat.stop();
    Ok(models)
}
