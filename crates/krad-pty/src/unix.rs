use std::io::Read as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const EXECUTE_BITS: u32 = 0o111;
const SHELL_ENV: &str = "SHELL";
const FALLBACK_SHELL: &str = "/bin/sh";
const COMMAND_FLAG: &str = "-c";
const SPAM_SCRIPT: &str = "while :; do date; seq 1 200; done";
const POSIX_CLEAR: &str = "printf '\\033[H\\033[2J\\033[3J'; ";
const LOGIN_FLAGS: [&str; 3] = ["-l", "-i", "-c"];
const ENV_MARKER: &str = "KRAD_LOGIN_ENV";
const ENV_TIMEOUT: Duration = Duration::from_secs(5);
const SHELL_OWN_VARS: [&str; 4] = ["PWD", "OLDPWD", "SHLVL", "_"];

#[must_use]
pub fn default_shell() -> (String, Vec<String>) {
    match std::env::var(SHELL_ENV) {
        Ok(shell) if !shell.is_empty() => (shell, Vec::new()),
        _ => (FALLBACK_SHELL.to_owned(), Vec::new()),
    }
}

#[must_use]
pub fn log_spammer() -> (String, Vec<String>) {
    (
        FALLBACK_SHELL.to_owned(),
        vec![COMMAND_FLAG.to_owned(), SPAM_SCRIPT.to_owned()],
    )
}

pub const SHELL_QUOTING: krad_domain::ShellQuoting = krad_domain::ShellQuoting::Posix;

pub(crate) fn executable_names(name: &str) -> Vec<String> {
    vec![name.to_owned()]
}

pub(crate) fn is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & EXECUTE_BITS != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn only_files_with_an_execute_bit_count_as_programs() -> std::io::Result<()> {
        let dir = std::env::temp_dir().join(format!("krad-pty-exec-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let script = dir.join("agent");
        fs::write(&script, "#!/bin/sh\n")?;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o644))?;
        let readable_only = is_executable(&script);
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755))?;
        let executable = is_executable(&script);
        let folder = is_executable(&dir);
        fs::remove_dir_all(&dir)?;
        assert!(!readable_only);
        assert!(executable);
        assert!(!folder);
        assert!(crate::find_program("sh").is_some());
        Ok(())
    }
}

fn env_dump_script() -> String {
    format!("printf '\\0{ENV_MARKER}\\0'; /usr/bin/env -0")
}

fn parse_env_dump(dump: &[u8]) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(dump);
    let marker = format!("\0{ENV_MARKER}\0");
    let Some((_, entries)) = text.split_once(&marker) else {
        return Vec::new();
    };
    entries
        .split('\0')
        .filter_map(|entry| entry.split_once('='))
        .filter(|(name, _)| !name.is_empty() && !SHELL_OWN_VARS.contains(name))
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
}

#[must_use]
pub fn login_shell_env() -> Vec<(String, String)> {
    let (shell, _) = default_shell();
    let Ok(mut child) = Command::new(shell)
        .args(LOGIN_FLAGS)
        .arg(env_dump_script())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };
    let Some(mut stdout) = child.stdout.take() else {
        return Vec::new();
    };
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut dump = Vec::new();
        let read = stdout.read_to_end(&mut dump).map(|_| dump);
        sender.send(read).unwrap_or_default();
    });
    let dump = receiver.recv_timeout(ENV_TIMEOUT);
    child.kill().unwrap_or_default();
    drop(child.wait());
    match dump {
        Ok(Ok(dump)) => parse_env_dump(&dump),
        _ => Vec::new(),
    }
}

#[must_use]
pub const fn clear_screen_prefix(_in_wsl: bool) -> &'static str {
    POSIX_CLEAR
}

#[cfg(test)]
mod login_env_tests {
    use super::*;

    #[test]
    fn only_what_follows_the_marker_is_taken_without_the_shells_own_vars() {
        let dump = format!("profile noise\0{ENV_MARKER}\0A=1\0PWD=/x\0B=two=parts\0");
        assert_eq!(
            parse_env_dump(dump.as_bytes()),
            [
                ("A".to_owned(), "1".to_owned()),
                ("B".to_owned(), "two=parts".to_owned())
            ]
        );
        assert_eq!(parse_env_dump(b"no marker"), Vec::new());
    }
}
