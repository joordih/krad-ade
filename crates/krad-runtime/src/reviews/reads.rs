use krad_domain::review::{
    project_on_host, Issue, MergeRequestIid, MergeRequestQuery, ProjectPath,
};
use krad_domain::CodeHost;
use krad_gitlab::GitlabConfig;
use krad_host::HostLink;
use krad_protocol::{
    Fetched, ListedRequests, RequestDetailRead, RequestRef, ReviewAnswer, ReviewConnection,
    ReviewWrite, RowFacts,
};
use krad_repo::{remote_url, ORIGIN_REMOTE};
use krad_store::Secrets;
use std::path::Path;
use std::sync::Arc;
use std::thread;

use super::{ReviewDone, ReviewReply};
use crate::code_hosts::{origin_repo, HostClient, HostError, HostLogin};
use crate::jobs::{JobDone, Steps};

pub(crate) const NOT_A_PROJECT: &str =
    "The group's origin remote does not point at a GitLab or GitHub project";
const GITLAB_NOT_SIGNED_IN: &str =
    "Sign in on the GitLab page of Settings to review merge requests.";
const GITHUB_NOT_SIGNED_IN: &str =
    "Sign in on the GitHub page of Settings to review pull requests.";
const GITLAB_NO_ISSUES: &str =
    "Sign in on the GitLab page of Settings to start sessions from issues.";
const GITHUB_NO_ISSUES: &str =
    "Sign in on the GitHub page of Settings to start sessions from issues.";
const FACT_READERS: usize = 8;

fn answered(reply: ReviewReply, answer: ReviewAnswer) -> JobDone {
    JobDone::Review(Box::new(ReviewDone::Answered { reply, answer }))
}

const fn signed_out_hint(host: CodeHost) -> &'static str {
    match host {
        CodeHost::Gitlab => GITLAB_NOT_SIGNED_IN,
        CodeHost::Github => GITHUB_NOT_SIGNED_IN,
    }
}

const fn no_issues_hint(host: CodeHost) -> &'static str {
    match host {
        CodeHost::Gitlab => GITLAB_NO_ISSUES,
        CodeHost::Github => GITHUB_NO_ISSUES,
    }
}

pub(crate) fn probe(
    reply: ReviewReply,
    (link, secrets): (&HostLink, &Secrets),
    gitlab: Option<GitlabConfig>,
) -> JobDone {
    let host = origin_repo(link, &reply.root, gitlab).and_then(|repo| {
        let signed_in = repo.login.client(secrets).ok().flatten().is_some();
        signed_in.then(|| repo.login.code_host())
    });
    answered(reply, ReviewAnswer::Probed { host })
}

#[tracing::instrument(skip_all, fields(environment = ?reply.environment))]
pub(crate) fn connect(
    reply: ReviewReply,
    (link, secrets): (&HostLink, &Secrets),
    gitlab: Option<GitlabConfig>,
) -> JobDone {
    let (client, connection) = signed_in((link, secrets), &reply.root, gitlab);
    JobDone::Review(Box::new(ReviewDone::Connected {
        reply,
        client: client.map(Arc::new),
        connection,
    }))
}

fn signed_in(
    (link, secrets): (&HostLink, &Secrets),
    root: &Path,
    gitlab: Option<GitlabConfig>,
) -> (Option<HostClient>, ReviewConnection) {
    let Some(repo) = origin_repo(link, root, gitlab) else {
        let reason = NOT_A_PROJECT.to_owned();
        return (None, ReviewConnection::SignedOut { reason });
    };
    let client = match repo.login.client(secrets) {
        Ok(Some(client)) => client,
        Ok(None) => {
            let reason = signed_out_hint(repo.login.code_host()).to_owned();
            return (None, ReviewConnection::SignedOut { reason });
        }
        Err(error) => {
            let message = error.to_string();
            return (None, ReviewConnection::Failed { message });
        }
    };
    match client.current_user() {
        Ok(me) => {
            let host = client.code_host();
            (Some(client), ReviewConnection::Ready { host, me })
        }
        Err(error) if error.is_signed_out() => {
            let reason = error.to_string();
            (None, ReviewConnection::SignedOut { reason })
        }
        Err(error) => {
            let message = error.to_string();
            (None, ReviewConnection::Failed { message })
        }
    }
}

pub(crate) fn list(
    reply: ReviewReply,
    (client, link): (&HostClient, &HostLink),
    query: MergeRequestQuery,
) -> JobDone {
    let listed = listed_requests(client, link, &reply.root, query);
    answered(
        reply,
        ReviewAnswer::Listed {
            listed: listed.into(),
        },
    )
}

fn listed_requests(
    client: &HostClient,
    link: &HostLink,
    root: &Path,
    query: MergeRequestQuery,
) -> Result<ListedRequests, String> {
    let url = remote_url(link, root, ORIGIN_REMOTE).map_err(|error| error.to_string())?;
    let project = url
        .and_then(|url| project_on_host(&url, client.host()))
        .ok_or_else(|| NOT_A_PROJECT.to_owned())?;
    let requests = client
        .merge_requests(&project, query)
        .map_err(|error| error.to_string())?;
    Ok(ListedRequests { project, requests })
}

