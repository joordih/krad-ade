use futures::channel::mpsc;
use krad_domain::ServerLaunch;
use krad_extension_api::protocol::{
    Empty, Exit, Initialize, InitializeParams, InitializeResult, Initialized,
};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::program::connection::{ProgramConnection, ProgramEvent, REQUEST_TIMEOUT};
use crate::program::error::ProgramError;

pub const MAX_LOG_LINES: usize = 500;
const STDERR_THREAD: &str = "krad-extension-stderr";

#[derive(Debug, Clone, Default)]
pub struct ProgramLog(Arc<Mutex<VecDeque<String>>>);

impl ProgramLog {
    fn push(&self, line: String) {
        let mut lines = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if lines.len() == MAX_LOG_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }
}

pub struct ExtensionProcess {
    pub connection: ProgramConnection,
    pub log: ProgramLog,
    child: Child,
    started: Instant,
}

impl ExtensionProcess {
    /// # Errors
    ///
    /// Returns [`ProgramError::Spawn`] when the program cannot be started.
    #[tracing::instrument(skip_all, fields(program = %launch.program.display()))]
    pub fn start(
        launch: &ServerLaunch,
        cwd: &Path,
        log: ProgramLog,
    ) -> Result<(Self, mpsc::Receiver<ProgramEvent>), ProgramError> {
        let mut child = Command::new(&launch.program)
            .args(&launch.args)
            .envs(launch.env.iter().map(|(name, value)| (name, value)))
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(ProgramError::Spawn)?;
        let (Some(input), Some(output), Some(errors)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(ProgramError::Stopped);
        };
        let kept = log.clone();
        thread::Builder::new()
            .name(STDERR_THREAD.to_owned())
            .spawn(move || keep_lines(errors, &kept))
            .map_err(ProgramError::Spawn)?;
        let (connection, events) =
            ProgramConnection::over(BufReader::new(output), input, REQUEST_TIMEOUT)?;
        tracing::info!("extension program started");
        let process = Self {
            connection,
            log,
            child,
            started: Instant::now(),
        };
        Ok((process, events))
    }

    #[must_use]
    pub fn ran_for(&self) -> Duration {
        self.started.elapsed()
    }
}

/// # Errors
///
/// Returns [`ProgramError`] when the program refuses or does not answer `initialize`.
pub async fn initialize(
    connection: &ProgramConnection,
    params: &InitializeParams,
) -> Result<InitializeResult, ProgramError> {
    let result = connection.request::<Initialize>(params).await?;
    connection.notify::<Initialized>(&Empty {})?;
    Ok(result)
}

impl Drop for ExtensionProcess {
    fn drop(&mut self) {
        self.connection
            .notify::<Exit>(&Empty {})
            .unwrap_or_default();
        self.child.kill().unwrap_or_default();
        if let Err(error) = self.child.wait() {
            tracing::debug!(%error, "extension program could not be reaped");
        }
    }
}

fn keep_lines(errors: impl Read, log: &ProgramLog) {
    for line in BufReader::new(errors).lines() {
        match line {
            Ok(line) => log.push(line),
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_keeps_only_its_newest_lines() {
        let log = ProgramLog::default();
        for line in 0..=MAX_LOG_LINES {
            log.push(line.to_string());
        }
        let lines = log.lines();
        assert_eq!(lines.len(), MAX_LOG_LINES);
        assert_eq!(lines.first().map(String::as_str), Some("1"));
    }
}
