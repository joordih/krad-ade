use futures::channel::mpsc;
use krad_domain::AgentPreset;
use krad_host::{HostCommand, HostError, HostLink};
use krad_session::{PtyOpener, TERMINAL_ENV};
use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};

use super::remote_pty::multiplexed_opener;

pub type OutputTap = mpsc::Sender<Vec<u8>>;

pub struct TerminalLaunch {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub vars: Vec<(String, String)>,
    pub place: TerminalPlace,
    pub harness: Option<AgentPreset>,
    pub prefill: Vec<String>,
    pub typed: Option<Vec<u8>>,
    pub tap: Option<OutputTap>,
}

impl fmt::Debug for TerminalLaunch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TerminalLaunch")
            .field("harness", &self.harness)
            .field("place", &self.place)
            .finish_non_exhaustive()
    }
}

pub enum TerminalPlace {
    Here,
    Host {
        link: HostLink,
        shell: HostShell,
        in_wsl: bool,
    },
    Opened(PtyOpener),
}

impl fmt::Debug for TerminalPlace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Here => formatter.write_str("Here"),
            Self::Host { shell, in_wsl, .. } => formatter
                .debug_struct("Host")
                .field("shell", shell)
                .field("in_wsl", in_wsl)
                .finish_non_exhaustive(),
            Self::Opened(_) => formatter.write_str("Opened"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HostShell {
    Login,
    Given,
}

pub(crate) struct Placed {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) vars: Vec<(String, String)>,
    pub(crate) opener: Option<PtyOpener>,
}

impl Placed {
    pub(crate) fn place(self, place: TerminalPlace) -> Result<Self, HostError> {
        match place {
            TerminalPlace::Here => Ok(self),
            TerminalPlace::Opened(opener) => Ok(self.opened(opener)),
            TerminalPlace::Host {
                link,
                shell,
                in_wsl,
            } => self.on(&link, shell, in_wsl),
        }
    }

    fn opened(self, opener: PtyOpener) -> Self {
        Self {
            cwd: None,
            vars: Vec::new(),
            opener: Some(opener),
            ..self
        }
    }

    fn on(self, link: &HostLink, shell: HostShell, in_wsl: bool) -> Result<Self, HostError> {
        let given =
            (shell == HostShell::Given).then_some((self.program.as_str(), self.args.as_slice()));
        if let Some(opener) = remote_terminal_opener(link, given, self.cwd.as_deref(), &self.vars) {
            return Ok(self.opened(opener));
        }
        Ok(
            match remote_terminal(link, given, self.cwd.as_deref(), &self.vars)? {
                Some((program, args)) => Self {
                    program,
                    args,
                    cwd: None,
                    vars: Vec::new(),
                    opener: None,
                },
                None if in_wsl => Self { cwd: None, ..self },
                None => self,
            },
        )
    }
}

/// # Errors
///
/// Fails when the host cannot build the command that opens the terminal.
pub fn remote_terminal(
    link: &HostLink,
    given: Option<(&str, &[String])>,
    cwd: Option<&Path>,
    vars: &[(String, String)],
) -> Result<Option<(String, Vec<String>)>, HostError> {
    let built = terminal_command(given, cwd, vars).build_terminal(link)?;
    Ok(built.map(|(program, args)| (text(&program), args.iter().map(|arg| text(arg)).collect())))
}

#[must_use]
pub fn remote_terminal_opener(
    link: &HostLink,
    given: Option<(&str, &[String])>,
    cwd: Option<&Path>,
    vars: &[(String, String)],
) -> Option<PtyOpener> {
    multiplexed_opener(link, terminal_command(given, cwd, vars))
}

fn terminal_command(
    given: Option<(&str, &[String])>,
    cwd: Option<&Path>,
    vars: &[(String, String)],
) -> HostCommand {
    let mut command = match given {
        Some((program, args)) => {
            let mut command = HostCommand::new(program);
            command.args(args).in_login_shell();
            command
        }
        None => HostCommand::login_shell(),
    };
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    for (key, value) in TERMINAL_ENV {
        command.env(key, value);
    }
    command.path_and_env(vars);
    command
}