pub(crate) fn facts(
    reply: &ReviewReply,
    client: &HostClient,
    (project, iids): (&ProjectPath, &[MergeRequestIid]),
    steps: &Steps,
) {
    let chunk_len = iids.len().div_ceil(FACT_READERS).max(1);
    thread::scope(|scope| {
        for chunk in iids.chunks(chunk_len) {
            scope.spawn(move || {
                for iid in chunk {
                    let facts = row_facts(client, project, *iid).into();
                    let answer = ReviewAnswer::Facts { iid: *iid, facts };
                    steps.report(answered(reply.clone(), answer));
                }
            });
        }
    });
}

fn row_facts(
    client: &HostClient,
    project: &ProjectPath,
    iid: MergeRequestIid,
) -> Result<RowFacts, HostError> {
    let request = client.merge_request(project, iid)?;
    let approvals = client.approvals(project, iid)?;
    Ok(RowFacts {
        pipeline: request.head_pipeline.map(|pipeline| pipeline.status),
        approvals,
    })
}

pub(crate) fn detail(reply: ReviewReply, client: &HostClient, request: &RequestRef) -> JobDone {
    let RequestRef { project, iid } = request;
    let iid = *iid;
    let read = thread::scope(|scope| {
        let merge_request = scope.spawn(|| {
            client.merge_request(project, iid).map(|request| {
                let jobs = request
                    .head_pipeline
                    .as_ref()
                    .map(|pipeline| client.jobs(project, pipeline.id));
                (request, jobs)
            })
        });
        let commits = scope.spawn(|| client.commits(project, iid));
        let changes = scope.spawn(|| client.changes(project, iid));
        let discussions = scope.spawn(|| client.discussions(project, iid));
        let approvals = scope.spawn(|| client.approvals(project, iid));
        let pipelines = scope.spawn(|| client.pipelines(project, iid));
        let (request, jobs) = match merge_request.join() {
            Ok(Ok((request, jobs))) => (
                Fetched::Ready(request),
                jobs.map_or(Fetched::Ready(Vec::new()), Fetched::from),
            ),
            Ok(Err(error)) => (Fetched::Failed(error.to_string()), lost()),
            Err(_) => (lost(), lost()),
        };
        RequestDetailRead {
            iid,
            request,
            jobs,
            commits: joined(commits.join()),
            changes: joined(changes.join()),
            discussions: joined(discussions.join()),
            approvals: joined(approvals.join()),
            pipelines: joined(pipelines.join()),
        }
    });
    answered(
        reply,
        ReviewAnswer::Detail {
            detail: Box::new(read),
        },
    )
}

const READ_LOST: &str = "The read stopped before it answered";

fn lost<T>() -> Fetched<T> {
    Fetched::Failed(READ_LOST.to_owned())
}

fn joined<T, P>(joined: Result<Result<T, HostError>, P>) -> Fetched<T> {
    joined.map_or_else(|_| lost(), Fetched::from)
}

pub(crate) fn issues(
    reply: ReviewReply,
    (link, secrets): (&HostLink, &Secrets),
    gitlab: Option<GitlabConfig>,
) -> JobDone {
    let issues = open_issues((link, secrets), &reply.root, gitlab).into();
    answered(reply, ReviewAnswer::Issues { issues })
}

fn open_issues(
    (link, secrets): (&HostLink, &Secrets),
    root: &Path,
    gitlab: Option<GitlabConfig>,
) -> Result<Vec<Issue>, String> {
    let repo = origin_repo(link, root, gitlab).ok_or_else(|| NOT_A_PROJECT.to_owned())?;
    let client = signed_in_client(&repo.login, secrets)?;
    client
        .issues(&repo.project)
        .map_err(|error| error.to_string())
}

fn signed_in_client(login: &HostLogin, secrets: &Secrets) -> Result<HostClient, String> {
    login
        .client(secrets)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| no_issues_hint(login.code_host()).to_owned())
}

pub(crate) fn write(
    reply: ReviewReply,
    client: &HostClient,
    request: &RequestRef,
    write: &ReviewWrite,
) -> JobDone {
    let RequestRef { project, iid } = request;
    let iid = *iid;
    let written = match write {
        ReviewWrite::Comment { body } => client.add_note(project, iid, body).map(drop),
        ReviewWrite::Reply { discussion, body } => {
            client.reply(project, iid, discussion, body).map(drop)
        }
        ReviewWrite::Resolve { discussion } => {
            client.resolve_discussion(project, iid, discussion, true)
        }
        ReviewWrite::Approve => client.approve(project, iid),
        ReviewWrite::Revoke => client.unapprove(project, iid),
        ReviewWrite::Merge { options } => client.merge(project, iid, options).map(drop),
    };
    let failure = written.err().map(|error| error.to_string());
    answered(reply, ReviewAnswer::Written { failure })
}
