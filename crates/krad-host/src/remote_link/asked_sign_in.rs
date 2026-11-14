use std::process::{Output, Stdio};
use std::time::Duration;

use super::{ssh_command, RemoteLink};
use crate::error::{HostError, Waited};
use crate::mux::{ask_through, askpass_alias, Broker, MuxError, PromptAsker};
use crate::run::output_within;
use crate::ssh::{Interaction, Transport};

const SIGN_IN_WAIT: Duration = Duration::from_mins(10);
const NO_PROMPTS: &str = "nothing answers sign-in prompts in this app";
const SSH_ENDED_SILENTLY: &str = "ssh ended without an exit code";

impl RemoteLink {
    #[must_use]
    pub fn asks_to_sign_in(&self) -> bool {
        self.machine.is_none()
            && self.args.transport == Transport::ControlMaster
            && self.tools.local_cli.is_some()
            && self.tools.prompts.is_some()
    }

    /// # Errors
    ///
    /// Returns an error when the sign-in cannot start, or ssh's reason when it gives up.
    #[tracing::instrument(skip_all, fields(destination = %self.args.destination))]
    pub fn sign_in_asking(&self, asker: PromptAsker) -> Result<(), HostError> {
        self.refuse_ssh()?;
        let local_cli = self
            .tools
            .local_cli
            .as_deref()
            .ok_or(MuxError::NoLocalCli)?;
        let prompts = self
            .tools
            .prompts
            .clone()
            .ok_or_else(|| HostError::SignInRefused(NO_PROMPTS.to_owned()))?;
        let askpass = askpass_alias(local_cli, self.tools.config_dir.as_deref())?;
        self.args.ensure_control_dir()?;
        let broker = Broker::start(asker, Some(prompts))?;
        let mut command = ssh_command(&self.args.sign_in_args(Interaction::Terminal));
        command.stdin(Stdio::null());
        ask_through(&mut command, &askpass, &broker);
        let output = output_within(command, SIGN_IN_WAIT, Waited::Connect)?;
        refused_reason(&output).map_or(Ok(()), |reason| Err(HostError::SignInRefused(reason)))
    }
}

fn refused_reason(output: &Output) -> Option<String> {
    if output.status.success() {
        return None;
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let said = stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty());
    Some(said.map_or_else(
        || match output.status.code() {
            Some(code) => format!("ssh exited with code {code}"),
            None => SSH_ENDED_SILENTLY.to_owned(),
        },
        str::to_owned,
    ))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt as _;
    use std::process::ExitStatus;

    fn output(code: i32, stderr: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn ssh_giving_up_tells_its_last_words_and_a_silent_failure_its_code() {
        assert_eq!(refused_reason(&output(0, "")), None);
        assert_eq!(
            refused_reason(&output(
                255,
                "Permission denied, please try again.\nme@gpu: Permission denied (publickey,password).\n"
            ))
            .as_deref(),
            Some("me@gpu: Permission denied (publickey,password).")
        );
        assert_eq!(
            refused_reason(&output(255, "\n")).as_deref(),
            Some("ssh exited with code 255")
        );
    }
}
