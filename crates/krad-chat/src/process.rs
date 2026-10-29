use krad_domain::{parse_wsl_path, wsl_login_exec_args, INHERITED_SESSION_VARS, WSL_PROGRAM};
use std::path::{Path, PathBuf};
use std::process::Command;

const WSLENV_VAR: &str = "WSLENV";
const WSLENV_SEPARATOR: &str = ":";
const PATH_VAR: &str = "PATH";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Runner {
    Native(PathBuf),
    Wsl { distro: String, program: String },
}

impl Runner {
    #[must_use]
    pub const fn in_wsl(&self) -> bool {
        matches!(self, Self::Wsl { .. })
    }
}

pub(crate) fn harness_command(
    runner: &Runner,
    cwd: &Path,
    args: &[String],
    env: &[(String, String)],
) -> Command {
    let mut command = match runner {
        Runner::Native(program) => {
            let mut command = platform::command(program);
            command.current_dir(cwd).args(args);
            command
        }
        Runner::Wsl { distro, program } => {
            let linux_dir = parse_wsl_path(cwd).map_or_default(|inside| inside.linux);
            let mut command = Command::new(WSL_PROGRAM);
            command
                .args(wsl_login_exec_args(distro, &linux_dir))
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
            command
        }
    };
    for name in INHERITED_SESSION_VARS {
        command.env_remove(name);
    }
    command.envs(env.iter().map(|(name, value)| (name, value)));
    platform::hide_console_window(&mut command);
    command
}

#[cfg(windows)]
mod platform {
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Command;

    const CMD_PROGRAM: &str = "cmd.exe";
    const CMD_ARGS: [&str; 2] = ["/D", "/C"];
    const SCRIPT_EXTENSIONS: [&str; 2] = ["cmd", "bat"];
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

    pub(super) fn hide_console_window(command: &mut Command) {
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::Path;
    use std::process::Command;

    pub(super) fn command(program: &Path) -> Command {
        Command::new(program)
    }

    pub(super) fn hide_console_window(_command: &mut Command) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn a_harness_does_not_inherit_the_markers_of_a_claude_code_session() {
        let runner = Runner::Native(PathBuf::from("claude"));
        let command = harness_command(&runner, Path::new("/w"), &[], &[]);
        let removed: Vec<&OsStr> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name)
            .collect();
        for name in INHERITED_SESSION_VARS {
            assert!(removed.contains(&OsStr::new(name)), "{name} is inherited");
        }
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
        );
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
