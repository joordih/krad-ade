use futures::channel::oneshot;
use krad_agents::{
    ApiError, Caller, ErrorCode, Incoming, LeadHandle, Request, Response, TerminalHandle,
    VariantsRequest,
};
use krad_domain::{
    EnvironmentName, GroupId, ProjectTask, SessionId, TerminalId, VariantId, VariantRemoval,
    VariantRunId, VariantsOrder,
};
use krad_protocol::Reply;
use krad_repo::detect_tasks;

use super::Runtime;
use crate::jobs::JobDone;
use crate::variants::{chosen_task, BusOrder, VariantJob};

const NO_ENVIRONMENT: &str = "The caller's environment is not open";
const NO_CALLER_GROUP: &str = "The caller is in no group; name one";
const NO_SUCH_GROUP: &str = "No group with that name in the caller's environment";
const NOT_CONFIRMED: &str =
    "Picking a winner discards the other variants and removes their worktrees; confirm it";
const JOB_NOT_STARTED: &str = "the project's tasks could not be read";

fn reply_with(reply: oneshot::Sender<Response>, response: Response) {
    if reply.send(response).is_err() {
        tracing::trace!("a bus caller left before its variant answer");
    }
}

fn bus_error(reply: Reply) -> Response {
    match reply {
        Reply::Accepted => Response::error(ErrorCode::Failed, String::new()),
        Reply::Refused { message, .. } => Response::error(ErrorCode::BadRequest, message),
    }
}

impl Runtime {
    fn variant_caller(&self, from: &Caller) -> Option<(EnvironmentName, Option<GroupId>)> {
        let (environment, group) = match from {
            Caller::Terminal(TerminalHandle {
                environment,
                terminal,
            }) => {
                let environment = EnvironmentName::new(environment.clone());
                let data = self.environments.data(&environment)?;
                let group = data.group_of(TerminalId(*terminal)).map(|group| group.id);
                (environment, group)
            }
            Caller::Lead(LeadHandle {
                environment,
                session,
            }) => {
                let environment = EnvironmentName::new(environment.clone());
                let data = self.environments.data(&environment)?;
                let group = data
                    .session(SessionId(*session))
                    .and_then(|found| found.group);
                (environment, group)
            }
            Caller::Git(_) | Caller::Client => return None,
        };
        Some((environment, group))
    }

    pub(super) fn answer_variant_requests(&mut self, incoming: Incoming) -> Option<Incoming> {
        if !matches!(
            incoming.request,
            Request::RunVariants(_) | Request::Variants { .. } | Request::PickVariant { .. }
        ) {
            return Some(incoming);
        }
        let Some((environment, caller)) = self.variant_caller(&incoming.from) else {
            if matches!(incoming.from, Caller::Terminal(_) | Caller::Lead(_)) {
                reply_with(
                    incoming.reply,
                    Response::error(ErrorCode::NotFound, NO_ENVIRONMENT),
                );
                return None;
            }
            return Some(incoming);
        };
        let Incoming { request, reply, .. } = incoming;
        match request {
            Request::RunVariants(asked) => {
                self.run_variants_from_bus(&environment, caller, asked, reply);
            }
            Request::Variants { run } => {
                let shown = self.environments.data(&environment).map_or_default(|data| {
                    let runs = &data.variant_runs;
                    match run {
                        Some(run) => runs.run(VariantRunId(run)).cloned().into_iter().collect(),
                        None => caller.map_or_else(
                            || runs.all().to_vec(),
                            |group| runs.of_group(group).cloned().collect(),
                        ),
                    }
                });
                reply_with(reply, Response::Variants(shown));
            }
            Request::PickVariant {
                run,
                variant,
                outcome,
                confirmed,
            } => {
                if !confirmed {
                    reply_with(reply, Response::error(ErrorCode::BadRequest, NOT_CONFIRMED));
                    return None;
                }
                let picked = (VariantRunId(run), VariantId(variant), outcome);
                let removal = VariantRemoval::OnlyIfSettled;
                if let Err(refusal) = self.pick_variant(&environment, picked, removal, Some(reply))
                {
                    tracing::debug!(?refusal, "a bus pick was refused");
                }
            }
            _ => {}
        }
        None
    }

    fn run_variants_from_bus(
        &mut self,
        environment: &EnvironmentName,
        caller: Option<GroupId>,
        asked: VariantsRequest,
        reply: oneshot::Sender<Response>,
    ) {
        let VariantsRequest {
            prompt,
            variants,
            test,
            group,
        } = asked;
        let Some(data) = self.environments.data(environment) else {
            return reply_with(reply, Response::error(ErrorCode::NotFound, NO_ENVIRONMENT));
        };
        let group = match group.as_deref() {
            None => caller.ok_or_else(|| ApiError::new(ErrorCode::Unsupported, NO_CALLER_GROUP)),
            Some(name) => data
                .groups
                .iter()
                .find(|found| found.name == name)
                .map(|found| found.id)
                .ok_or_else(|| ApiError::new(ErrorCode::NotFound, NO_SUCH_GROUP)),
        };
        let group = match group {
            Ok(group) => group,
            Err(error) => return reply_with(reply, Response::Error(error)),
        };
        let Some(test) = test else {
            let order = VariantsOrder::new(&prompt, variants, None);
            return self.start_bus_variants(environment, group, order, reply);
        };
        let Some(root) = data.group(group).map(|found| found.root.clone()) else {
            return reply_with(reply, Response::error(ErrorCode::NotFound, NO_SUCH_GROUP));
        };
        let link = self.hosts.link(&data.host);
        let order = BusOrder {
            environment: environment.clone(),
            group,
            prompt,
            variants,
            test,
            reply,
        };
        let started = self.jobs.run(move || {
            let tasks = detect_tasks(&link, &root).tasks;
            JobDone::Variant(VariantJob::TasksRead { order, tasks })
        });
        if let Err(error) = started {
            tracing::warn!(%error, "{JOB_NOT_STARTED}");
        }
    }

    pub(super) fn variant_tasks_read(&mut self, order: BusOrder, tasks: Vec<ProjectTask>) {
        let BusOrder {
            environment,
            group,
            prompt,
            variants,
            test,
            reply,
        } = order;
        match chosen_task(tasks, &test) {
            Ok(task) => {
                let order = VariantsOrder::new(&prompt, variants, Some(task));
                self.start_bus_variants(&environment, group, order, reply);
            }
            Err(error) => reply_with(reply, Response::error(ErrorCode::BadRequest, error)),
        }
    }

    fn start_bus_variants(
        &mut self,
        environment: &EnvironmentName,
        group: GroupId,
        order: Result<VariantsOrder, krad_domain::VariantsOrderError>,
        reply: oneshot::Sender<Response>,
    ) {
        let response = match order {
            Err(error) => Response::error(ErrorCode::BadRequest, error.to_string()),
            Ok(order) => match self.launch_variants(environment, group, order) {
                Ok(run) => Response::VariantsStarted { run: run.0 },
                Err(refusal) => bus_error(refusal),
            },
        };
        reply_with(reply, response);
    }
}
