use krad_domain::{AppEvent, ReviewVerdict};
use krad_gitlab::GitlabConfig;
use krad_protocol::{
    ErrorCode, EventKind, Reply, RequestId, ReviewAct, ReviewAnswer, ReviewAnswered, ReviewCommand,
    ReviewWrite,
};

use krad_host::HostLink;
use std::io;
use std::sync::Arc;

use super::{refused, Runtime, UNSUPPORTED_COMMAND};
use crate::code_hosts::HostClient;
use crate::environments::UNKNOWN_ENVIRONMENT;
use crate::reviews::{
    connect, detail, facts, issues, list, probe, read_fix, write, ReviewDone, ReviewReply,
};

const NOT_CONNECTED: &str = "connect to the group's code host first";
const NOT_STARTED: &str = "the code host read could not start";

const fn verdict_of(write: &ReviewWrite) -> Option<ReviewVerdict> {
    match write {
        ReviewWrite::Comment { .. } => Some(ReviewVerdict::Commented),
        ReviewWrite::Reply { .. } => Some(ReviewVerdict::Replied),
        ReviewWrite::Approve => Some(ReviewVerdict::Approved),
        ReviewWrite::Resolve { .. }
        | ReviewWrite::Revoke
        | ReviewWrite::Merge { .. }
        | ReviewWrite::Unknown => None,
    }
}

impl Runtime {
    pub(super) fn execute_review(&mut self, id: RequestId, command: ReviewCommand) -> Reply {
        let ReviewCommand {
            environment,
            root,
            act,
        } = command;
        let Some(host) = self.environments.host_of(&environment) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let link = self.hosts.link(&host);
        let gitlab = GitlabConfig::try_from(&self.settings.current.gitlab).ok();
        let reply = ReviewReply {
            request: id,
            environment,
            root,
        };
        let connected = self.reviews.client(&reply.key());
        let secrets = self.secrets.clone();
        let started = match act {
            ReviewAct::Probe => self
                .jobs
                .run(move || probe(reply, (&link, &secrets), gitlab)),
            ReviewAct::Connect => self
                .jobs
                .run(move || connect(reply, (&link, &secrets), gitlab)),
            ReviewAct::Issues => self
                .jobs
                .run(move || issues(reply, (&link, &secrets), gitlab)),
            ReviewAct::Unknown
            | ReviewAct::Write {
                write: ReviewWrite::Unknown,
                ..
            } => return refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND),
            connected_act => match connected {
                Some(client) => self.start_connected(reply, (client, link), connected_act),
                None => return refused(ErrorCode::Unavailable, NOT_CONNECTED),
            },
        };
        match started {
            Ok(()) => Reply::Accepted,
            Err(error) => {
                tracing::warn!(%error, "a code host read could not start");
                self.reviews.verdicts.remove(&id);
                refused(ErrorCode::Unavailable, NOT_STARTED)
            }
        }
    }

    fn start_connected(
        &mut self,
        reply: ReviewReply,
        (client, link): (Arc<HostClient>, HostLink),
        act: ReviewAct,
    ) -> io::Result<()> {
        match act {
            ReviewAct::List { query } => {
                self.jobs.run(move || list(reply, (&client, &link), query))
            }
            ReviewAct::Facts { project, iids } => self.jobs.run_reporting(move |steps| {
                facts(&reply, &client, (&project, &iids), steps);
            }),
            ReviewAct::Detail { request } => {
                self.jobs.run(move || detail(reply, &client, &request))
            }
            ReviewAct::Write {
                request,
                write: change,
            } => {
                if let Some(verdict) = verdict_of(&change) {
                    self.reviews
                        .verdicts
                        .insert(reply.request, (request.iid, verdict));
                }
                self.jobs
                    .run(move || write(reply, &client, &request, &change))
            }
            ReviewAct::FixJob {
                request,
                job,
                harness,
            } => self
                .jobs
                .run(move || read_fix(reply, &client, (&request, job, harness))),
            ReviewAct::Probe | ReviewAct::Connect | ReviewAct::Issues | ReviewAct::Unknown => {
                Ok(())
            }
        }
    }

    pub(super) fn review_done(&mut self, done: ReviewDone) {
        match done {
            ReviewDone::Answered { reply, answer } => {
                let sent = self.review_sent(&reply, &answer);
                let place = (reply.environment.clone(), reply.root.clone());
                self.answer_review(reply, answer);
                if let Some(event) = sent {
                    let group = self.group_with_root(&place.0, &place.1);
                    self.events
                        .publish_own(&place.0, group, &event, &mut self.outbox);
                }
            }
            ReviewDone::Connected {
                reply,
                client,
                connection,
            } => {
                self.reviews.connected(reply.key(), client);
                self.answer_review(reply, ReviewAnswer::Connected { connection });
            }
            ReviewDone::FixRead {
                reply,
                job,
                harness,
                read,
            } => self.deliver_fix(reply, (job, harness), read),
            ReviewDone::Login(done) => self.login_done(done),
        }
    }

    pub(super) fn answer_review(&mut self, reply: ReviewReply, answer: ReviewAnswer) {
        let ReviewReply {
            request,
            environment,
            root,
        } = reply;
        self.outbox
            .push(EventKind::ReviewAnswered(Box::new(ReviewAnswered {
                request,
                environment,
                root,
                answer,
            })));
    }

    fn review_sent(&mut self, reply: &ReviewReply, answer: &ReviewAnswer) -> Option<AppEvent> {
        let (merge_request, verdict) = self.reviews.verdicts.remove(&reply.request)?;
        matches!(answer, ReviewAnswer::Written { failure: None }).then_some(AppEvent::ReviewSent {
            merge_request,
            verdict,
        })
    }
}
