use std::path::Path;

use super::{quote, watcher, HostCommand, Program};
use crate::path::remote_path;

const LOGIN_SHELL: &str = r#""${SHELL:-/bin/sh}" -l"#;
const INTERACTIVE_SHELL: &str = r#""${SHELL:-/bin/sh}" -i"#;
const PROFILE_FALLBACK: &str = r#"{ . /etc/profile; [ -r "$HOME/.profile" ] && . "$HOME/.profile"; } >/dev/null 2>&1 </dev/null; :"#;
const SOURCED_STATUS: &str = "sourced=$?";
const SOURCED_OK: &str = r#"[ "$sourced" -eq 0 ]"#;
const PATH_LIST_SEPARATOR: char = ':';
const ENV_PROGRAM: &str = "env";
const CD: &str = "cd";
const AND: &str = " && ";
const THEN: &str = "; ";
const EXEC: &str = "exec";
const SPACE: &str = " ";
const PATH_VAR: &str = "PATH";
const REMOTE_PATH_TAIL: &str = r#""$PATH""#;
const SOURCE: &str = ".";
const REMOVE_FILE: &str = "rm -f";
const QUIET: &str = ">/dev/null 2>&1";
const ASSIGN: char = '=';

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ScriptContext<'a> {
    pub(crate) login_env: Option<&'a Path>,
}

impl HostCommand {
    fn path_word(&self) -> Option<String> {
        self.path_prefix.as_deref().map(|dir| {
            format!(
                "{PATH_VAR}{ASSIGN}{}{REMOTE_PATH_TAIL}",
                quote(&format!("{}{PATH_LIST_SEPARATOR}", remote_path(dir)))
            )
        })
    }

    fn env_words(&self) -> Option<String> {
        let path_word = self.path_word();
        if self.env.is_empty() && path_word.is_none() {
            return None;
        }
        let mut words = vec![ENV_PROGRAM.to_owned()];
        words.extend(self.env.iter().map(|(key, value)| {
            quote(&format!(
                "{}{ASSIGN}{}",
                key.to_string_lossy(),
                value.to_string_lossy()
            ))
        }));
        words.extend(path_word);
        Some(words.join(SPACE))
    }

    fn program_words(&self, context: ScriptContext<'_>) -> String {
        match &self.program {
            Program::LoginShell if context.login_env.is_some() => INTERACTIVE_SHELL.to_owned(),
            Program::LoginShell => LOGIN_SHELL.to_owned(),
            Program::Named(program) => std::iter::once(program)
                .chain(&self.args)
                .map(|word| quote(&word.to_string_lossy()))
                .collect::<Vec<_>>()
                .join(SPACE),
        }
    }

    fn uses_login_env(&self) -> bool {
        self.through_login_shell || matches!(self.program, Program::LoginShell)
    }

    fn env_file_step(&self) -> Option<String> {
        let file = quote(&remote_path(self.env_file.as_deref()?));
        Some(format!(
            "{SOURCE} {file}; {SOURCED_STATUS}; {REMOVE_FILE} {file}; {SOURCED_OK} || exit 1{THEN}"
        ))
    }

    fn login_env_step(&self, context: ScriptContext<'_>) -> Option<String> {
        if !self.uses_login_env() {
            return None;
        }
        match context.login_env {
            Some(snapshot) => Some(format!(
                "{SOURCE} {} {QUIET}{THEN}",
                quote(&remote_path(snapshot))
            )),
            None if matches!(self.program, Program::Named(_)) => {
                Some(format!("{PROFILE_FALLBACK}{THEN}"))
            }
            None => None,
        }
    }

    fn cd_step(&self) -> Option<String> {
        let cwd = self.cwd.as_deref()?;
        Some(format!("{CD} {}{AND}", quote(&remote_path(cwd))))
    }

    pub(crate) fn script_with(&self, context: ScriptContext<'_>) -> String {
        let run = match self.env_words() {
            Some(env) => format!("{env} {}", self.program_words(context)),
            None => self.program_words(context),
        };
        let script: String = [
            self.env_file_step(),
            self.login_env_step(context),
            self.cd_step(),
            Some(format!("{EXEC} {run}")),
        ]
        .into_iter()
        .flatten()
        .collect();
        if self.ends_with_stdin {
            return watcher::bound_to_stdin(&script);
        }
        script
    }

