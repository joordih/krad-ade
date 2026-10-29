use crate::config::GitlabConfig;
use crate::encode::percent_encode;
use crate::model::{
    scope_param, state_param, ApprovalsJson, BranchJson, ChangeJson, CommitJson, DiscussionJson,
    JobJson, MergeRequestBody, MergeRequestJson, NoteJson, NoteRequest, PipelineJson,
    ResolveRequest, TreeEntryJson, UserJson,
};
use crate::oauth::{self, oauth_agent, OAuthError};
use krad_domain::review::{
    Approvals, Branch, Change, Commit, Discussion, DiscussionId, Job, MergeOptions, MergeRequest,
    MergeRequestIid, MergeRequestQuery, Note, Pipeline, PipelineId, ProjectPath, TreeEntry, User,
};
use krad_domain::GitlabTokens;
use serde::de::DeserializeOwned;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;
use ureq::Agent;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const UNAUTHORIZED: u16 = 401;
const AUTHORIZATION: &str = "Authorization";
const BEARER: &str = "Bearer ";
const NEXT_PAGE_HEADER: &str = "x-next-page";
const PAGE_PARAM: &str = "page";
const PER_PAGE_PARAM: &str = "per_page";
const FIRST_PAGE: &str = "1";
const PER_PAGE: &str = "100";
const STATE_PARAM: &str = "state";
const SCOPE_PARAM: &str = "scope";
const REVIEWER_PARAM: &str = "reviewer_id";
const REF_PARAM: &str = "ref";
const PATH_PARAM: &str = "path";
const USER_PATH: &str = "/user";
const PROJECTS_PATH: &str = "/projects";
const MERGE_REQUESTS_PATH: &str = "/merge_requests";
const COMMITS_PATH: &str = "/commits";
const DIFFS_PATH: &str = "/diffs";
const DISCUSSIONS_PATH: &str = "/discussions";
const NOTES_PATH: &str = "/notes";
const APPROVALS_PATH: &str = "/approvals";
const APPROVE_PATH: &str = "/approve";
const UNAPPROVE_PATH: &str = "/unapprove";
const MERGE_PATH: &str = "/merge";
const PIPELINES_PATH: &str = "/pipelines";
const JOBS_PATH: &str = "/jobs";
const BRANCHES_PATH: &str = "/repository/branches";
const TREE_PATH: &str = "/repository/tree";
const FILES_PATH: &str = "/repository/files";
const RAW_PATH: &str = "/raw";

