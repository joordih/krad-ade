use std::path::PathBuf;

use super::{lock, SshLink};
use crate::command::{env_file_text, is_env_name, HostCommand};
use crate::error::{HostError, Waited};
use crate::mode::FileMode;

const MARKER: &str = "KRAD_ADE_LOGIN_ENV_8f2c41";
const CAPTURE_SCRIPT: &str = r#"m="$1"; s="${SHELL:-/bin/sh}"
case "${s##*/}" in
  csh|tcsh) set -- -c ;;
  fish|nu|elvish|xonsh) set -- -l -c ;;
  *) set -- -l -i -c ;;
esac
out=$("$s" "$@" "printf %s $m; /usr/bin/env -0" </dev/null 2>/dev/null)
case "$out" in
  *"$m"*) printf %s "$out" ;;
  *) /bin/sh -l -c "printf %s $m; /usr/bin/env -0" </dev/null 2>/dev/null ;;
esac"#;
const NUL: char = '\0';
const ASSIGN: char = '=';
const VOLATILE: [&str; 10] = [
    "PWD", "OLDPWD", "SHLVL", "_", "PS1", "PS2", "TERM", "COLUMNS", "LINES", "TMOUT",
];
const SESSION_PREFIX: &str = "SSH_";
const MAX_VARS: usize = 4096;

pub(crate) fn parse(output: &str) -> Vec<(String, String)> {
    let Some((_, listed)) = output.split_once(MARKER) else {
        return Vec::new();
    };
    listed
        .split(NUL)
        .filter_map(|entry| entry.split_once(ASSIGN))
        .filter(|(name, _)| {
            is_env_name(name) && !VOLATILE.contains(name) && !name.starts_with(SESSION_PREFIX)
        })
        .take(MAX_VARS)
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
}

impl SshLink {
    pub(crate) fn login_env_file(&self) -> Option<PathBuf> {
        lock(&self.login_env).clone()
    }

    /// # Errors
    #[tracing::instrument(skip_all, fields(destination = %self.args.destination))]
    pub fn refresh_environment(&self) -> Result<(), HostError> {
        let mut capture = HostCommand::posix_shell(CAPTURE_SCRIPT);
        capture.arg(MARKER);
        let output = self.run_plain(&capture, Waited::Command)?;
        let vars = parse(&String::from_utf8_lossy(&output.stdout));
        if vars.is_empty() {
            tracing::warn!(
                "the login shell printed no environment; commands use the plain ssh one"
            );
            lock(&self.login_env).take();
            return Ok(());
        }
        let layout = self.layout().ok_or(HostError::NotConnected)?;
        let file = layout.login_env_file()?;
        if let Some(dir) = file.parent() {
            self.create_dir_with_mode(dir, FileMode::PRIVATE_DIR)?;
        }
        self.write_with_mode(
            &file,
            env_file_text(&vars)?.as_bytes(),
            FileMode::PRIVATE_FILE,
        )?;
        tracing::debug!(vars = vars.len(), "captured the host's login environment");
        *lock(&self.login_env) = Some(file);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listing_after_the_marker_becomes_safe_assignments() {
        let output = format!(
            "motd noise\n{MARKER}PATH=/usr/local/bin:/usr/bin\0HOME=/home/me\0PWD=/tmp\0SSH_AUTH_SOCK=/tmp/a\0BASH_FUNC_x%%=() {{ :; }}\0MULTI=a\nb\0"
        );
        assert_eq!(
            parse(&output),
            [
                ("PATH".to_owned(), "/usr/local/bin:/usr/bin".to_owned()),
                ("HOME".to_owned(), "/home/me".to_owned()),
                ("MULTI".to_owned(), "a\nb".to_owned()),
            ]
        );
        assert_eq!(parse("no marker here"), Vec::new());
    }

    #[cfg(unix)]
    #[test]
    fn the_capture_script_runs_under_dash_with_a_fish_like_shell_missing() -> std::io::Result<()> {
        for shell in ["/bin/sh", "/bin/dash"] {
            if !std::path::Path::new(shell).exists() {
                continue;
            }
            let output = std::process::Command::new(shell)
                .args(["-c", CAPTURE_SCRIPT, "sh", MARKER])
                .env("SHELL", "/nonexistent/fish")
                .output()?;
            let vars = parse(&String::from_utf8_lossy(&output.stdout));
            assert!(vars.iter().any(|(name, _)| name == "PATH"), "{shell}");
        }
        Ok(())
    }
}
