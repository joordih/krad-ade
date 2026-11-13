use krad_domain::review::{
    project_on_host, Approvals, Change, Commit, Discussion, DiscussionId, Issue, Job, JobId,
    JobLog, MergeOptions, MergeRequest, MergeRequestIid, MergeRequestQuery, NewMergeRequest, Note,
    Pipeline, PipelineId, ProjectPath, User,
};
use krad_domain::CodeHost;
use krad_github::{GithubClient, GithubError, GITHUB_HOST};
use krad_gitlab::{GitlabClient, GitlabConfig, GitlabError, OAuthError, TokenSaver};
use krad_host::HostLink;
use krad_repo::{resolved_remote_url, workplace_home, ORIGIN_REMOTE};
use krad_store::{read_github_login, read_gitlab_login, write_gitlab_login, LoginStoreError};
use std::fmt;
use std::path::Path;

mod watch;

pub(crate) use watch::{observe, CodeHostWatch, Observed, WatchTarget};

pub fn gitlab_token_saver(host: String) -> TokenSaver {
    Box::new(move |tokens| write_gitlab_login(&host, tokens).map_err(std::io::Error::other))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostLogin {
    Gitlab(GitlabConfig),
    Github,
}

impl HostLogin {
    #[must_use]
    pub const fn code_host(&self) -> CodeHost {
        match self {
            Self::Gitlab(_) => CodeHost::Gitlab,
            Self::Github => CodeHost::Github,
        }
    }

    /// # Errors
    ///
    /// Fails when the saved login cannot be read.
    pub fn client(&self) -> Result<Option<HostClient>, LoginStoreError> {
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
pub struct HostRepo {
    pub login: HostLogin,
    pub project: ProjectPath,
}

#[must_use]
pub fn repo_of_remote(remote: &str, gitlab: Option<GitlabConfig>) -> Option<HostRepo> {
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

#[must_use]
pub fn origin_repo(link: &HostLink, root: &Path, gitlab: Option<GitlabConfig>) -> Option<HostRepo> {
    let home = workplace_home(link, Some(root)).ok().flatten();
    let remote = resolved_remote_url(link, root, ORIGIN_REMOTE, home.as_deref())
        .ok()
        .flatten()?;
    repo_of_remote(&remote, gitlab)
}

pub enum HostClient {
    Gitlab(GitlabClient),
    Github(GithubClient),
}

#[derive(Debug)]
pub enum HostError {
    Gitlab(GitlabError),
    Github(GithubError),
}

impl HostError {
    #[must_use]
    pub const fn is_signed_out(&self) -> bool {
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
    #[must_use]
    pub const fn code_host(&self) -> CodeHost {
        match self {
            Self::Gitlab(_) => CodeHost::Gitlab,
            Self::Github(_) => CodeHost::Github,
        }
    }

    #[must_use]
    pub fn host(&self) -> &str {
        match self {
            Self::Gitlab(client) => client.host(),
            Self::Github(client) => client.host(),
        }
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn current_user(&self) -> Result<User, HostError> {
        on_host!(self, current_user())
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn issues(&self, project: &ProjectPath) -> Result<Vec<Issue>, HostError> {
        on_host!(self, issues(project))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn merge_requests(
        &self,
        project: &ProjectPath,
        query: MergeRequestQuery,
    ) -> Result<Vec<MergeRequest>, HostError> {
        on_host!(self, merge_requests(project, query))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn merge_request(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<MergeRequest, HostError> {
        on_host!(self, merge_request(project, iid))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn commits(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Commit>, HostError> {
        on_host!(self, commits(project, iid))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn changes(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Change>, HostError> {
        on_host!(self, changes(project, iid))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn discussions(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Discussion>, HostError> {
        on_host!(self, discussions(project, iid))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn add_note(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        body: &str,
    ) -> Result<Note, HostError> {
        on_host!(self, add_note(project, iid, body))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn reply(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        discussion: &DiscussionId,
        body: &str,
    ) -> Result<Note, HostError> {
        on_host!(self, reply(project, iid, discussion, body))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn resolve_discussion(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        discussion: &DiscussionId,
        resolved: bool,
    ) -> Result<(), HostError> {
        on_host!(self, resolve_discussion(project, iid, discussion, resolved))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn approvals(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Approvals, HostError> {
        on_host!(self, approvals(project, iid))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn approve(&self, project: &ProjectPath, iid: MergeRequestIid) -> Result<(), HostError> {
        on_host!(self, approve(project, iid))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn unapprove(&self, project: &ProjectPath, iid: MergeRequestIid) -> Result<(), HostError> {
        on_host!(self, unapprove(project, iid))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn merge(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        options: &MergeOptions,
    ) -> Result<MergeRequest, HostError> {
        on_host!(self, merge(project, iid, options))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn create_merge_request(
        &self,
        project: &ProjectPath,
        request: &NewMergeRequest,
    ) -> Result<MergeRequest, HostError> {
        match self {
            Self::Gitlab(client) => client
                .create_merge_request(project, request)
                .map_err(HostError::from),
            Self::Github(client) => client
                .create_pull_request(project, request)
                .map_err(HostError::from),
        }
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn pipelines(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Pipeline>, HostError> {
        on_host!(self, pipelines(project, iid))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn jobs(&self, project: &ProjectPath, pipeline: PipelineId) -> Result<Vec<Job>, HostError> {
        on_host!(self, jobs(project, pipeline))
    }

    /// # Errors
    ///
    /// Fails when the code host cannot be reached or refuses the request.
    pub fn job_log(&self, project: &ProjectPath, job: JobId) -> Result<JobLog, HostError> {
        on_host!(self, job_log(project, job))
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
