use krad_host::{HostCommand, HostError, HostLink};
use krad_session::PtyOpener;

use super::remote_pty::multiplexed_opener;
use krad_session::TERMINAL_ENV;
use std::ffi::OsStr;
use std::path::Path;

pub(in crate::environment_shell) fn remote_terminal(
    link: &HostLink,
    given: Option<(&str, &[String])>,
    cwd: Option<&Path>,
    vars: &[(String, String)],
) -> Result<Option<(String, Vec<String>)>, HostError> {
    let built = terminal_command(given, cwd, vars).build_terminal(link)?;
    Ok(built.map(|(program, args)| (text(&program), args.iter().map(|arg| text(arg)).collect())))
}

pub(in crate::environment_shell) fn remote_terminal_opener(
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
}
