mod lines;
mod stream;

use krad_domain::{CloudProcessId, JournalOffset, Pairing};
use krad_host::runner::{authorize, run_authorized, stop_command, ATTACH_VERB};
use krad_host::{remote_path, HostCommand, HostError, HostLink, SecretCommand};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

pub(crate) use lines::{HarnessLines, LineOrigin};

use crate::process::Runner;

const CWD_FLAG: &str = "--cwd";
const PASS_FLAG: &str = "--pass";
const END_OF_FLAGS: &str = "--";
const PATH_VAR: &str = "PATH";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachRefusal {
    PastEnd { end: JournalOffset },
    Refused,
}

#[derive(Debug)]
pub(crate) struct CloudCursor {
    offset: AtomicU64,
    refusal: OnceLock<AttachRefusal>,
}

impl CloudCursor {
    pub(crate) const fn starting_at(from: JournalOffset) -> Self {
        Self {
            offset: AtomicU64::new(from.0),
            refusal: OnceLock::new(),
        }
    }

    pub(crate) fn handled_to(&self, offset: JournalOffset) {
        self.offset.store(offset.0, Ordering::SeqCst);
    }

    pub(crate) fn refuse(&self, refusal: AttachRefusal) {
        self.refusal.get_or_init(|| refusal);
    }

    fn consumed(&self) -> JournalOffset {
        JournalOffset(self.offset.load(Ordering::SeqCst))
    }
}

#[derive(Clone, Debug)]
pub struct CloudRun {
    pub link: HostLink,
    pub cli: PathBuf,
    pub pairing: Pairing,
    pub program: String,
    pub process: CloudProcessId,
    pub from: JournalOffset,
    cursor: Arc<CloudCursor>,
}

impl PartialEq for CloudRun {
    fn eq(&self, other: &Self) -> bool {
        self.link == other.link
            && self.cli == other.cli
            && self.pairing == other.pairing
            && self.program == other.program
            && self.process == other.process
            && self.from == other.from
            && Arc::ptr_eq(&self.cursor, &other.cursor)
    }
}

impl Eq for CloudRun {}

impl CloudRun {
    #[must_use]
    pub fn new(
        link: HostLink,
        cli: PathBuf,
        pairing: Pairing,
        program: String,
        process: CloudProcessId,
        from: JournalOffset,
    ) -> Self {
        Self {
            link,
            cli,
            pairing,
            program,
            process,
            from,
            cursor: Arc::new(CloudCursor::starting_at(from)),
        }
    }

    #[must_use]
    pub fn consumed(&self) -> JournalOffset {
        self.cursor.consumed()
    }

    #[must_use]
    pub fn refusal(&self) -> Option<AttachRefusal> {
        self.cursor.refusal.get().copied()
    }

    #[must_use]
    pub fn host_runner(&self) -> Runner {
        Runner::Ssh {
            link: self.link.clone(),
            program: self.program.clone(),
        }
    }

    pub(crate) fn cursor(&self) -> Arc<CloudCursor> {
        Arc::clone(&self.cursor)
    }

    fn attach_command(&self, cwd: &Path, args: &[String], env: &[(String, String)]) -> HostCommand {
        let mut command = HostCommand::new(&self.cli);
        command
            .args([ATTACH_VERB, self.process.as_str(), &self.from.0.to_string()])
            .args([CWD_FLAG, &remote_path(cwd)])
            .in_login_shell();
        authorize(&mut command, &self.pairing);
        for (name, value) in env {
            if name == PATH_VAR {
                command.prepend_path(value);
            } else {
                command.args([PASS_FLAG, name]).read_env_from_stdin(name);
            }
        }
        command.arg(END_OF_FLAGS).arg(&self.program).args(args);
        command
    }

    pub(crate) fn command(
        &self,
        cwd: &Path,
        args: &[String],
        env: &[(String, String)],
    ) -> Result<SecretCommand, HostError> {
        let command = self.attach_command(cwd, args, env);
        let secrets: Vec<&str> = std::iter::once(self.pairing.token.expose())
            .chain(
                env.iter()
                    .filter(|(name, _)| name != PATH_VAR)
                    .map(|(_, value)| value.as_str()),
            )
            .collect();
        self.link.secret_command(&command, &secrets)
    }
}

/// # Errors
///
/// Fails when the runner cannot be reached or refuses to stop the process.
#[tracing::instrument(skip_all, fields(process = %id))]
pub fn stop_cloud_process(
    link: &HostLink,
    cli: &Path,
    pairing: &Pairing,
    id: &CloudProcessId,
) -> Result<(), HostError> {
    let command = stop_command(cli, pairing, id);
    let output = run_authorized(link, &command, pairing)?;
    if output.status.success() {
        return Ok(());
    }
    Err(HostError::Remote {
        program: command.program_name(),
        status: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

#[cfg(test)]
mod tests;
