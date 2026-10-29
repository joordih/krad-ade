use gpui_kit::{AppContext as _, Context};
use krad_domain::review::{remote_project, ProjectPath, RemoteProject};
use krad_github::GITHUB_HOST;
use krad_repo::{resolved_remote_url, ORIGIN_REMOTE};
use std::fmt;
use std::path::PathBuf;

use crate::environment_shell::chat::workplace_home;
use crate::environment_shell::EnvironmentShell;

pub(in crate::environment_shell::settings_view) struct GroupRemote {
    pub(in crate::environment_shell::settings_view) name: String,
    pub(in crate::environment_shell::settings_view) origin: Result<Option<String>, String>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(in crate::environment_shell::settings_view) enum RemoteMatch {
    Gitlab(ProjectPath),
    Github(ProjectPath),
    NoOrigin,
    OtherHost(String),
    NotAProject,
    Unreadable(String),
}

impl RemoteMatch {
    pub(in crate::environment_shell::settings_view) fn of(
        origin: &Result<Option<String>, String>,
        gitlab_host: &str,
    ) -> Self {
        let url = match origin {
            Err(error) => return Self::Unreadable(error.clone()),
            Ok(None) => return Self::NoOrigin,
            Ok(Some(url)) => url,
        };
        let gitlab_host = gitlab_host.trim();
        if !gitlab_host.is_empty() {
            if let RemoteProject::OnHost(project) = remote_project(url, gitlab_host) {
                return Self::Gitlab(project);
            }
        }
        match remote_project(url, GITHUB_HOST) {
            RemoteProject::OnHost(project) => Self::Github(project),
            RemoteProject::OtherHost(other) => Self::OtherHost(other),
            RemoteProject::NotAProject => Self::NotAProject,
        }
    }

    pub(in crate::environment_shell::settings_view) const fn is_project(&self) -> bool {
        matches!(self, Self::Gitlab(_) | Self::Github(_))
    }
}

impl fmt::Display for RemoteMatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gitlab(project) => write!(f, "GitLab · {project}"),
            Self::Github(project) => write!(f, "GitHub · {project}"),
            Self::NoOrigin => f.write_str("no origin remote"),
            Self::OtherHost(other) => {
                write!(
                    f,
                    "origin is on {other}, not GitHub or the GitLab host above"
                )
            }
            Self::NotAProject => f.write_str("origin does not name a project"),
            Self::Unreadable(error) => f.write_str(error),
        }
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn load_group_remotes(&mut self, cx: &mut Context<Self>) {
        let groups: Vec<(String, PathBuf)> = self.envs.get(&self.active).map_or_default(|live| {
            live.data
                .groups
                .iter()
                .map(|group| (group.name.clone(), group.root.clone()))
                .collect()
        });
        let read = cx.background_spawn(async move {
            groups
                .into_iter()
                .map(|(name, root)| GroupRemote {
                    name,
                    origin: {
                        let home = workplace_home(Some(&root)).ok().flatten();
                        resolved_remote_url(&root, ORIGIN_REMOTE, home.as_deref())
                            .map_err(|error| error.to_string())
                    },
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let remotes = read.await;
            this.update(cx, |shell, cx| {
                if let Some(gitlab) = shell.gitlab_ui_mut() {
                    gitlab.remotes = Some(remotes);
                    cx.notify();
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: &str = "https://gitlab.example.test";

    fn reason(origin: Result<Option<&str>, &str>, host: &str) -> String {
        let origin = origin
            .map(|url| url.map(str::to_owned))
            .map_err(str::to_owned);
        RemoteMatch::of(&origin, host).to_string()
    }

    #[test]
    fn a_remote_on_the_host_shows_its_project() {
        assert_eq!(
            reason(Ok(Some("git@gitlab.example.test:acme/portfolio.git")), HOST),
            "GitLab · acme/portfolio"
        );
        assert_eq!(
            reason(Ok(Some("git@github.com:acme/website.git")), " "),
            "GitHub · acme/website",
            "a GitHub remote needs no host"
        );
    }

    #[test]
    fn a_remote_that_does_not_match_says_why() {
        assert_eq!(
            reason(Ok(Some("git@codeberg.org:acme/website.git")), HOST),
            "origin is on codeberg.org, not GitHub or the GitLab host above"
        );
        assert_eq!(reason(Ok(None), HOST), "no origin remote");
        assert_eq!(
            reason(Ok(Some("/srv/git/app.git")), HOST),
            "origin does not name a project"
        );
        assert_eq!(
            reason(Err("not a git repository"), HOST),
            "not a git repository"
        );
        assert_eq!(
            reason(Ok(Some("git@gitlab.example.test:acme/app.git")), " "),
            "origin is on gitlab.example.test, not GitHub or the GitLab host above"
        );
    }
}