    #[must_use]
    pub fn posix_script(&self) -> String {
        self.script_with(ScriptContext::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn run_in_sh(script: &str) -> std::io::Result<String> {
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .env("SHELL", "/bin/sh")
            .env("PATH", "/usr/bin:/bin")
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    #[test]
    fn a_command_runs_in_its_folder_with_its_env() {
        let mut command = HostCommand::new("git");
        command
            .args(["status", "--porcelain"])
            .current_dir("/home/me/my repo")
            .env("GIT_OPTIONAL_LOCKS", "0");
        assert_eq!(
            command.posix_script(),
            "cd '/home/me/my repo' && exec env 'GIT_OPTIONAL_LOCKS=0' 'git' 'status' '--porcelain'"
        );
    }

    #[test]
    fn a_login_command_sources_the_captured_environment_first() {
        let mut command = HostCommand::new("claude");
        command.arg("--resume").in_login_shell();
        let context = ScriptContext {
            login_env: Some(Path::new("/home/me/.krad-ade/clients/x/login-env")),
        };
        assert_eq!(
            command.script_with(context),
            ". '/home/me/.krad-ade/clients/x/login-env' >/dev/null 2>&1; exec 'claude' '--resume'"
        );
        assert!(command.posix_script().starts_with("{ . /etc/profile;"));
    }

    #[test]
    fn a_terminal_starts_the_users_shell_with_the_captured_environment() {
        let mut command = HostCommand::login_shell();
        command
            .current_dir("/srv/app")
            .env("COLORTERM", "truecolor")
            .prepend_path("/home/me/.krad-ade/0.2.0/bin");
        let context = ScriptContext {
            login_env: Some(Path::new("/e")),
        };
        assert_eq!(
            command.script_with(context),
            r#". '/e' >/dev/null 2>&1; cd '/srv/app' && exec env 'COLORTERM=truecolor' PATH='/home/me/.krad-ade/0.2.0/bin:'"$PATH" "${SHELL:-/bin/sh}" -i"#
        );
        assert!(command
            .posix_script()
            .ends_with(r#""${SHELL:-/bin/sh}" -l"#));
    }

    #[cfg(unix)]
    #[test]
    fn the_path_prefix_survives_a_snapshot_that_sets_path() -> std::io::Result<()> {
        let snapshot = std::env::temp_dir().join(format!("krad-snapshot-{}", std::process::id()));
        std::fs::write(&snapshot, "export PATH='/opt/login/bin:/usr/bin:/bin'\n")?;
        let mut command = HostCommand::new("sh");
        command
            .args(["-c", r#"printf %s "$PATH""#])
            .prepend_path("/home/me/.krad-ade/bin")
            .in_login_shell();
        let script = command.script_with(ScriptContext {
            login_env: Some(&snapshot),
        });
        let path = run_in_sh(&script)?;
        std::fs::remove_file(&snapshot)?;
        assert_eq!(path, "/home/me/.krad-ade/bin:/opt/login/bin:/usr/bin:/bin");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn an_env_file_is_sourced_and_removed_before_the_program_starts() -> std::io::Result<()> {
        let file = std::env::temp_dir().join(format!("krad-env-{}", std::process::id()));
        std::fs::write(
            &file,
            super::super::env_file_text(&[("API_KEY", "it's $secret")])?,
        )?;
        let mut command = HostCommand::new("sh");
        command
            .args(["-c", r#"printf %s "$API_KEY""#])
            .current_dir("/nonexistent-krad-folder")
            .source_env_file(&file);
        run_in_sh(&command.posix_script())?;
        assert!(!file.exists());
        std::fs::write(
            &file,
            super::super::env_file_text(&[("API_KEY", "it's $secret")])?,
        )?;
        command.cwd = None;
        assert_eq!(run_in_sh(&command.posix_script())?, "it's $secret");
        assert!(!file.exists());
        Ok(())
    }
}
