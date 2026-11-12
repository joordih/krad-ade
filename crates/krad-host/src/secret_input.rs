use std::fmt;
use std::process::{Command, Output, Stdio};

use crate::command::HostCommand;
use crate::error::{HostError, Waited};
use crate::link::{HostLink, RemoteLink};
use crate::run::output_fed_within;

const LINE_END: char = '\n';
const REDACTED: &str = "[REDACTED]";

pub struct SecretCommand {
    pub command: Command,
    pub input: Option<String>,
}

impl fmt::Debug for SecretCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretCommand")
            .field("command", &self.command.get_program())
            .field("input", &self.input.as_ref().map(|_| REDACTED))
            .finish()
    }
}

fn secret_run(
    link: &HostLink,
    command: &HostCommand,
    secrets: &[&str],
) -> Result<SecretCommand, HostError> {
    let names = &command.env_from_stdin;
    if names.len() != secrets.len() {
        return Err(HostError::SecretCount {
            expected: names.len(),
            given: secrets.len(),
        });
    }
    if let Some((name, _)) = names
        .iter()
        .zip(secrets)
        .find(|(_, secret)| secret.contains(LINE_END))
    {
        return Err(HostError::MultilineSecret(
            name.to_string_lossy().into_owned(),
        ));
    }
    if names.is_empty() {
        return Ok(SecretCommand {
            command: command.build(link)?,
            input: None,
        });
    }
    match link {
        HostLink::Local => {
            let mut local = command.clone();
            local.env_from_stdin = Vec::new();
            let mut built = local.build(link)?;
            built.envs(names.iter().zip(secrets));
            Ok(SecretCommand {
                command: built,
                input: None,
            })
        }
        HostLink::Remote(_) => Ok(SecretCommand {
            command: command.build(link)?,
            input: Some(secrets.iter().fold(String::new(), |mut lines, secret| {
                lines.push_str(secret);
                lines.push(LINE_END);
                lines
            })),
        }),
    }
}

impl HostLink {
    /// # Errors
    pub fn secret_command(
        &self,
        command: &HostCommand,
        secrets: &[&str],
    ) -> Result<SecretCommand, HostError> {
        secret_run(self, command, secrets)
    }

    /// # Errors
    #[tracing::instrument(skip_all, fields(program = %command.program_name()))]
    pub fn output_with_secret(
        &self,
        command: &HostCommand,
        secret: &str,
    ) -> Result<Output, HostError> {
        let SecretCommand { mut command, input } = secret_run(self, command, &[secret])?;
        if input.is_none() {
            command.stdin(Stdio::null());
        }
        let timeout = match self {
            Self::Local => None,
            Self::Remote(ssh) => Some(ssh.args.timeouts.command.duration()),
        };
        let Some(timeout) = timeout else {
            return Ok(command.output()?);
        };
        RemoteLink::checked(output_fed_within(
            command,
            input.as_deref().map(str::as_bytes),
            timeout,
            Waited::Command,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::HostContext;
    use crate::ssh::Transport;
    use krad_domain::Host;

    const SECRET: &str = "00112233445566778899aabbccddeeff";

    fn command() -> HostCommand {
        let mut command = HostCommand::new("/home/me/.krad-ade/bin/krad-ade-agents");
        command
            .arg("cloud-status")
            .env("KRAD_ADE_CLOUD_INSTALL", "0123456789abcdef")
            .read_env_from_stdin("KRAD_ADE_CLOUD_TOKEN");
        command
    }

    #[test]
    fn over_ssh_the_secret_goes_to_stdin_never_to_the_arguments(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = HostContext {
            transport: Some(Transport::ControlMaster),
            ..HostContext::default()
        };
        let link = HostLink::new(
            &Host::Ssh {
                destination: "me@devbox".parse()?,
            },
            &context,
        );
        let run = secret_run(&link, &command(), &[SECRET])?;
        let argv: Vec<String> = std::iter::once(run.command.get_program())
            .chain(run.command.get_args())
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(argv.iter().all(|arg| !arg.contains(SECRET)));
        assert!(run
            .command
            .get_envs()
            .all(|(_, value)| value.is_none_or(|value| value != SECRET)));
        assert_eq!(run.input, Some(format!("{SECRET}\n")));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn here_the_secret_is_handed_through_the_environment() -> Result<(), HostError> {
        let mut command = HostCommand::posix_shell(r#"printf %s "$KRAD_ADE_CLOUD_TOKEN""#);
        command.read_env_from_stdin("KRAD_ADE_CLOUD_TOKEN");
        let output = HostLink::Local.output_with_secret(&command, SECRET)?;
        assert_eq!(output.stdout, SECRET.as_bytes());
        Ok(())
    }

    #[test]
    fn every_secret_takes_one_line_in_the_order_the_command_reads_them(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let link = HostLink::new(
            &Host::Ssh {
                destination: "me@devbox".parse()?,
            },
            &HostContext {
                transport: Some(Transport::ControlMaster),
                ..HostContext::default()
            },
        );
        let mut two = command();
        two.read_env_from_stdin("ANTHROPIC_API_KEY");
        let run = link.secret_command(&two, &[SECRET, "sk-1"])?;
        assert_eq!(run.input, Some(format!("{SECRET}\nsk-1\n")));
        assert!(!format!("{run:?}").contains(SECRET));
        assert!(matches!(
            link.secret_command(&two, &[SECRET]),
            Err(HostError::SecretCount {
                expected: 2,
                given: 1
            })
        ));
        assert!(matches!(
            link.secret_command(&two, &[SECRET, "a\nb"]),
            Err(HostError::MultilineSecret(name)) if name == "ANTHROPIC_API_KEY"
        ));
        Ok(())
    }
}
