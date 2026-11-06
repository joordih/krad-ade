use krad_domain::{parse_wsl_path, wsl_login_exec_args, INHERITED_SESSION_VARS, WSL_PROGRAM};
use krad_host::{hide_console_window, remote_path, HostCommand, HostLink};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const WSLENV_VAR: &str = "WSLENV";
const WSLENV_SEPARATOR: &str = ":";
const PATH_VAR: &str = "PATH";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Runner {
    Native(PathBuf),
    Wsl { distro: String, program: String },
    Ssh { link: HostLink, program: String },
}

impl Runner {
    #[must_use]
    pub const fn in_wsl(&self) -> bool {
        matches!(self, Self::Wsl { .. })
    }

    #[must_use]
    pub const fn on_ssh(&self) -> bool {
        matches!(self, Self::Ssh { .. })
    }

    #[must_use]
    pub fn link(&self) -> HostLink {
        match self {
            Self::Ssh { link, .. } => link.clone(),
            Self::Native(_) | Self::Wsl { .. } => HostLink::Local,
        }
    }

    pub(crate) fn served_directory(&self, cwd: &Path) -> String {
        match self {
            Self::Wsl { .. } => parse_wsl_path(cwd).map_or_default(|inside| inside.linux),
            Self::Native(_) => cwd.to_string_lossy().into_owned(),
            Self::Ssh { .. } => remote_path(cwd),
        }
    }
}

pub(crate) fn harness_command(
    runner: &Runner,
    cwd: &Path,
    args: &[String],
    env: &[(String, String)],
) -> io::Result<Command> {
    match runner {
        Runner::Ssh { link, program } => ssh_command(link, program, cwd, args, env),
        Runner::Native(program) => {
            let mut command = platform::command(program);
            command.current_dir(cwd).args(args);
            Ok(with_env(command, env))
        }
        Runner::Wsl { distro, program } => Ok(wsl_command(
            distro,
            &runner.served_directory(cwd),
            program,
            args,
            env,
        )),
    }
}

pub(crate) fn wsl_command(
    distro: &str,
    linux_dir: &str,
    program: &str,
    args: &[String],
    env: &[(String, String)],
) -> Command {
    let mut command = Command::new(WSL_PROGRAM);
    command
        .args(wsl_login_exec_args(distro, linux_dir))
        .arg(program)
        .args(args);
    let shared: Vec<String> = env
        .iter()
        .filter(|(name, _)| name != PATH_VAR)
        .map(|(name, _)| name.clone())
        .collect();
    if !shared.is_empty() {
        command.env(WSLENV_VAR, shared.join(WSLENV_SEPARATOR));
    }
    with_env(command, env)
}

fn with_env(mut command: Command, env: &[(String, String)]) -> Command {
    for name in INHERITED_SESSION_VARS {
        command.env_remove(name);
    }
    command.envs(env.iter().map(|(name, value)| (name, value)));
    hide_console_window(&mut command);
    command
}

fn ssh_command(
    link: &HostLink,
    program: &str,
    cwd: &Path,
    args: &[String],
    env: &[(String, String)],
) -> io::Result<Command> {
    let mut remote = HostCommand::new(program);
    remote.args(args).current_dir(cwd).in_login_shell();
    remote.path_and_env(env);
    Ok(remote.build(link)?)
}

