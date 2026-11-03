mod script;
pub(crate) mod transport;
mod watcher;

pub(crate) use script::ScriptContext;

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::HostError;
use crate::link::HostLink;
use crate::local::hide_console_window;
use crate::ssh::{Interaction, SshArgs};

const ENV_NAME_JOINER: char = '_';
const SINGLE_QUOTE: char = '\'';
const ESCAPED_SINGLE_QUOTE: &str = r"'\''";
const PATH_VAR: &str = "PATH";
const EXPORT: &str = "export";
const SPACE: &str = " ";
const NEWLINE: char = '\n';
const ASSIGN: char = '=';
const SH_PROGRAM: &str = "sh";
const SH_COMMAND_FLAG: &str = "-c";
const SH_SCRIPT_NAME: &str = "sh";
const LOGIN_SHELL_NAME: &str = "login shell";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Program {
    Named(OsString),
    LoginShell,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostCommand {
    pub program: Program,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(OsString, OsString)>,
    pub through_login_shell: bool,
    pub path_prefix: Option<PathBuf>,
    pub env_file: Option<PathBuf>,
    pub ends_with_stdin: bool,
}

impl HostCommand {
    #[must_use]
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self::with_program(Program::Named(program.as_ref().to_owned()))
    }

    #[must_use]
    pub fn posix_shell(script: &str) -> Self {
        let mut command = Self::new(SH_PROGRAM);
        command.args([SH_COMMAND_FLAG, script, SH_SCRIPT_NAME]);
        command
    }

    #[must_use]
    pub const fn login_shell() -> Self {
        Self::with_program(Program::LoginShell)
    }

    const fn with_program(program: Program) -> Self {
        Self {
            program,
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            through_login_shell: false,
            path_prefix: None,
            env_file: None,
            ends_with_stdin: false,
        }
    }

    pub fn prepend_path(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.path_prefix = Some(dir.as_ref().to_owned());
        self
    }

    pub const fn end_with_stdin(&mut self) -> &mut Self {
        self.ends_with_stdin = true;
        self
    }

    pub fn path_and_env<'a>(
        &mut self,
        vars: impl IntoIterator<Item = &'a (String, String)>,
    ) -> &mut Self {
        for (key, value) in vars {
            if key == PATH_VAR {
                self.prepend_path(value);
            } else {
                self.env(key, value);
            }
        }
        self
    }

    pub fn source_env_file(&mut self, file: impl AsRef<Path>) -> &mut Self {
        self.env_file = Some(file.as_ref().to_owned());
        self
    }

    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.args.push(arg.as_ref().to_owned());
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|arg| arg.as_ref().to_owned()));
        self
    }

    pub fn current_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.cwd = Some(dir.as_ref().to_owned());
        self
    }

    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.env
            .push((key.as_ref().to_owned(), value.as_ref().to_owned()));
        self
    }

    #[must_use]
    pub fn program_name(&self) -> String {
        match &self.program {
            Program::Named(program) => std::path::Path::new(program)
                .file_name()
                .unwrap_or(program)
                .to_string_lossy()
                .into_owned(),
            Program::LoginShell => LOGIN_SHELL_NAME.to_owned(),
        }
    }

    pub const fn in_login_shell(&mut self) -> &mut Self {
        self.through_login_shell = true;
        self
    }

    /// # Errors
    pub fn build(&self, link: &HostLink) -> Result<Command, HostError> {
        match link {
            HostLink::Local => Ok(self.local()),
            HostLink::Ssh(ssh) => {
                let login_env = ssh.login_env_file();
                if let Some(mux) = ssh.multiplexed() {
                    return Ok(mux.command(self.script_with(ScriptContext {
                        login_env: login_env.as_deref(),
                    })));
                }
                let script = self.script_with(ScriptContext {
                    login_env: login_env.as_deref(),
                });
                let word = match transport::inline(&script) {
                    Some(word) => word,
                    None => transport::sourcing(&ssh.stage_script(&script)?),
                };
                Ok(over_ssh(&ssh.args, &word))
            }
        }
    }

    /// # Errors
    pub fn build_terminal(
        &self,
        link: &HostLink,
    ) -> Result<Option<(OsString, Vec<OsString>)>, HostError> {
        let HostLink::Ssh(ssh) = link else {
            return Ok(None);
        };
        let login_env = ssh.login_env_file();
        let script = self.script_with(ScriptContext {
            login_env: login_env.as_deref(),
        });
        let word = match transport::inline(&script) {
            Some(word) => word,
            None => transport::sourcing(&ssh.stage_script(&script)?),
        };
        let mut args = ssh.args.base(Interaction::Terminal);
        args.push(OsString::from(word));
        Ok(Some((SshArgs::program(), args)))
    }

    pub(crate) fn plain_script(&self) -> String {
        self.script_with(ScriptContext::default())
    }

    pub(crate) fn over_ssh_plain(
        &self,
        ssh: &SshArgs,
        stage: impl FnOnce(&str) -> Result<String, HostError>,
    ) -> Result<Command, HostError> {
        let script = self.script_with(ScriptContext::default());
        let word = match transport::inline(&script) {
            Some(word) => word,
            None => transport::sourcing(&stage(&script)?),
        };
        Ok(over_ssh(ssh, &word))
    }

    fn local(&self) -> Command {
        let program = match &self.program {
            Program::Named(program) => program.clone(),
            Program::LoginShell => crate::local::default_shell(),
        };
        let mut command = Command::new(program);
        command.args(&self.args);
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        command.envs(self.env.iter().map(|(key, value)| (key, value)));
        if let Some(dir) = &self.path_prefix {
            let inherited = self
                .env
                .iter()
                .find(|(key, _)| key == PATH_VAR)
                .map(|(_, value)| value.clone())
                .or_else(|| std::env::var_os(PATH_VAR))
                .unwrap_or_default();
            let joined = std::env::join_paths(
                std::iter::once(dir.clone()).chain(std::env::split_paths(&inherited)),
            );
            if let Ok(path) = joined {
                command.env(PATH_VAR, path);
            }
        }
        hide_console_window(&mut command);
        command
    }
}

