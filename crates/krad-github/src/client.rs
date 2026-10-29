use crate::encode::encode_path;
use crate::graphql::{self, Answer, Ignored, ReplyData, ThreadsData};
use crate::model::{
    BranchJson, CommentBody, CommitJson, CompareJson, ContentJson, DismissBody, FileJson,
    IssueCommentJson, JobsJson, MergeBody, PullJson, RepositoryJson, ReviewBody, ReviewJson,
    ReviewState, RunsJson, UserJson, CONVERSATION_PREFIX,
};
use krad_domain::review::{
    Approval, Approvals, Branch, Change, Commit, CommitSha, DiffRefs, Discussion, DiscussionId,
    Job, MergeOptions, MergeRequest, MergeRequestIid, MergeRequestQuery, Note, Pipeline,
    PipelineId, ProjectPath, Scope, StateFilter, TreeEntry, User, UserId,
};
use krad_domain::GithubToken;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;
use ureq::Agent;

pub const GITHUB_HOST: &str = "github.com";
const API_URL: &str = "https://api.github.com";
const GRAPHQL_PATH: &str = "/graphql";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const UNAUTHORIZED: u16 = 401;
const AUTHORIZATION: &str = "Authorization";
const BEARER: &str = "Bearer ";
const ACCEPT: &str = "Accept";
const JSON_MEDIA: &str = "application/vnd.github+json";
const RAW_MEDIA: &str = "application/vnd.github.raw+json";
const API_VERSION_HEADER: &str = "X-GitHub-Api-Version";
const API_VERSION: &str = "2022-11-28";
const USER_AGENT_HEADER: &str = "User-Agent";
const USER_AGENT: &str = "krad-ade";
const LINK_HEADER: &str = "link";
const NEXT_RELATION: &str = "rel=\"next\"";
const LINK_SEPARATOR: char = ',';
const LINK_FIELD_SEPARATOR: char = ';';
const PER_PAGE_PARAM: &str = "per_page";
const PER_PAGE: &str = "100";
const STATE_PARAM: &str = "state";
const HEAD_SHA_PARAM: &str = "head_sha";
const REF_PARAM: &str = "ref";
const OPEN_STATE: &str = "open";
const CLOSED_STATE: &str = "closed";
const ALL_STATE: &str = "all";
const APPROVE_EVENT: &str = "APPROVE";
const DISMISS_EVENT: &str = "DISMISS";
const WITHDRAWN_MESSAGE: &str = "Approval withdrawn";
const SQUASH_METHOD: &str = "squash";
const MERGE_METHOD: &str = "merge";
const PATH_SEPARATOR: char = '/';

#[derive(Debug, thiserror::Error)]
pub enum GithubError {
    #[error("The GitHub login is no longer valid; sign in again")]
    SignedOut,
    #[error("A GitHub repository path looks like owner/repo")]
    NotARepository,
    #[error("GitHub refused the request: {0}")]
    Graphql(String),
    #[error("GitHub request failed: {0}")]
    Http(ureq::Error),
}

impl From<ureq::Error> for GithubError {
    fn from(error: ureq::Error) -> Self {
        match error {
            ureq::Error::StatusCode(UNAUTHORIZED) => Self::SignedOut,
            error => Self::Http(error),
        }
    }
}

pub struct GithubClient {
    agent: Agent,
    api: String,
    bearer: String,
    me: OnceLock<User>,
}

impl GithubClient {
    #[must_use]
    pub fn new(token: &GithubToken) -> Self {
        Self::with_api(API_URL.to_owned(), token)
    }

    fn with_api(api: String, token: &GithubToken) -> Self {
        let agent = Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build()
            .new_agent();
        Self {
            agent,
            api,
            bearer: format!("{BEARER}{}", token.access_token),
            me: OnceLock::new(),
        }
    }

    #[must_use]
    pub fn host(&self) -> &str {
        GITHUB_HOST
    }

