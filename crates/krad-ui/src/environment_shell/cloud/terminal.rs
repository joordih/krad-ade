use krad_domain::{CloudProcessId, JournalOffset};
use krad_host::runner::{
    attach_terminal_command, resize_command, run_authorized, JournalHeader, TerminalAttach,
};
use krad_host::{HostError, SecretCommand};
use krad_session::{
    GridSize, PtyError, PtyKill, PtyOpener, PtyResize, PtyWait, SharedKiller, SharedMaster,
    SpawnedPty, TERMINAL_ENV,
};
use std::io::{self, BufRead as _, BufReader, Read as _, Write as _};
use std::process::{Child, ChildStderr, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use super::CloudSite;

const SHELL_PROGRAM: &str = "sh";
const LOGIN_SHELL_ARGS: [&str; 2] = ["-c", "exec \"${SHELL:-/bin/sh}\" -l"];
const SHELL_VAR: &str = "SHELL";
const PATH_VAR: &str = "PATH";
const EXIT_POLL: Duration = Duration::from_millis(150);
const MAX_STDERR_BYTES: u64 = 4096;
const REFUSED: &str = "The cloud runner did not open the terminal";
const NO_PIPE: &str = "the attach process gave no pipe";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct CloudProgram {
    pub(in crate::environment_shell) program: String,
    pub(in crate::environment_shell) args: Vec<String>,
    pub(in crate::environment_shell) vars: Vec<(String, String)>,
}

impl CloudProgram {
    pub(in crate::environment_shell) fn login_shell() -> Self {
        Self {
            program: SHELL_PROGRAM.to_owned(),
            args: LOGIN_SHELL_ARGS.map(str::to_owned).to_vec(),
            vars: Vec::new(),
        }
    }

    fn passed(&self) -> Vec<&str> {
        TERMINAL_ENV
            .iter()
            .map(|(name, _)| *name)
            .chain([SHELL_VAR])
            .chain(
                self.vars
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .filter(|name| *name != PATH_VAR),
            )
            .collect()
    }
}

pub(in crate::environment_shell) struct CloudTerminal {
    pub(in crate::environment_shell) site: CloudSite,
    pub(in crate::environment_shell) process: CloudProcessId,
    pub(in crate::environment_shell) program: CloudProgram,
}

struct CloudResize {
    site: CloudSite,
    process: CloudProcessId,
}

struct CloudWait(Arc<Mutex<Child>>);
struct CloudKill(Arc<Mutex<Child>>);

fn host_failure(error: HostError) -> PtyError {
    PtyError::Spawn(Box::new(error))
}

fn no_pipe() -> PtyError {
    PtyError::Io(io::Error::other(NO_PIPE))
}

impl PtyResize for CloudResize {
    fn resize(&self, size: GridSize) -> Result<(), PtyError> {
        let command = resize_command(
            &self.site.cli,
            &self.site.pairing,
            &self.process,
            size.cols,
            size.rows,
        );
        run_authorized(&self.site.link, &command, &self.site.pairing).map_err(host_failure)?;
        Ok(())
    }
}

impl PtyWait for CloudWait {
    fn wait(&mut self) -> Option<i32> {
        loop {
            let polled = self
                .0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .try_wait();
            match polled {
                Ok(Some(status)) => return status.code(),
                Ok(None) => std::thread::sleep(EXIT_POLL),
                Err(error) => {
                    tracing::debug!(%error, "the cloud terminal's attach could not be awaited");
                    return None;
                }
            }
        }
    }
}

impl PtyKill for CloudKill {
    fn kill(&mut self) {
        let killed = self.0.lock().unwrap_or_else(PoisonError::into_inner).kill();
        if let Err(error) = killed {
            tracing::debug!(%error, "the cloud terminal was already detached");
        }
    }
}

fn drain(stderr: ChildStderr) -> JoinHandle<String> {
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Err(error) = stderr.take(MAX_STDERR_BYTES).read_to_string(&mut text) {
            tracing::debug!(%error, "the cloud terminal's errors could not be read");
        }
        text
    })
}

fn refusal(errors: JoinHandle<String>) -> PtyError {
    let said = errors.join().unwrap_or_default();
    let said = said.trim();
    let text = if said.is_empty() {
        REFUSED.to_owned()
    } else {
        format!("{REFUSED}: {said}")
    };
    PtyError::Io(io::Error::other(text))
}

impl CloudTerminal {
    pub(in crate::environment_shell) fn opener(self) -> PtyOpener {
        Box::new(move |cols, rows| self.open(cols, rows))
    }

    #[tracing::instrument(skip_all, fields(process = %self.process))]
    fn open(&self, cols: u16, rows: u16) -> Result<SpawnedPty, PtyError> {
        let passed = self.program.passed();
        let attach = TerminalAttach {
            id: &self.process,
            from: JournalOffset::default(),
            cwd: &self.site.placement.worktree,
            cols,
            rows,
            passed: &passed,
            program: &self.program.program,
            args: &self.program.args,
        };
        let mut command = attach_terminal_command(&self.site.cli, &self.site.pairing, &attach);
        for (name, value) in TERMINAL_ENV {
            command.env(name, value);
        }
        let mut secrets = vec![self.site.pairing.token.expose()];
        for (name, value) in &self.program.vars {
            if name == PATH_VAR {
                command.prepend_path(value);
            } else {
                command.read_env_from_stdin(name);
                secrets.push(value);
            }
        }
        let SecretCommand { mut command, input } = self
            .site
            .link
            .secret_command(&command, &secrets)
            .map_err(host_failure)?;
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let pid = child.id();
        let (Some(mut stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(no_pipe());
        };
        let errors = drain(stderr);
        if let Some(input) = input {
            stdin.write_all(input.as_bytes())?;
            stdin.flush()?;
        }
        let mut reader = BufReader::new(stdout);
        let mut header = String::new();
        reader.read_line(&mut header)?;
        if header.parse::<JournalHeader>().is_err() {
            if let Err(error) = child.kill() {
                tracing::debug!(%error, "the refused attach had already ended");
            }
            return Err(refusal(errors));
        }
        let child = Arc::new(Mutex::new(child));
        let resize = CloudResize {
            site: self.site.clone(),
            process: self.process.clone(),
        };
        Ok(SpawnedPty {
            reader: Box::new(reader),
            writer: Box::new(stdin),
            master: SharedMaster::new(Box::new(resize)),
            killer: SharedKiller::new(Box::new(CloudKill(Arc::clone(&child)))),
            child: Box::new(CloudWait(child)),
            pid: Some(pid),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cloud_shell_passes_its_terminal_and_shell_names_but_no_values() {
        let mut program = CloudProgram::login_shell();
        program
            .vars
            .push(("KRAD_TASK".to_owned(), "build".to_owned()));
        let passed = program.passed();
        assert!(passed.contains(&"TERM"));
        assert!(passed.contains(&SHELL_VAR));
        assert!(passed.contains(&"KRAD_TASK"));
        assert!(!passed.contains(&"build"));
        program
            .vars
            .push(("PATH".to_owned(), "/home/me/.krad-ade/bin".to_owned()));
        assert!(!program.passed().contains(&"PATH"));
        assert_eq!(program.program, SHELL_PROGRAM);
    }
}