pub(crate) fn secret_from_stdin_script(name: &str) -> String {
    format!(r#"IFS= read -r {name} && export {name} && exec "$@""#)
}

pub(crate) fn ssh_command_reading_secret(
    link: &HostLink,
    program: &str,
    cwd: &Path,
    args: &[String],
    env: &[(String, String)],
    secret: &str,
) -> io::Result<Command> {
    let mut remote = HostCommand::posix_shell(&secret_from_stdin_script(secret));
    remote
        .arg(program)
        .args(args)
        .current_dir(cwd)
        .in_login_shell();
    remote.path_and_env(env);
    Ok(remote.build(link)?)
}

#[cfg(windows)]
mod platform {
    use std::path::Path;
    use std::process::Command;

    const CMD_PROGRAM: &str = "cmd.exe";
    const CMD_ARGS: [&str; 2] = ["/D", "/C"];
    const SCRIPT_EXTENSIONS: [&str; 2] = ["cmd", "bat"];

    pub(super) fn command(program: &Path) -> Command {
        let is_script = program.extension().is_some_and(|extension| {
            SCRIPT_EXTENSIONS
                .iter()
                .any(|script| extension.eq_ignore_ascii_case(script))
        });
        if is_script {
            let mut command = Command::new(CMD_PROGRAM);
            command.args(CMD_ARGS).arg(program);
            command
        } else {
            Command::new(program)
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::Path;
    use std::process::Command;

    pub(super) fn command(program: &Path) -> Command {
        Command::new(program)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn a_harness_does_not_inherit_the_markers_of_a_claude_code_session() {
        let runner = Runner::Native(PathBuf::from("claude"));
        let command = harness_command(&runner, Path::new("/w"), &[], &[])
            .unwrap_or_else(|_| Command::new("missing"));
        let removed: Vec<&OsStr> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name)
            .collect();
        for name in INHERITED_SESSION_VARS {
            assert!(removed.contains(&OsStr::new(name)), "{name} is inherited");
        }
    }

    fn sent_script(command: &Command) -> Result<String, Box<dyn std::error::Error>> {
        let remote = command
            .get_args()
            .last()
            .map(|arg| arg.to_string_lossy().into_owned())
            .ok_or("no remote script")?;
        let encoded = remote
            .rsplit_once(" '")
            .and_then(|(_, payload)| payload.strip_suffix('\''))
            .ok_or("no encoded script")?;
        let mut bytes = Vec::new();
        let mut rest = encoded.as_bytes();
        while let Some((&byte, tail)) = rest.split_first() {
            if byte == b'\\' {
                let digits = std::str::from_utf8(tail.get(..3).ok_or("short escape")?)?;
                bytes.push(u8::from_str_radix(digits, 8)?);
                rest = &tail[3..];
            } else {
                bytes.push(byte);
                rest = tail;
            }
        }
        Ok(String::from_utf8(bytes)?)
    }

    #[test]
    fn an_ssh_harness_runs_in_the_hosts_folder_through_its_login_shell(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let host = krad_domain::Host::Ssh {
            destination: "devbox".parse()?,
        };
        let runner = Runner::Ssh {
            link: HostLink::new(&host, &krad_host::HostContext::default()),
            program: "claude".to_owned(),
        };
        let env = [
            ("KRAD_ADE_TOKEN_FILE".to_owned(), "/home/me/t".to_owned()),
            (PATH_VAR.to_owned(), "/home/me/.krad-ade/bin".to_owned()),
        ];
        let command = harness_command(&runner, Path::new("/srv/api"), &["-p".to_owned()], &env)?;
        assert_eq!(command.get_program(), OsStr::new("ssh"));
        let script = sent_script(&command)?;
        assert!(script.contains("cd '/srv/api' && exec env 'KRAD_ADE_TOKEN_FILE=/home/me/t'"));
        assert!(script.contains("'claude' '-p'"));
        assert!(script.contains("PATH='/home/me/.krad-ade/bin:'\"$PATH\""));
        assert_eq!(command.get_envs().count(), 0);
        Ok(())
    }

    #[test]
    fn a_secret_for_the_host_is_read_from_stdin_and_never_typed_on_the_command_line(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let host = krad_domain::Host::Ssh {
            destination: "devbox".parse()?,
        };
        let command = ssh_command_reading_secret(
            &HostLink::new(&host, &krad_host::HostContext::default()),
            "opencode",
            Path::new("/srv"),
            &["serve".to_owned()],
            &[],
            "OPENCODE_PASSWORD",
        )?;
        let script = sent_script(&command)?;
        assert!(script.contains("read -r OPENCODE_PASSWORD"));
        assert!(script.contains("opencode"));
        assert!(!script.contains("OPENCODE_PASSWORD="));
        Ok(())
    }

    #[test]
    fn a_wsl_harness_runs_in_the_linux_folder_with_the_variables_shared() {
        let runner = Runner::Wsl {
            distro: "Ubuntu".to_owned(),
            program: "claude".to_owned(),
        };
        let env = [
            ("KRAD_ADE_TOKEN".to_owned(), "t".to_owned()),
            (PATH_VAR.to_owned(), "C:\\bin".to_owned()),
        ];
        let command = harness_command(
            &runner,
            Path::new(r"\\wsl.localhost\Ubuntu\home\dev\api"),
            &["-p".to_owned()],
            &env,
        )
        .unwrap_or_else(|_| Command::new("missing"));
        assert_eq!(command.get_program(), OsStr::new(WSL_PROGRAM));
        let args: Vec<&OsStr> = command.get_args().collect();
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--cd", "/home/dev/api"]));
        assert_eq!(
            args[args.len() - 2..],
            [OsStr::new("claude"), OsStr::new("-p")]
        );
        let wslenv = command
            .get_envs()
            .find(|(name, _)| *name == OsStr::new(WSLENV_VAR))
            .and_then(|(_, value)| value);
        assert_eq!(wslenv, Some(OsStr::new("KRAD_ADE_TOKEN")));
    }
}
