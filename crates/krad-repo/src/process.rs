use krad_domain::{
    parse_wsl_path, wsl_unc_path, WslPath, POSIX_SHELL, POSIX_SHELL_COMMAND_FLAG,
    WSL_LOGIN_SHELL_SCRIPT,
};
use krad_host::{remote_path, HostCommand, HostLink};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::RepoError;

pub(crate) const GIT_PROGRAM: &str = "git";
const OPTIONAL_LOCKS_VAR: &str = "GIT_OPTIONAL_LOCKS";
const OPTIONAL_LOCKS_DISABLED: &str = "0";
const TERMINAL_PROMPT_VAR: &str = "GIT_TERMINAL_PROMPT";
const TERMINAL_PROMPT_DISABLED: &str = "0";
const MESSAGES_LOCALE_VAR: &str = "LC_MESSAGES";
const MESSAGES_LOCALE: &str = "C";
const LANGUAGE_VAR: &str = "LANGUAGE";
const QUOTE_PATH_DISABLED: &str = "core.quotePath=false";

const WSL_PROGRAM: &str = "wsl.exe";
const WSL_DISTRO_FLAG: &str = "--distribution";
const WSL_CD_FLAG: &str = "--cd";
const WSL_EXEC_FLAG: &str = "--exec";
const ENV_PROGRAM: &str = "env";
const GIT_DIR_FLAG: &str = "-C";
const GIT_CONFIG_FLAG: &str = "-c";
const PRINT_HOME: &str = r#"printf %s "$HOME""#;
const LINUX_ROOT: char = '/';
const READLINK_PROGRAM: &str = "readlink";
const CANONICALIZE_FLAG: &str = "-f";
const ARG_SEPARATOR: &str = " ";
const PRINT_FOUND_PROGRAMS: &str = r#"for program in "$@"; do command -v "$program" >/dev/null 2>&1 && printf '%s\n' "$program"; done; :"#;

pub(crate) fn git_output<S: AsRef<OsStr>>(
    host: &HostLink,
    repo: &Path,
    args: &[S],
) -> Result<Output, RepoError> {
    if host.is_local() {
        let mut command = local_git_command(repo);
        command.args(args);
        return Ok(command.output()?);
    }
    let mut command = HostCommand::new(GIT_PROGRAM);
    command
        .arg(GIT_DIR_FLAG)
        .arg(remote_path(repo))
        .args([GIT_CONFIG_FLAG, QUOTE_PATH_DISABLED])
        .args(args)
        .env(OPTIONAL_LOCKS_VAR, OPTIONAL_LOCKS_DISABLED)
        .env(TERMINAL_PROMPT_VAR, TERMINAL_PROMPT_DISABLED)
        .env(MESSAGES_LOCALE_VAR, MESSAGES_LOCALE)
        .env(LANGUAGE_VAR, MESSAGES_LOCALE);
    Ok(host.output(&command)?)
}

fn local_git_command(repo: &Path) -> Command {
    let mut command = parse_wsl_path(repo).map_or_else(
        || {
            let mut command = Command::new(GIT_PROGRAM);
            command
                .arg(GIT_DIR_FLAG)
                .arg(repo)
                .env(OPTIONAL_LOCKS_VAR, OPTIONAL_LOCKS_DISABLED)
                .env(TERMINAL_PROMPT_VAR, TERMINAL_PROMPT_DISABLED)
                .env(MESSAGES_LOCALE_VAR, MESSAGES_LOCALE)
                .env(LANGUAGE_VAR, MESSAGES_LOCALE);
            command
        },
        |WslPath { distro, linux }| {
            let mut command = Command::new(WSL_PROGRAM);
            command
                .args([
                    WSL_DISTRO_FLAG,
                    &distro,
                    WSL_CD_FLAG,
                    &linux,
                    WSL_EXEC_FLAG,
                    ENV_PROGRAM,
                ])
                .arg(format!("{OPTIONAL_LOCKS_VAR}={OPTIONAL_LOCKS_DISABLED}"))
                .arg(format!("{TERMINAL_PROMPT_VAR}={TERMINAL_PROMPT_DISABLED}"))
                .arg(format!("{MESSAGES_LOCALE_VAR}={MESSAGES_LOCALE}"))
                .arg(format!("{LANGUAGE_VAR}={MESSAGES_LOCALE}"))
                .arg(GIT_PROGRAM);
            command
        },
    );
    command.args(["-c", QUOTE_PATH_DISABLED]);
    hide_console_window(&mut command);
    command
}