pub type TokenSaver = Box<dyn Fn(&GitlabTokens) -> std::io::Result<()> + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum GitlabError {
    #[error(transparent)]
    Login(#[from] OAuthError),
    #[error("Could not save the refreshed GitLab login: {0}")]
    SaveTokens(std::io::Error),
    #[error("GitLab request failed: {0}")]
    Http(#[from] ureq::Error),
}

pub struct GitlabClient {
    agent: Agent,
    oauth: Agent,
    config: GitlabConfig,
    tokens: Mutex<GitlabTokens>,
    save: TokenSaver,
}

impl GitlabClient {
    #[must_use]
    pub fn new(config: GitlabConfig, tokens: GitlabTokens, save: TokenSaver) -> Self {
        let agent = Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build()
            .new_agent();
        Self {
            agent,
            oauth: oauth_agent(),
            config,
            tokens: Mutex::new(tokens),
            save,
        }
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.config.host
    }

    /// # Errors
    ///
    /// [`GitlabError::Login`] when the login cannot be refreshed,
    /// [`GitlabError::SaveTokens`] when a refreshed one cannot be saved, and
    /// [`GitlabError::Http`] when GitLab cannot be reached or refuses.
    pub fn current_user(&self) -> Result<User, GitlabError> {
        self.get::<UserJson, _>(USER_PATH)
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn merge_requests(
        &self,
        project: &ProjectPath,
        query: MergeRequestQuery,
    ) -> Result<Vec<MergeRequest>, GitlabError> {
        let mut params = vec![
            (STATE_PARAM, state_param(query.state).to_owned()),
            (SCOPE_PARAM, scope_param(query.scope).to_owned()),
        ];
        if let Some(reviewer) = query.reviewer {
            params.push((REVIEWER_PARAM, reviewer.to_string()));
        }
        self.list::<MergeRequestJson, _>(&project_path(project, MERGE_REQUESTS_PATH), &params)
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn merge_request(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<MergeRequest, GitlabError> {
        self.get::<MergeRequestJson, _>(&merge_request_path(project, iid, ""))
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn commits(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Commit>, GitlabError> {
        self.list::<CommitJson, _>(&merge_request_path(project, iid, COMMITS_PATH), &[])
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn changes(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Change>, GitlabError> {
        self.list::<ChangeJson, _>(&merge_request_path(project, iid, DIFFS_PATH), &[])
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn discussions(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Discussion>, GitlabError> {
        self.list::<DiscussionJson, _>(&merge_request_path(project, iid, DISCUSSIONS_PATH), &[])
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn add_note(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        body: &str,
    ) -> Result<Note, GitlabError> {
        self.post::<NoteJson, _>(
            &merge_request_path(project, iid, NOTES_PATH),
            &NoteRequest { body },
        )
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn reply(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        discussion: &DiscussionId,
        body: &str,
    ) -> Result<Note, GitlabError> {
        let thread = format!(
            "{DISCUSSIONS_PATH}/{}{NOTES_PATH}",
            percent_encode(&discussion.0)
        );
        self.post::<NoteJson, _>(
            &merge_request_path(project, iid, &thread),
            &NoteRequest { body },
        )
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn resolve_discussion(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        discussion: &DiscussionId,
        resolved: bool,
    ) -> Result<(), GitlabError> {
        let thread = format!("{DISCUSSIONS_PATH}/{}", percent_encode(&discussion.0));
        let url = self
            .config
            .api_url(&merge_request_path(project, iid, &thread));
        self.authorized(|bearer| {
            self.agent
                .put(&url)
                .header(AUTHORIZATION, bearer)
                .send_json(ResolveRequest { resolved })
                .map(drop)
        })
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn approvals(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Approvals, GitlabError> {
        self.get::<ApprovalsJson, _>(&merge_request_path(project, iid, APPROVALS_PATH))
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn approve(&self, project: &ProjectPath, iid: MergeRequestIid) -> Result<(), GitlabError> {
        self.post_empty(&merge_request_path(project, iid, APPROVE_PATH))
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn unapprove(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<(), GitlabError> {
        self.post_empty(&merge_request_path(project, iid, UNAPPROVE_PATH))
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`]; GitLab answers 405 or 422 when it
    /// cannot be merged, and 409 when `options.sha` is no longer the head.
    pub fn merge(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        options: &MergeOptions,
    ) -> Result<MergeRequest, GitlabError> {
        let url = self
            .config
            .api_url(&merge_request_path(project, iid, MERGE_PATH));
        let body = MergeRequestBody::from(options);
        self.authorized(|bearer| {
            self.agent
                .put(&url)
                .header(AUTHORIZATION, bearer)
                .send_json(&body)?
                .body_mut()
                .read_json::<MergeRequestJson>()
        })
        .map(MergeRequest::from)
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn pipelines(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Pipeline>, GitlabError> {
        self.list::<PipelineJson, _>(&merge_request_path(project, iid, PIPELINES_PATH), &[])
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn jobs(
        &self,
        project: &ProjectPath,
        pipeline: PipelineId,
    ) -> Result<Vec<Job>, GitlabError> {
        let path = format!("{PIPELINES_PATH}/{pipeline}{JOBS_PATH}");
        self.list::<JobJson, _>(&project_path(project, &path), &[])
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn branches(&self, project: &ProjectPath) -> Result<Vec<Branch>, GitlabError> {
        self.list::<BranchJson, _>(&project_path(project, BRANCHES_PATH), &[])
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn tree(
        &self,
        project: &ProjectPath,
        git_ref: &str,
        path: &str,
    ) -> Result<Vec<TreeEntry>, GitlabError> {
        let mut params = vec![(REF_PARAM, git_ref.to_owned())];
        if !path.is_empty() {
            params.push((PATH_PARAM, path.to_owned()));
        }
        self.list::<TreeEntryJson, _>(&project_path(project, TREE_PATH), &params)
    }

    /// # Errors
    ///
    /// See [`GitlabClient::current_user`].
    pub fn file(
        &self,
        project: &ProjectPath,
        git_ref: &str,
        path: &str,
    ) -> Result<Vec<u8>, GitlabError> {
        let file = format!("{FILES_PATH}/{}{RAW_PATH}", percent_encode(path));
        let url = self.config.api_url(&project_path(project, &file));
        self.authorized(|bearer| {
            self.agent
                .get(&url)
                .header(AUTHORIZATION, bearer)
                .query(REF_PARAM, git_ref)
                .call()?
                .body_mut()
                .read_to_vec()
        })
    }

    fn get<J: DeserializeOwned, T: From<J>>(&self, path: &str) -> Result<T, GitlabError> {
        let url = self.config.api_url(path);
        self.authorized(|bearer| {
            self.agent
                .get(&url)
                .header(AUTHORIZATION, bearer)
                .call()?
                .body_mut()
                .read_json::<J>()
        })
        .map(T::from)
    }

    fn post<J: DeserializeOwned, T: From<J>>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T, GitlabError> {
        let url = self.config.api_url(path);
        self.authorized(|bearer| {
            self.agent
                .post(&url)
                .header(AUTHORIZATION, bearer)
                .send_json(body)?
                .body_mut()
                .read_json::<J>()
        })
        .map(T::from)
    }

    fn post_empty(&self, path: &str) -> Result<(), GitlabError> {
        let url = self.config.api_url(path);
        self.authorized(|bearer| {
            self.agent
                .post(&url)
                .header(AUTHORIZATION, bearer)
                .send_empty()
                .map(drop)
        })
    }

    fn list<J: DeserializeOwned, T: From<J>>(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<Vec<T>, GitlabError> {
        let url = self.config.api_url(path);
        let mut items = Vec::new();
        let mut page = Some(FIRST_PAGE.to_owned());
        while let Some(current) = page {
            let (batch, next) = self.authorized(|bearer| {
                let mut response = self
                    .agent
                    .get(&url)
                    .header(AUTHORIZATION, bearer)
                    .query(PER_PAGE_PARAM, PER_PAGE)
                    .query(PAGE_PARAM, &current)
                    .query_pairs(params.iter().map(|(key, value)| (*key, value.as_str())))
                    .call()?;
                let next = response
                    .headers()
                    .get(NEXT_PAGE_HEADER)
                    .and_then(|value| value.to_str().ok())
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned);
                Ok((response.body_mut().read_json::<Vec<J>>()?, next))
            })?;
            items.extend(batch.into_iter().map(T::from));
            page = next;
        }
        Ok(items)
    }

    fn authorized<T>(
        &self,
        call: impl Fn(&str) -> Result<T, ureq::Error>,
    ) -> Result<T, GitlabError> {
        let stale = self
            .tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        match call(&bearer(&stale)) {
            Err(ureq::Error::StatusCode(UNAUTHORIZED)) => {
                let fresh = self.refresh(&stale)?;
                Ok(call(&bearer(&fresh))?)
            }
            result => Ok(result?),
        }
    }

    fn refresh(&self, stale: &GitlabTokens) -> Result<GitlabTokens, GitlabError> {
        let mut tokens = self.tokens.lock().unwrap_or_else(PoisonError::into_inner);
        if tokens.access_token != stale.access_token {
            return Ok(tokens.clone());
        }
        let fresh = oauth::refresh(&self.oauth, &self.config, &tokens)?;
        let saved = (self.save)(&fresh);
        *tokens = fresh.clone();
        saved.map_err(GitlabError::SaveTokens)?;
        Ok(fresh)
    }
}

fn bearer(tokens: &GitlabTokens) -> String {
    format!("{BEARER}{}", tokens.access_token)
}

fn project_path(project: &ProjectPath, rest: &str) -> String {
    format!("{PROJECTS_PATH}/{}{rest}", percent_encode(project.as_ref()))
}

fn merge_request_path(project: &ProjectPath, iid: MergeRequestIid, rest: &str) -> String {
    project_path(project, &format!("{MERGE_REQUESTS_PATH}/{iid}{rest}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::review::{InvalidProjectPath, UserId};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{Ipv4Addr, TcpListener};
    use std::sync::Arc;
    use std::thread::JoinHandle;

    const TOKEN: &str = include_str!("../fixtures/token.json");
    const USER: &str = r#"{"id":5,"username":"me","name":"Me"}"#;
    const CONTENT_LENGTH: &str = "content-length:";
    const CRLF: &str = "\r\n";

    fn serve(
        answers: Vec<(u16, &'static str)>,
    ) -> std::io::Result<(String, JoinHandle<Vec<String>>)> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let host = format!("http://{}", listener.local_addr()?);
        let server = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (status, body) in answers {
                let Ok((stream, _)) = listener.accept() else {
                    break;
                };
                let mut reader = BufReader::new(&stream);
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap_or_default();
                seen.push(request_line.trim().to_owned());
                let mut length = 0;
                let mut header = String::new();
                while reader
                    .read_line(&mut header)
                    .is_ok_and(|read| read > CRLF.len())
                {
                    if let Some(value) = header.to_ascii_lowercase().strip_prefix(CONTENT_LENGTH) {
                        length = value.trim().parse().unwrap_or(0);
                    }
                    header.clear();
                }
                let mut request_body = vec![0; length];
                reader.read_exact(&mut request_body).unwrap_or_default();
                let answer = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                (&stream).write_all(answer.as_bytes()).unwrap_or_default();
            }
            seen
        });
        Ok((host, server))
    }

    fn tokens(access: &str) -> GitlabTokens {
        GitlabTokens {
            access_token: access.to_owned(),
            refresh_token: "old-refresh".to_owned(),
        }
    }

    #[test]
    fn project_and_file_paths_are_one_encoded_segment() -> Result<(), InvalidProjectPath> {
        let project: ProjectPath = "group/sub/project".parse()?;
        assert_eq!(
            merge_request_path(&project, MergeRequestIid(7), NOTES_PATH),
            "/projects/group%2Fsub%2Fproject/merge_requests/7/notes"
        );
        Ok(())
    }

    #[test]
    fn an_expired_token_is_refreshed_saved_and_retried() -> Result<(), Box<dyn std::error::Error>> {
        let (host, server) = serve(vec![(401, "{}"), (200, TOKEN), (200, USER)])?;
        let saved: Arc<Mutex<Vec<GitlabTokens>>> = Arc::default();
        let sink = Arc::clone(&saved);
        let client = GitlabClient::new(
            GitlabConfig {
                host,
                client_id: "app".to_owned(),
            },
            tokens("old-access"),
            Box::new(move |fresh| {
                sink.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(fresh.clone());
                Ok(())
            }),
        );
        let user = client.current_user()?;
        assert_eq!(user.id, UserId(5));
        let saved = saved.lock().unwrap_or_else(PoisonError::into_inner).clone();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].refresh_token, "fixture-refresh");
        let seen = server.join().map_err(|_| "server panicked")?;
        assert!(seen[0].starts_with("GET /api/v4/user"));
        assert!(seen[1].starts_with("POST /oauth/token"));
        assert!(seen[2].starts_with("GET /api/v4/user"));
        Ok(())
    }

    #[test]
    fn a_failed_save_still_keeps_the_rotated_pair_in_memory(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (host, server) = serve(vec![(401, "{}"), (200, TOKEN)])?;
        let client = GitlabClient::new(
            GitlabConfig {
                host,
                client_id: "app".to_owned(),
            },
            tokens("old-access"),
            Box::new(|_| Err(std::io::Error::other("no keyring"))),
        );
        assert!(matches!(
            client.current_user(),
            Err(GitlabError::SaveTokens(_))
        ));
        server.join().map_err(|_| "server panicked")?;
        let kept = client
            .tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        assert_eq!(kept.access_token, "fixture-access");
        Ok(())
    }
}