fn over_ssh(ssh: &SshArgs, word: &str) -> Command {
    let mut command = Command::new(SshArgs::program());
    command.args(ssh.base(Interaction::Batch));
    command.arg(word);
    hide_console_window(&mut command);
    command
}

/// # Errors
pub fn env_file_text<K: AsRef<str>, V: AsRef<str>>(vars: &[(K, V)]) -> Result<String, HostError> {
    let mut text = String::new();
    for (key, value) in vars {
        if !is_env_name(key.as_ref()) {
            return Err(HostError::InvalidEnvName(key.as_ref().to_owned()));
        }
        text.push_str(EXPORT);
        text.push_str(SPACE);
        text.push_str(key.as_ref());
        text.push(ASSIGN);
        text.push_str(&quote(value.as_ref()));
        text.push(NEWLINE);
    }
    Ok(text)
}

#[must_use]
pub fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == ENV_NAME_JOINER)
        && chars.all(|c| c.is_ascii_alphanumeric() || c == ENV_NAME_JOINER)
}

#[must_use]
pub fn quote(word: &str) -> String {
    let escaped = word.replace(SINGLE_QUOTE, ESCAPED_SINGLE_QUOTE);
    format!("{SINGLE_QUOTE}{escaped}{SINGLE_QUOTE}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_files_refuse_names_the_shell_cannot_export() {
        assert!(env_file_text(&[("MY_VAR", "x")]).is_ok());
        for bad in ["MY-VAR", "1ABC", "A B", "X;rm", ""] {
            assert!(matches!(
                env_file_text(&[(bad, "x")]),
                Err(HostError::InvalidEnvName(_))
            ));
        }
    }

    #[test]
    fn words_are_single_quoted_with_quotes_escaped() {
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote("$HOME; rm -rf /"), "'$HOME; rm -rf /'");
    }

    #[test]
    fn a_posix_shell_script_gets_its_name_and_arguments() {
        let mut command = HostCommand::posix_shell(r#"printf %s "$1""#);
        command.arg("x");
        assert_eq!(
            command.posix_script(),
            r#"exec 'sh' '-c' 'printf %s "$1"' 'sh' 'x'"#
        );
    }

    #[test]
    fn the_local_program_gets_the_path_prefix_first() {
        let mut command = HostCommand::new("tool");
        command
            .env(PATH_VAR, "/usr/bin")
            .prepend_path("/opt/tool/bin");
        let built = command.local();
        let path = built
            .get_envs()
            .find(|(key, _)| *key == PATH_VAR)
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned());
        let separator = if cfg!(windows) { ";" } else { ":" };
        assert_eq!(path, Some(format!("/opt/tool/bin{separator}/usr/bin")));
    }
}
