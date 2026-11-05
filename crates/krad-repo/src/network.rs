use krad_host::HostLink;
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::git::run_git_within;
use crate::process::git_path;
use crate::RepoError;

const NETWORK_WAIT: Duration = Duration::from_mins(10);
const GIT_ASKPASS_VAR: &str = "GIT_ASKPASS";
const SSH_ASKPASS_VAR: &str = "SSH_ASKPASS";
const SSH_ASKPASS_REQUIRE_VAR: &str = "SSH_ASKPASS_REQUIRE";
const SSH_ASKPASS_FORCED: &str = "force";
const DISPLAY_VAR: &str = "DISPLAY";
const PLACEHOLDER_DISPLAY: &str = "krad-ade:0";
const FETCH_ARGS: [&str; 2] = ["fetch", "--prune"];
const PULL_ARGS: [&str; 2] = ["pull", "--no-edit"];
const PUSH: &str = "push";
const SET_UPSTREAM_FLAG: &str = "--set-upstream";
const PUSH_TAGS_ARGS: [&str; 2] = ["push", "--tags"];
const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Default)]
pub struct Askpass {
    vars: Vec<(String, OsString)>,
    program: Option<PathBuf>,
}

impl Askpass {
    #[must_use]
    pub fn new(program: PathBuf, bus: Vec<(String, String)>) -> Self {
        Self {
            vars: bus
                .into_iter()
                .map(|(name, value)| (name, value.into()))
                .collect(),
            program: Some(program),
        }
    }

    fn env(&self, host: &HostLink, repo: &Path) -> Vec<(String, OsString)> {
        let Some(program) = &self.program else {
            return Vec::new();
        };
        let program = git_path(host, repo, program);
        let mut env = self.vars.clone();
        env.extend([
            (GIT_ASKPASS_VAR.to_owned(), program.clone()),
            (SSH_ASKPASS_VAR.to_owned(), program),
            (
                SSH_ASKPASS_REQUIRE_VAR.to_owned(),
                SSH_ASKPASS_FORCED.into(),
            ),
        ]);
        if !host.is_local() || std::env::var_os(DISPLAY_VAR).is_none() {
            env.push((DISPLAY_VAR.to_owned(), PLACEHOLDER_DISPLAY.into()));
        }
        env
    }
}

impl fmt::Debug for Askpass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PushTarget {
    Upstream,
    Publish { remote: String, branch: String },
}

fn network<S: AsRef<std::ffi::OsStr>>(
    host: &HostLink,
    repo: &Path,
    args: &[S],
    askpass: &Askpass,
) -> Result<(), RepoError> {
    run_git_within(host, repo, args, &askpass.env(host, repo), NETWORK_WAIT).map(drop)
}

/// # Errors
///
/// Returns [`RepoError::Git`] when a remote cannot be reached or refuses
/// the sign-in, and [`RepoError::Host`] when it takes too long.
pub fn fetch(host: &HostLink, repo: &Path, askpass: &Askpass) -> Result<(), RepoError> {
    network(host, repo, &FETCH_ARGS, askpass)
}

/// # Errors
///
/// As [`fetch`], and when the branch has no upstream or the merge conflicts.
pub fn pull(host: &HostLink, repo: &Path, askpass: &Askpass) -> Result<(), RepoError> {
    network(host, repo, &PULL_ARGS, askpass)
}

/// # Errors
///
/// As [`fetch`], and when the remote rejects the push.
pub fn push(
    host: &HostLink,
    repo: &Path,
    target: &PushTarget,
    askpass: &Askpass,
) -> Result<(), RepoError> {
    match target {
        PushTarget::Upstream => network(host, repo, &[PUSH], askpass),
        PushTarget::Publish { remote, branch } => network(
            host,
            repo,
            &[PUSH, SET_UPSTREAM_FLAG, remote.as_str(), branch.as_str()],
            askpass,
        ),
    }
}

/// # Errors
///
/// As [`pull`] and [`push`].
pub fn sync(
    host: &HostLink,
    repo: &Path,
    target: &PushTarget,
    askpass: &Askpass,
) -> Result<(), RepoError> {
    if *target == PushTarget::Upstream {
        pull(host, repo, askpass)?;
    }
    push(host, repo, target, askpass)
}

/// # Errors
///
/// As [`push`].
pub fn push_tags(host: &HostLink, repo: &Path, askpass: &Askpass) -> Result<(), RepoError> {
    network(host, repo, &PUSH_TAGS_ARGS, askpass)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_askpass_points_git_and_ssh_at_the_program_and_never_prints() {
        let askpass = Askpass::new(
            PathBuf::from("/cfg/bin/krad-ade-askpass"),
            vec![("KRAD_ADE_TOKEN".to_owned(), "secret".to_owned())],
        );
        let env = askpass.env(&HostLink::Local, Path::new("/src/app"));
        let value = |name: &str| {
            env.iter()
                .find(|(var, _)| *var == name)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(
            value(GIT_ASKPASS_VAR),
            Some(OsString::from("/cfg/bin/krad-ade-askpass"))
        );
        assert_eq!(value(SSH_ASKPASS_VAR), value(GIT_ASKPASS_VAR));
        assert_eq!(
            value(SSH_ASKPASS_REQUIRE_VAR),
            Some(OsString::from(SSH_ASKPASS_FORCED))
        );
        assert_eq!(value("KRAD_ADE_TOKEN"), Some(OsString::from("secret")));
        assert_eq!(format!("{askpass:?}"), REDACTED);
        assert_eq!(
            Askpass::default()
                .env(&HostLink::Local, Path::new("/src/app"))
                .len(),
            0
        );
    }
}
