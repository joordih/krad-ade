use krad_domain::review::{
    project_on_host, Approvals, Change, Commit, Discussion, DiscussionId, Job, MergeOptions,
    MergeRequest, MergeRequestIid, MergeRequestQuery, Note, Pipeline, PipelineId, ProjectPath,
    User,
};
use krad_domain::CodeHost;
use krad_github::{GithubClient, GithubError, GITHUB_HOST};
use krad_gitlab::{GitlabClient, GitlabConfig, GitlabError, OAuthError};
use krad_repo::{resolved_remote_url, ORIGIN_REMOTE};
use krad_store::{read_github_login, read_gitlab_login, LoginStoreError};
use std::fmt;
use std::path::Path;

use crate::environment_shell::chat::workplace_home;
use crate::environment_shell::settings_view::gitlab_token_saver;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum HostLogin {
    Gitlab(GitlabConfig),
    Github,
}

impl HostLogin {
    pub(in crate::environment_shell) const fn code_host(&self) -> CodeHost {
        match self {
            Self::Gitlab(_) => CodeHost::Gitlab,
            Self::Github => CodeHost::Github,
        }
    }

    pub(in crate::environment_shell) fn client(
        &self,
    ) -> Result<Option<HostClient>, LoginStoreError> {
        Ok(match self {
            Self::Gitlab(config) => read_gitlab_login(&config.host)?.map(|tokens| {
                let saver = gitlab_token_saver(config.host.clone());
                HostClient::Gitlab(GitlabClient::new(config.clone(), tokens, saver))
            }),
            Self::Github => {
                read_github_login()?.map(|token| HostClient::Github(GithubClient::new(&token)))
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct HostRepo {
    pub(in crate::environment_shell) login: HostLogin,
    pub(in crate::environment_shell) project: ProjectPath,
}

fn repo_of_remote(remote: &str, gitlab: Option<GitlabConfig>) -> Option<HostRepo> {
    let on_gitlab = gitlab.and_then(|config| {
        Some(HostRepo {
            project: project_on_host(remote, &config.host)?,
            login: HostLogin::Gitlab(config),
        })
    });
    on_gitlab.or_else(|| {
        Some(HostRepo {
            project: project_on_host(remote, GITHUB_HOST)?,
            login: HostLogin::Github,
        })
    })
}

pub(in crate::environment_shell) fn origin_repo(
    root: &Path,
    gitlab: Option<GitlabConfig>,
) -> Option<HostRepo> {
    let home = workplace_home(Some(root)).ok().flatten();
    let remote = resolved_remote_url(root, ORIGIN_REMOTE, home.as_deref())
        .ok()
        .flatten()?;
    repo_of_remote(&remote, gitlab)
}

pub(in crate::environment_shell) enum HostClient {
    Gitlab(GitlabClient),
    Github(GithubClient),
}

#[derive(Debug)]
pub(in crate::environment_shell) enum HostError {
    Gitlab(GitlabError),
    Github(GithubError),
}

impl HostError {
    pub(in crate::environment_shell) const fn is_signed_out(&self) -> bool {
        matches!(
            self,
            Self::Gitlab(GitlabError::Login(OAuthError::Expired))
                | Self::Github(GithubError::SignedOut)
        )
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gitlab(error) => error.fmt(f),
            Self::Github(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for HostError {}

impl From<GitlabError> for HostError {
    fn from(error: GitlabError) -> Self {
        Self::Gitlab(error)
    }
}

impl From<GithubError> for HostError {
    fn from(error: GithubError) -> Self {
        Self::Github(error)
    }
}

macro_rules! on_host {
    ($client:expr, $method:ident($($arg:expr),*)) => {
        match $client {
            HostClient::Gitlab(client) => client.$method($($arg),*).map_err(HostError::from),
            HostClient::Github(client) => client.$method($($arg),*).map_err(HostError::from),
        }
    };
}

impl HostClient {
    pub(in crate::environment_shell) const fn code_host(&self) -> CodeHost {
        match self {
            Self::Gitlab(_) => CodeHost::Gitlab,
            Self::Github(_) => CodeHost::Github,
        }
    }

    pub(in crate::environment_shell) fn host(&self) -> &str {
        match self {
            Self::Gitlab(client) => client.host(),
            Self::Github(client) => client.host(),
        }
    }

    pub(in crate::environment_shell) fn current_user(&self) -> Result<User, HostError> {
        on_host!(self, current_user())
    }

    pub(in crate::environment_shell) fn merge_requests(
        &self,
        project: &ProjectPath,
        query: MergeRequestQuery,
    ) -> Result<Vec<MergeRequest>, HostError> {
        on_host!(self, merge_requests(project, query))
    }

    pub(in crate::environment_shell) fn merge_request(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<MergeRequest, HostError> {
        on_host!(self, merge_request(project, iid))
    }

    pub(in crate::environment_shell) fn commits(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Commit>, HostError> {
        on_host!(self, commits(project, iid))
    }

    pub(in crate::environment_shell) fn changes(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Change>, HostError> {
        on_host!(self, changes(project, iid))
    }

    pub(in crate::environment_shell) fn discussions(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Discussion>, HostError> {
        on_host!(self, discussions(project, iid))
    }

    pub(in crate::environment_shell) fn add_note(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        body: &str,
    ) -> Result<Note, HostError> {
        on_host!(self, add_note(project, iid, body))
    }

    pub(in crate::environment_shell) fn reply(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        discussion: &DiscussionId,
        body: &str,
    ) -> Result<Note, HostError> {
        on_host!(self, reply(project, iid, discussion, body))
    }

    pub(in crate::environment_shell) fn resolve_discussion(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        discussion: &DiscussionId,
        resolved: bool,
    ) -> Result<(), HostError> {
        on_host!(self, resolve_discussion(project, iid, discussion, resolved))
    }

    pub(in crate::environment_shell) fn approvals(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Approvals, HostError> {
        on_host!(self, approvals(project, iid))
    }

    pub(in crate::environment_shell) fn approve(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<(), HostError> {
        on_host!(self, approve(project, iid))
    }

    pub(in crate::environment_shell) fn unapprove(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<(), HostError> {
        on_host!(self, unapprove(project, iid))
    }

    pub(in crate::environment_shell) fn merge(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        options: &MergeOptions,
    ) -> Result<MergeRequest, HostError> {
        on_host!(self, merge(project, iid, options))
    }

    pub(in crate::environment_shell) fn pipelines(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Pipeline>, HostError> {
        on_host!(self, pipelines(project, iid))
    }

    pub(in crate::environment_shell) fn jobs(
        &self,
        project: &ProjectPath,
        pipeline: PipelineId,
    ) -> Result<Vec<Job>, HostError> {
        on_host!(self, jobs(project, pipeline))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::GitlabSettings;

    #[test]
    fn a_remote_is_on_the_gitlab_in_settings_or_on_github() -> Result<(), krad_gitlab::ConfigError>
    {
        let gitlab = GitlabConfig::try_from(&GitlabSettings {
            host: "gitlab.example.test".to_owned(),
            client_id: "app".to_owned(),
            ..GitlabSettings::default()
        })?;
        let host_of = |remote: &str, gitlab: Option<GitlabConfig>| {
            repo_of_remote(remote, gitlab).map(|repo| repo.login.code_host())
        };
        assert_eq!(
            host_of("git@gitlab.example.test:team/app.git", Some(gitlab.clone())),
            Some(CodeHost::Gitlab)
        );
        assert_eq!(
            host_of("git@github.com:octo/app.git", Some(gitlab)),
            Some(CodeHost::Github)
        );
        assert_eq!(host_of("git@gitlab.example.test:team/app.git", None), None);
        assert_eq!(host_of("/srv/git/app.git", None), None);
        Ok(())
    }
}