    /// # Errors
    ///
    /// [`GithubError::SignedOut`] when GitHub no longer takes the token and
    /// [`GithubError::Http`] when it cannot be reached or refuses.
    pub fn current_user(&self) -> Result<User, GithubError> {
        if let Some(me) = self.me.get() {
            return Ok(me.clone());
        }
        let me: User = self.get::<UserJson>("/user")?.into();
        Ok(self.me.get_or_init(|| me).clone())
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn merge_requests(
        &self,
        project: &ProjectPath,
        query: MergeRequestQuery,
    ) -> Result<Vec<MergeRequest>, GithubError> {
        let state = match query.state {
            StateFilter::Opened => OPEN_STATE,
            StateFilter::Merged | StateFilter::Closed => CLOSED_STATE,
            StateFilter::All => ALL_STATE,
        };
        let pulls: Vec<PullJson> = self.list(
            &repo_path(project, "/pulls")?,
            &[(STATE_PARAM, state.to_owned())],
        )?;
        let me = match (query.scope, query.reviewer) {
            (Scope::All, None) => None,
            _ => Some(self.current_user()?.id),
        };
        Ok(pulls
            .into_iter()
            .filter(|pull| match query.state {
                StateFilter::Merged => pull.is_merged(),
                StateFilter::Closed => !pull.is_merged(),
                StateFilter::Opened | StateFilter::All => true,
            })
            .filter(|pull| selects(pull, query, me))
            .map(MergeRequest::from)
            .collect())
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn merge_request(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<MergeRequest, GithubError> {
        let pull: PullJson = self.get(&pull_path(project, iid, "")?)?;
        let head = pull.head_sha().to_owned();
        let (base, branch) = pull.branches();
        let compare = format!("/compare/{}...{}", encode_path(base), encode_path(branch));
        let merge_base = self
            .get::<CompareJson>(&repo_path(project, &compare)?)
            .map(|compare| compare.merge_base_commit.sha);
        let mut request = MergeRequest::from(pull);
        request.head_pipeline = self.runs(project, &head)?.into_iter().next();
        request.diff_refs = merge_base.ok().map(|base| DiffRefs {
            base_sha: CommitSha(base),
            head_sha: CommitSha(head),
        });
        Ok(request)
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn commits(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Commit>, GithubError> {
        let commits: Vec<CommitJson> = self.list(&pull_path(project, iid, "/commits")?, &[])?;
        Ok(commits.into_iter().map(Commit::from).collect())
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn changes(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Change>, GithubError> {
        let files: Vec<FileJson> = self.list(&pull_path(project, iid, "/files")?, &[])?;
        Ok(files.into_iter().map(Change::from).collect())
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`]; [`GithubError::Graphql`] when the
    /// threads cannot be read.
    pub fn discussions(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Discussion>, GithubError> {
        let comments: Vec<IssueCommentJson> =
            self.list(&issue_path(project, iid, "/comments")?, &[])?;
        let reviews: Vec<ReviewJson> = self.list(&pull_path(project, iid, "/reviews")?, &[])?;
        let mut discussions: Vec<Discussion> = comments.into_iter().map(Discussion::from).collect();
        discussions.extend(reviews.into_iter().filter_map(ReviewJson::into_discussion));
        discussions.extend(self.threads(project, iid)?);
        Ok(discussions)
    }

    fn threads(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Discussion>, GithubError> {
        let (owner, name) = owner_and_name(project)?;
        let mut threads = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let request = graphql::list_threads(owner, name, iid.0, after.as_deref());
            let (page, next) = graphql::thread_page(self.graphql::<_, ThreadsData>(&request)?);
            threads.extend(page);
            match next {
                Some(cursor) => after = Some(cursor),
                None => return Ok(threads),
            }
        }
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn add_note(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        body: &str,
    ) -> Result<Note, GithubError> {
        let comment: IssueCommentJson = self.send_json(
            &issue_path(project, iid, "/comments")?,
            &CommentBody { body },
        )?;
        Ok(comment.into())
    }

    /// # Errors
    ///
    /// See [`GithubClient::discussions`].
    pub fn reply(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        discussion: &DiscussionId,
        body: &str,
    ) -> Result<Note, GithubError> {
        if discussion.0.starts_with(CONVERSATION_PREFIX) {
            return self.add_note(project, iid, body);
        }
        let data: ReplyData = self.graphql(&graphql::reply(&discussion.0, body))?;
        Ok(data.into_note())
    }

    /// # Errors
    ///
    /// See [`GithubClient::discussions`].
    pub fn resolve_discussion(
        &self,
        _project: &ProjectPath,
        _iid: MergeRequestIid,
        discussion: &DiscussionId,
        resolved: bool,
    ) -> Result<(), GithubError> {
        self.graphql::<_, Ignored>(&graphql::resolve(&discussion.0, resolved))
            .map(drop)
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn approvals(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Approvals, GithubError> {
        let reviews: Vec<ReviewJson> = self.list(&pull_path(project, iid, "/reviews")?, &[])?;
        let approved_by: Vec<Approval> = latest_verdicts(&reviews)
            .into_iter()
            .filter(|(_, review)| review.state == ReviewState::Approved)
            .filter_map(|(_, review)| review.author())
            .map(|user| Approval { user })
            .collect();
        Ok(Approvals {
            approved: !approved_by.is_empty(),
            approvals_required: 0,
            approvals_left: 0,
            approved_by,
        })
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn approve(&self, project: &ProjectPath, iid: MergeRequestIid) -> Result<(), GithubError> {
        self.send_json::<_, Ignored>(
            &pull_path(project, iid, "/reviews")?,
            &ReviewBody {
                event: APPROVE_EVENT,
            },
        )
        .map(drop)
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn unapprove(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<(), GithubError> {
        let me = self.current_user()?.id;
        let reviews: Vec<ReviewJson> = self.list(&pull_path(project, iid, "/reviews")?, &[])?;
        let Some(review) = latest_verdicts(&reviews)
            .remove(&me)
            .filter(|review| review.state == ReviewState::Approved)
        else {
            return Ok(());
        };
        let dismissal = format!("/reviews/{}/dismissals", review.id);
        self.put_json::<_, Ignored>(
            &pull_path(project, iid, &dismissal)?,
            &DismissBody {
                message: WITHDRAWN_MESSAGE,
                event: DISMISS_EVENT,
            },
        )
        .map(drop)
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`]; GitHub answers 405 when it cannot
    /// be merged and 409 when `options.sha` is no longer the head.
    pub fn merge(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
        options: &MergeOptions,
    ) -> Result<MergeRequest, GithubError> {
        let body = MergeBody {
            sha: options.sha.as_ref().map(|sha| sha.0.as_str()),
            merge_method: if options.squash {
                SQUASH_METHOD
            } else {
                MERGE_METHOD
            },
        };
        self.put_json::<_, Ignored>(&pull_path(project, iid, "/merge")?, &body)?;
        let merged = self.merge_request(project, iid)?;
        if options.should_remove_source_branch {
            let branch = format!("/git/refs/heads/{}", encode_path(&merged.source_branch));
            let url = self.url(&repo_path(project, &branch)?);
            self.call(self.agent.delete(&url)).map(drop)?;
        }
        Ok(merged)
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn pipelines(
        &self,
        project: &ProjectPath,
        iid: MergeRequestIid,
    ) -> Result<Vec<Pipeline>, GithubError> {
        let pull: PullJson = self.get(&pull_path(project, iid, "")?)?;
        self.runs(project, pull.head_sha())
    }

    fn runs(&self, project: &ProjectPath, head: &str) -> Result<Vec<Pipeline>, GithubError> {
        let url = self.url(&repo_path(project, "/actions/runs")?);
        let runs: RunsJson = self.read(
            self.agent
                .get(&url)
                .query(HEAD_SHA_PARAM, head)
                .query(PER_PAGE_PARAM, PER_PAGE),
        )?;
        Ok(runs.workflow_runs.into_iter().map(Pipeline::from).collect())
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn jobs(
        &self,
        project: &ProjectPath,
        pipeline: PipelineId,
    ) -> Result<Vec<Job>, GithubError> {
        let jobs: JobsJson = self.get(&repo_path(
            project,
            &format!("/actions/runs/{pipeline}/jobs"),
        )?)?;
        Ok(jobs.jobs.into_iter().map(Job::from).collect())
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn branches(&self, project: &ProjectPath) -> Result<Vec<Branch>, GithubError> {
        let repository: RepositoryJson = self.get(&repo_path(project, "")?)?;
        let branches: Vec<BranchJson> = self.list(&repo_path(project, "/branches")?, &[])?;
        Ok(branches
            .into_iter()
            .map(|branch| branch.into_branch(&repository.default_branch))
            .collect())
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn tree(
        &self,
        project: &ProjectPath,
        git_ref: &str,
        path: &str,
    ) -> Result<Vec<TreeEntry>, GithubError> {
        let url = self.url(&contents_path(project, path)?);
        let entries: Vec<ContentJson> =
            self.read(self.agent.get(&url).query(REF_PARAM, git_ref))?;
        Ok(entries
            .into_iter()
            .filter_map(ContentJson::into_entry)
            .collect())
    }

    /// # Errors
    ///
    /// See [`GithubClient::current_user`].
    pub fn file(
        &self,
        project: &ProjectPath,
        git_ref: &str,
        path: &str,
    ) -> Result<Vec<u8>, GithubError> {
        let url = self.url(&contents_path(project, path)?);
        let request = self
            .agent
            .get(&url)
            .header(AUTHORIZATION, &self.bearer)
            .header(ACCEPT, RAW_MEDIA)
            .header(API_VERSION_HEADER, API_VERSION)
            .header(USER_AGENT_HEADER, USER_AGENT)
            .query(REF_PARAM, git_ref);
        Ok(request.call()?.body_mut().read_to_vec()?)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.api)
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, GithubError> {
        self.read(self.agent.get(&self.url(path)))
    }

    fn read<T: DeserializeOwned>(
        &self,
        request: ureq::RequestBuilder<ureq::typestate::WithoutBody>,
    ) -> Result<T, GithubError> {
        Ok(self.call(request)?.body_mut().read_json()?)
    }

    fn call(
        &self,
        request: ureq::RequestBuilder<ureq::typestate::WithoutBody>,
    ) -> Result<ureq::http::Response<ureq::Body>, GithubError> {
        Ok(signed(request, &self.bearer).call()?)
    }

    fn send_json<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, GithubError> {
        let request = signed(self.agent.post(&self.url(path)), &self.bearer);
        Ok(request.send_json(body)?.body_mut().read_json()?)
    }

    fn put_json<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, GithubError> {
        let request = signed(self.agent.put(&self.url(path)), &self.bearer);
        Ok(request.send_json(body)?.body_mut().read_json()?)
    }

    fn graphql<V: Serialize, D: DeserializeOwned>(
        &self,
        request: &graphql::Request<V>,
    ) -> Result<D, GithubError> {
        let answer: Answer<D> = self.send_json(GRAPHQL_PATH, request)?;
        match answer.data {
            Some(data) if answer.errors.is_empty() => Ok(data),
            _ => Err(GithubError::Graphql(
                answer
                    .errors
                    .into_iter()
                    .map(|error| error.message)
                    .collect::<Vec<_>>()
                    .join("; "),
            )),
        }
    }

    fn list<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<Vec<T>, GithubError> {
        let mut items = Vec::new();
        let mut next = Some(self.url(path));
        let mut first = true;
        while let Some(url) = next {
            let mut request = self.agent.get(&url);
            if first {
                request = request
                    .query(PER_PAGE_PARAM, PER_PAGE)
                    .query_pairs(params.iter().map(|(key, value)| (*key, value.as_str())));
                first = false;
            }
            let mut response = self.call(request)?;
            next = response
                .headers()
                .get(LINK_HEADER)
                .and_then(|value| value.to_str().ok())
                .and_then(next_link);
            items.extend(response.body_mut().read_json::<Vec<T>>()?);
        }
        Ok(items)
    }
}

fn signed<B>(request: ureq::RequestBuilder<B>, bearer: &str) -> ureq::RequestBuilder<B> {
    request
        .header(AUTHORIZATION, bearer)
        .header(ACCEPT, JSON_MEDIA)
        .header(API_VERSION_HEADER, API_VERSION)
        .header(USER_AGENT_HEADER, USER_AGENT)
}

fn selects(pull: &PullJson, query: MergeRequestQuery, me: Option<UserId>) -> bool {
    let has = |users: &[UserJson], id: UserId| users.iter().any(|user| UserId(user.id) == id);
    let in_scope = match (query.scope, me) {
        (Scope::CreatedByMe, Some(me)) => pull.author_id() == me,
        (Scope::AssignedToMe, Some(me)) => has(&pull.assignees, me),
        _ => true,
    };
    let reviews = query
        .reviewer
        .is_none_or(|reviewer| has(&pull.requested_reviewers, reviewer));
    in_scope && reviews
}

fn latest_verdicts(reviews: &[ReviewJson]) -> HashMap<UserId, &ReviewJson> {
    reviews
        .iter()
        .filter(|review| review.state.is_verdict())
        .filter_map(|review| Some((UserId(review.user.as_ref()?.id), review)))
        .collect()
}

fn next_link(header: &str) -> Option<String> {
    header.split(LINK_SEPARATOR).find_map(|link| {
        let (url, relations) = link.split_once(LINK_FIELD_SEPARATOR)?;
        relations.contains(NEXT_RELATION).then(|| {
            url.trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_owned()
        })
    })
}

fn owner_and_name(project: &ProjectPath) -> Result<(&str, &str), GithubError> {
    project
        .as_ref()
        .split_once(PATH_SEPARATOR)
        .filter(|(_, name)| !name.contains(PATH_SEPARATOR))
        .ok_or(GithubError::NotARepository)
}

fn repo_path(project: &ProjectPath, rest: &str) -> Result<String, GithubError> {
    let (owner, name) = owner_and_name(project)?;
    Ok(format!(
        "/repos/{}/{}{rest}",
        encode_path(owner),
        encode_path(name)
    ))
}

fn pull_path(
    project: &ProjectPath,
    iid: MergeRequestIid,
    rest: &str,
) -> Result<String, GithubError> {
    repo_path(project, &format!("/pulls/{iid}{rest}"))
}

fn issue_path(
    project: &ProjectPath,
    iid: MergeRequestIid,
    rest: &str,
) -> Result<String, GithubError> {
    repo_path(project, &format!("/issues/{iid}{rest}"))
}

fn contents_path(project: &ProjectPath, path: &str) -> Result<String, GithubError> {
    let path = path.trim_matches(PATH_SEPARATOR);
    repo_path(project, &format!("/contents/{}", encode_path(path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_link_header_names_the_next_page() {
        let header = r#"<https://api.github.com/repos/o/r/pulls?page=2>; rel="next", <https://api.github.com/repos/o/r/pulls?page=5>; rel="last""#;
        assert_eq!(
            next_link(header).as_deref(),
            Some("https://api.github.com/repos/o/r/pulls?page=2")
        );
        assert_eq!(next_link(r#"<https://x>; rel="prev""#), None);
    }

    #[test]
    fn only_owner_and_repo_name_a_repository() -> Result<(), Box<dyn std::error::Error>> {
        let project: ProjectPath = "octo/app".parse()?;
        assert_eq!(repo_path(&project, "/pulls")?, "/repos/octo/app/pulls");
        let nested: ProjectPath = "group/sub/app".parse()?;
        assert!(matches!(
            repo_path(&nested, ""),
            Err(GithubError::NotARepository)
        ));
        Ok(())
    }

    #[test]
    fn a_reviewer_keeps_only_their_latest_verdict() -> serde_json::Result<()> {
        let reviews: Vec<ReviewJson> = serde_json::from_str(
            r#"[
              {"id":1,"node_id":"a","user":{"id":2,"login":"bo"},"state":"APPROVED"},
              {"id":2,"node_id":"b","user":{"id":2,"login":"bo"},"state":"COMMENTED"},
              {"id":3,"node_id":"c","user":{"id":3,"login":"cy"},"state":"APPROVED"},
              {"id":4,"node_id":"d","user":{"id":3,"login":"cy"},"state":"CHANGES_REQUESTED"}
            ]"#,
        )?;
        let latest = latest_verdicts(&reviews);
        assert_eq!(latest.get(&UserId(2)).map(|review| review.id), Some(1));
        assert_eq!(latest.get(&UserId(3)).map(|review| review.id), Some(4));
        Ok(())
    }
}
