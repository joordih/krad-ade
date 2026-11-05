use krad_host::HostLink;
use std::fmt;
use std::path::Path;

use crate::git::run_git;
use crate::RepoError;

const LIST_ARGS: [&str; 6] = [
    "for-each-ref",
    "--format=%(refname)%00%(upstream:short)%00%(HEAD)",
    "--sort=refname",
    "refs/heads",
    "refs/remotes",
    "refs/tags",
];
const FIELD_SEPARATOR: char = '\0';
const LOCAL_PREFIX: &str = "refs/heads/";
const REMOTE_PREFIX: &str = "refs/remotes/";
const TAG_PREFIX: &str = "refs/tags/";
const REMOTE_HEAD: &str = "HEAD";
const HEAD_MARK: &str = "*";
const REMOTE_SEPARATOR: char = '/';
const SWITCH: &str = "switch";
const CREATE_FLAG: &str = "--create";
const TRACK_FLAG: &str = "--track";
const TAG: &str = "tag";
const DELETE_FLAG: &str = "--delete";
const ALREADY_EXISTS: &str = "already exists";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BranchPlace {
    Local,
    Remote(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Branch {
    pub name: String,
    pub place: BranchPlace,
    pub upstream: Option<String>,
    pub is_head: bool,
}

impl fmt::Display for Branch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.place {
            BranchPlace::Local => f.write_str(&self.name),
            BranchPlace::Remote(remote) => write!(f, "{remote}{REMOTE_SEPARATOR}{}", self.name),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Refs {
    pub branches: Vec<Branch>,
    pub tags: Vec<String>,
}

impl Refs {
    pub fn local(&self) -> impl Iterator<Item = &Branch> {
        self.branches
            .iter()
            .filter(|branch| branch.place == BranchPlace::Local)
    }

    pub fn remote(&self) -> impl Iterator<Item = &Branch> {
        self.branches
            .iter()
            .filter(|branch| matches!(branch.place, BranchPlace::Remote(_)))
    }
}

/// # Errors
///
/// Returns [`RepoError::Git`] when `repo` is not a git repository.
pub fn list_refs(host: &HostLink, repo: &Path) -> Result<Refs, RepoError> {
    let output = run_git(host, repo, &LIST_ARGS)?;
    Ok(parse_refs(&String::from_utf8_lossy(&output)))
}

fn parse_refs(output: &str) -> Refs {
    let mut refs = Refs::default();
    for line in output.lines() {
        let mut fields = line.split(FIELD_SEPARATOR);
        let (Some(name), upstream, head) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        let upstream = upstream.filter(|upstream| !upstream.is_empty());
        if let Some(local) = name.strip_prefix(LOCAL_PREFIX) {
            refs.branches.push(Branch {
                name: local.to_owned(),
                place: BranchPlace::Local,
                upstream: upstream.map(str::to_owned),
                is_head: head == Some(HEAD_MARK),
            });
        } else if let Some(remote) = name.strip_prefix(REMOTE_PREFIX) {
            let Some((remote, branch)) = remote.split_once(REMOTE_SEPARATOR) else {
                continue;
            };
            if branch == REMOTE_HEAD {
                continue;
            }
            refs.branches.push(Branch {
                name: branch.to_owned(),
                place: BranchPlace::Remote(remote.to_owned()),
                upstream: None,
                is_head: false,
            });
        } else if let Some(tag) = name.strip_prefix(TAG_PREFIX) {
            refs.tags.push(tag.to_owned());
        }
    }
    refs
}

/// # Errors
///
/// Returns [`RepoError::Git`] when git refuses the switch, as with
/// uncommitted changes the other branch would overwrite.
pub fn check_out(host: &HostLink, repo: &Path, branch: &Branch) -> Result<(), RepoError> {
    match &branch.place {
        BranchPlace::Local => run_git(host, repo, &[SWITCH, branch.name.as_str()]).map(drop),
        BranchPlace::Remote(_) => {
            let tracked = branch.to_string();
            match run_git(host, repo, &[SWITCH, TRACK_FLAG, tracked.as_str()]) {
                Err(RepoError::Git { message, .. }) if message.contains(ALREADY_EXISTS) => {
                    run_git(host, repo, &[SWITCH, branch.name.as_str()]).map(drop)
                }
                switched => switched.map(drop),
            }
        }
    }
}

/// # Errors
///
/// Returns [`RepoError::Git`] when the name is not a valid branch name or
/// a branch of that name exists.
pub fn create_branch(host: &HostLink, repo: &Path, name: &str) -> Result<(), RepoError> {
    run_git(host, repo, &[SWITCH, CREATE_FLAG, name]).map(drop)
}

/// # Errors
///
/// Returns [`RepoError::Git`] when the name is not a valid tag name or a
/// tag of that name exists.
pub fn create_tag(host: &HostLink, repo: &Path, name: &str) -> Result<(), RepoError> {
    run_git(host, repo, &[TAG, name]).map(drop)
}

/// # Errors
///
/// Returns [`RepoError::Git`] when no tag has that name.
pub fn delete_tag(host: &HostLink, repo: &Path, name: &str) -> Result<(), RepoError> {
    run_git(host, repo, &[TAG, DELETE_FLAG, name]).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_split_into_local_and_remote_branches_and_tags() {
        let output = [
            "refs/heads/main\0origin/main\0*",
            "refs/heads/fix\0\0 ",
            "refs/remotes/origin/HEAD\0\0 ",
            "refs/remotes/origin/main\0\0 ",
            "refs/remotes/upstream/feat/login\0\0 ",
            "refs/tags/v1.0\0\0 ",
        ]
        .join("\n");

        let refs = parse_refs(&output);

        assert_eq!(
            refs.branches,
            [
                Branch {
                    name: "main".to_owned(),
                    place: BranchPlace::Local,
                    upstream: Some("origin/main".to_owned()),
                    is_head: true,
                },
                Branch {
                    name: "fix".to_owned(),
                    place: BranchPlace::Local,
                    upstream: None,
                    is_head: false,
                },
                Branch {
                    name: "main".to_owned(),
                    place: BranchPlace::Remote("origin".to_owned()),
                    upstream: None,
                    is_head: false,
                },
                Branch {
                    name: "feat/login".to_owned(),
                    place: BranchPlace::Remote("upstream".to_owned()),
                    upstream: None,
                    is_head: false,
                },
            ]
        );
        assert_eq!(refs.tags, ["v1.0"]);
        assert_eq!(refs.remote().count(), 2);
        assert_eq!(
            refs.remote().last().map(ToString::to_string).as_deref(),
            Some("upstream/feat/login")
        );
    }
}
