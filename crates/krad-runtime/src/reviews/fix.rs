use krad_domain::review::{
    ci_fix_prompt, ChangeStat, CiFailure, HeadRepository, JobId, ProjectPath,
};
use krad_domain::AgentPreset;
use krad_protocol::RequestRef;

use super::{ReviewDone, ReviewReply};
use crate::code_hosts::{HostClient, HostError};
use crate::jobs::JobDone;

const NO_PIPELINE: &str = "The merge request has no pipeline to read the job from";
const NO_JOB: &str = "The failed job is no longer part of the merge request's pipeline";

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct FixRead {
    pub(crate) project: ProjectPath,
    pub(crate) head: HeadRepository,
    pub(crate) branch: String,
    pub(crate) prompt: String,
}

#[tracing::instrument(skip_all, fields(environment = ?reply.environment, job = job.0))]
pub(crate) fn read_fix(
    reply: ReviewReply,
    client: &HostClient,
    (request, job, harness): (&RequestRef, JobId, AgentPreset),
) -> JobDone {
    let read = fix_of(client, request, job).map_err(|error| error.to_string());
    JobDone::Review(Box::new(ReviewDone::FixRead {
        reply,
        job,
        harness,
        read,
    }))
}

fn fix_of(client: &HostClient, request: &RequestRef, job: JobId) -> Result<FixRead, FixError> {
    let RequestRef { project, iid } = request;
    let merge_request = client.merge_request(project, *iid)?;
    let pipeline = merge_request
        .head_pipeline
        .as_ref()
        .ok_or(FixError::Missing(NO_PIPELINE))?;
    let failed = client
        .jobs(project, pipeline.id)?
        .into_iter()
        .find(|candidate| candidate.id == job)
        .ok_or(FixError::Missing(NO_JOB))?;
    let log = client.job_log(project, job)?;
    let changes: Vec<ChangeStat> = client
        .changes(project, *iid)?
        .iter()
        .map(ChangeStat::from)
        .collect();
    let prompt = ci_fix_prompt(&CiFailure {
        host: client.code_host(),
        request: &merge_request,
        job: &failed,
        log: &log,
        changes: &changes,
    });
    Ok(FixRead {
        project: project.clone(),
        head: merge_request.head_repository,
        branch: merge_request.source_branch,
        prompt,
    })
}

#[derive(Debug, thiserror::Error)]
enum FixError {
    #[error(transparent)]
    Host(#[from] HostError),
    #[error("{0}")]
    Missing(&'static str),
}