pub(crate) fn git_path(host: &HostLink, repo: &Path, path: &Path) -> OsString {
    if !host.is_local() {
        return OsString::from(remote_path(path));
    }
    match (parse_wsl_path(repo), parse_wsl_path(path)) {
        (Some(_), Some(inside)) => OsString::from(inside.linux),
        _ => path.as_os_str().to_owned(),
    }
}

pub(crate) fn host_path(host: &HostLink, repo: &Path, printed: &str) -> PathBuf {
    if !host.is_local() {
        return PathBuf::from(printed);
    }
    match parse_wsl_path(repo) {
        Some(WslPath { distro, .. }) => wsl_unc_path(&distro, printed),
        None => PathBuf::from(printed),
    }
}

/// # Errors
pub fn wsl_home(distro: &str) -> Result<String, RepoError> {
    let args = [POSIX_SHELL, POSIX_SHELL_COMMAND_FLAG, PRINT_HOME];
    let home = String::from_utf8_lossy(&run_in_wsl(distro, &args)?).into_owned();
    if home.starts_with(LINUX_ROOT) {
        return Ok(home);
    }
    Err(wsl_error(distro, &args, &home))
}

/// # Errors
pub fn wsl_programs<'a>(distro: &str, programs: &[&'a str]) -> Result<Vec<&'a str>, RepoError> {
    let mut args = vec![
        POSIX_SHELL,
        POSIX_SHELL_COMMAND_FLAG,
        WSL_LOGIN_SHELL_SCRIPT,
        POSIX_SHELL,
        PRINT_FOUND_PROGRAMS,
    ];
    args.extend_from_slice(programs);
    let output = String::from_utf8_lossy(&run_in_wsl(distro, &args)?).into_owned();
    Ok(programs
        .iter()
        .copied()
        .filter(|program| output.lines().any(|line| line.trim() == *program))
        .collect())
}

/// # Errors
pub fn host_programs<'a>(host: &HostLink, programs: &[&'a str]) -> Result<Vec<&'a str>, RepoError> {
    let mut command = HostCommand::posix_shell(PRINT_FOUND_PROGRAMS);
    command.args(programs).in_login_shell();
    let output = host.output(&command)?;
    let printed = String::from_utf8_lossy(&output.stdout);
    Ok(programs
        .iter()
        .copied()
        .filter(|program| printed.lines().any(|line| line.trim() == *program))
        .collect())
}

pub(crate) fn wsl_resolve_symlinks(path: &Path) -> Result<Option<PathBuf>, RepoError> {
    let Some(WslPath { distro, linux }) = parse_wsl_path(path) else {
        return Ok(None);
    };
    let resolved = run_in_wsl(&distro, &[READLINK_PROGRAM, CANONICALIZE_FLAG, &linux])?;
    let resolved = String::from_utf8_lossy(&resolved);
    Ok(Some(wsl_unc_path(&distro, resolved.trim_end())))
}

fn run_in_wsl(distro: &str, args: &[&str]) -> Result<Vec<u8>, RepoError> {
    let mut command = Command::new(WSL_PROGRAM);
    command
        .args([WSL_DISTRO_FLAG, distro, WSL_EXEC_FLAG])
        .args(args);
    hide_console_window(&mut command);
    let output = command.output()?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    Err(wsl_error(
        distro,
        args,
        String::from_utf8_lossy(&output.stderr).trim(),
    ))
}

fn wsl_error(distro: &str, args: &[&str], message: &str) -> RepoError {
    RepoError::Wsl {
        distro: distro.to_owned(),
        command: args.join(ARG_SEPARATOR),
        message: message.to_owned(),
    }
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}