fn text(word: &OsStr) -> String {
    word.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_host::{RemoteLink, SshArgs};
    use std::sync::Arc;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn ssh_link() -> Result<HostLink, krad_domain::SshDestinationError> {
        Ok(HostLink::Remote(Arc::new(RemoteLink::new(SshArgs::new(
            "me@devbox".parse()?,
            None,
        )))))
    }

    fn placed(cwd: Option<&str>) -> Placed {
        Placed {
            program: "zsh".to_owned(),
            args: vec!["-l".to_owned()],
            cwd: cwd.map(PathBuf::from),
            vars: vec![("KRAD".to_owned(), "1".to_owned())],
            opener: None,
        }
    }

    #[test]
    fn only_a_multiplexed_host_opens_terminals_through_its_session() -> TestResult {
        assert!(remote_terminal_opener(&HostLink::Local, None, None, &[]).is_none());
        assert!(remote_terminal_opener(&ssh_link()?, None, None, &[]).is_none());
        Ok(())
    }

    #[test]
    fn a_local_link_keeps_the_local_launch() {
        assert!(matches!(
            remote_terminal(&HostLink::Local, None, None, &[]),
            Ok(None)
        ));
    }

    #[test]
    fn a_remote_shell_opens_with_a_tty_in_its_folder() -> TestResult {
        let vars = [("KRAD_LAUNCH".to_owned(), "it's".to_owned())];
        let (program, args) =
            remote_terminal(&ssh_link()?, None, Some(Path::new("/srv/app")), &vars)?
                .ok_or("no remote launch")?;
        assert_eq!(program, "ssh");
        assert!(args.contains(&"-t".to_owned()));
        let script = terminal_command(None, Some(Path::new("/srv/app")), &vars).posix_script();
        assert!(script.contains("cd '/srv/app' && exec env 'TERM=xterm-256color'"));
        assert!(script.contains(r"'KRAD_LAUNCH=it'\''s'"));
        assert!(script.ends_with(r#""${SHELL:-/bin/sh}" -l"#));
        Ok(())
    }

    #[test]
    fn a_given_command_runs_as_itself_on_the_host() -> TestResult {
        let args = ["-c".to_owned(), "top".to_owned()];
        assert!(remote_terminal(&ssh_link()?, Some(("sh", &args)), None, &[])?.is_some());
        let script = terminal_command(Some(("sh", &args)), None, &[]).posix_script();
        assert!(script.ends_with("'sh' '-c' 'top'"));
        Ok(())
    }

    #[test]
    fn a_launch_here_keeps_its_folder_and_variables() -> TestResult {
        let here = placed(Some("/work")).place(TerminalPlace::Here)?;
        assert_eq!(here.cwd, Some(PathBuf::from("/work")));
        assert_eq!(here.vars.len(), 1);
        let local = TerminalPlace::Host {
            link: HostLink::Local,
            shell: HostShell::Login,
            in_wsl: false,
        };
        assert_eq!(
            placed(Some("/work")).place(local)?.cwd,
            Some(PathBuf::from("/work"))
        );
        Ok(())
    }

    #[test]
    fn a_wsl_launch_drops_the_windows_folder_and_a_remote_one_runs_through_ssh() -> TestResult {
        let wsl = TerminalPlace::Host {
            link: HostLink::Local,
            shell: HostShell::Login,
            in_wsl: true,
        };
        assert_eq!(placed(Some("/work")).place(wsl)?.cwd, None);
        let remote = TerminalPlace::Host {
            link: ssh_link()?,
            shell: HostShell::Login,
            in_wsl: false,
        };
        let on_host = placed(Some("/srv/app")).place(remote)?;
        assert_eq!(on_host.program, "ssh");
        assert_eq!(on_host.cwd, None);
        assert_eq!(on_host.vars, Vec::new());
        Ok(())
    }
}
