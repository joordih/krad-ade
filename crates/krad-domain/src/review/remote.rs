use std::borrow::Cow;

use super::{ProjectPath, PATH_SEPARATOR};

const SCHEME_SEPARATOR: &str = "://";
const SCP_HOST_SEPARATOR: char = ':';
const PORT_SEPARATOR: char = ':';
const USER_SEPARATOR: char = '@';
const GIT_SUFFIX: &str = ".git";
const HOST_KEYWORD: &str = "Host";
const HOST_NAME_KEYWORD: &str = "HostName";
const COMMENT_MARK: char = '#';
const OPTION_SEPARATOR: char = '=';

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RemoteProject {
    OnHost(ProjectPath),
    OtherHost(String),
    NotAProject,
}

#[must_use]
pub fn remote_project(remote_url: &str, host: &str) -> RemoteProject {
    let remote_url = remote_url.trim();
    let split = match remote_url.split_once(SCHEME_SEPARATOR) {
        Some((_, rest)) => rest.split_once(PATH_SEPARATOR),
        None => remote_url.split_once(SCP_HOST_SEPARATOR),
    };
    let Some((authority, path)) = split else {
        return RemoteProject::NotAProject;
    };
    let remote_host = hostname(authority);
    if !remote_host.eq_ignore_ascii_case(hostname(host)) {
        return RemoteProject::OtherHost(remote_host.to_owned());
    }
    let path = path.trim_end_matches(PATH_SEPARATOR);
    path.strip_suffix(GIT_SUFFIX)
        .unwrap_or(path)
        .parse()
        .map_or(RemoteProject::NotAProject, RemoteProject::OnHost)
}

#[must_use]
pub fn project_on_host(remote_url: &str, host: &str) -> Option<ProjectPath> {
    match remote_project(remote_url, host) {
        RemoteProject::OnHost(project) => Some(project),
        RemoteProject::OtherHost(_) | RemoteProject::NotAProject => None,
    }
}

#[must_use]
pub fn resolve_ssh_alias<'a>(remote_url: &'a str, ssh_config: &str) -> Cow<'a, str> {
    let authority = match remote_url.split_once(SCHEME_SEPARATOR) {
        Some((_, rest)) => rest.split(PATH_SEPARATOR).next().unwrap_or(rest),
        None => remote_url
            .split(SCP_HOST_SEPARATOR)
            .next()
            .unwrap_or(remote_url),
    };
    let alias = hostname(authority);
    let Some(real) = ssh_host_name(ssh_config, alias) else {
        return Cow::Borrowed(remote_url);
    };
    let at = remote_url.find(authority).map_or(0, |start| {
        start + authority.len() - alias.len() - port_len(authority)
    });
    Cow::Owned(format!(
        "{}{real}{}",
        &remote_url[..at],
        &remote_url[at + alias.len()..]
    ))
}

fn port_len(authority: &str) -> usize {
    let host = authority
        .rsplit_once(USER_SEPARATOR)
        .map_or(authority, |(_, host)| host);
    host.find(PORT_SEPARATOR)
        .map_or(0, |colon| host.len() - colon)
}

fn ssh_host_name(ssh_config: &str, alias: &str) -> Option<String> {
    let mut in_block = false;
    for line in ssh_config.lines() {
        let line = line.trim();
        if line.starts_with(COMMENT_MARK) {
            continue;
        }
        let Some((keyword, value)) = line
            .split_once(|c: char| c.is_whitespace() || c == OPTION_SEPARATOR)
            .map(|(keyword, value)| (keyword, value.trim_start_matches(OPTION_SEPARATOR).trim()))
        else {
            continue;
        };
        if keyword.eq_ignore_ascii_case(HOST_KEYWORD) {
            in_block = value.split_whitespace().any(|pattern| pattern == alias);
        } else if in_block && keyword.eq_ignore_ascii_case(HOST_NAME_KEYWORD) {
            return Some(value.to_owned());
        }
    }
    None
}

fn hostname(url: &str) -> &str {
    let url = url
        .split_once(SCHEME_SEPARATOR)
        .map_or(url, |(_, rest)| rest);
    let authority = url.split(PATH_SEPARATOR).next().unwrap_or(url);
    let host = authority
        .rsplit_once(USER_SEPARATOR)
        .map_or(authority, |(_, host)| host);
    host.split(PORT_SEPARATOR).next().unwrap_or(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remotes_on_the_host_name_their_project() {
        let host = "https://gitlab.example.test";
        let cases = [
            ("git@gitlab.example.test:team/app.git", Some("team/app")),
            (
                "https://gitlab.example.test/team/sub/app.git",
                Some("team/sub/app"),
            ),
            (
                "ssh://git@GitLab.example.test:2222/team/app",
                Some("team/app"),
            ),
            (
                "https://user@gitlab.example.test/team/app/",
                Some("team/app"),
            ),
            ("git@github.com:team/app.git", None),
            ("https://gitlab.example.test/", None),
            ("not a url", None),
        ];
        for (remote, expected) in cases {
            assert_eq!(
                project_on_host(remote, host).as_ref().map(AsRef::as_ref),
                expected,
                "remote {remote:?}"
            );
        }
        assert!(project_on_host("git@github.com:team/app.git", "github.com").is_some());
    }

    #[test]
    fn an_ssh_alias_stands_for_the_host_its_config_names() {
        let config = "# work account\nHost github-work\n  HostName github.com\n  User git\n\nHost other\n  HostName example.test\n";
        assert_eq!(
            resolve_ssh_alias("git@github-work:team/app.git", config),
            "git@github.com:team/app.git"
        );
        assert_eq!(
            resolve_ssh_alias("ssh://git@github-work:22/team/app", config),
            "ssh://git@github.com:22/team/app"
        );
        assert_eq!(
            resolve_ssh_alias("git@github.com:team/app.git", config),
            "git@github.com:team/app.git",
            "a real host stays"
        );
        let remote = resolve_ssh_alias("git@github-work:team/app.git", config);
        assert!(project_on_host(&remote, "github.com").is_some());
    }

    #[test]
    fn a_remote_off_the_host_names_its_host() {
        let host = "https://gitlab.example.test";
        assert_eq!(
            remote_project("git@github.com:team/app.git", host),
            RemoteProject::OtherHost("github.com".to_owned())
        );
        assert_eq!(
            remote_project("https://user@codeberg.org:443/team/app", host),
            RemoteProject::OtherHost("codeberg.org".to_owned())
        );
        assert_eq!(
            remote_project("/srv/git/app.git", host),
            RemoteProject::NotAProject
        );
    }
}
