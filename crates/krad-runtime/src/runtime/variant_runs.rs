use krad_chat::{new_session_id, LiveSession};
use krad_domain::{
    AppEvent, EnvironmentName, GroupEdit, GroupId, NodeEdit, NodePlace, Prompt, TerminalId,
    VariantEvent, VariantId, VariantRemoval, VariantRunId, VariantWorktree, VariantsOrder,
    WorkerSpawn, Worktree, WorktreeOrigin,
};
use krad_protocol::{ErrorCode, Reply, VariantAct, VariantCommand};
use krad_repo::add_worktree;
use std::path::Path;
use std::time::SystemTime;

use super::working::agent;
use super::{refused, Runtime, UNSUPPORTED_COMMAND};
use crate::chats::{ChatBrief, ChatRole, ChatStart};
use crate::environments::UNKNOWN_ENVIRONMENT;
use crate::jobs::JobDone;
use crate::variants::{VariantJob, VariantLaunch};
use crate::workers::{runs_as_worker, worker_node};

const NOT_A_REPO_TEXT: &str = "Variants need a group inside a git repository";
const NOT_A_WORKER_TEXT: &str = "That agent cannot run as a variant";
const NO_CHAT_TEXT: &str = "The variant's chat could not open";
const JOB_NOT_STARTED: &str = "the variant's work could not start";
const SESSION_SEPARATOR: &str = " · ";

impl Runtime {
    pub(super) fn execute_variant(&mut self, command: VariantCommand) -> Reply {
        let VariantCommand { environment, act } = command;
        if !self.environments.knows(&environment) {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        }
        let done = match act {
            VariantAct::Run { group, order } => {
                self.launch_variants(&environment, group, *order).map(drop)
            }
            VariantAct::Pick {
                run,
                winner,
                outcome,
                removal,
            } => self.pick_variant(&environment, (run, winner, outcome), removal, None),
            VariantAct::Retest { run, variant } => {
                self.finish_variant(&environment, run, variant);
                Ok(())
            }
            VariantAct::CheckWorktrees => {
                self.check_variant_worktrees(&environment);
                Ok(())
            }
            VariantAct::Unknown => Err(refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND)),
        };
        done.map_or_else(|refusal| refusal, |()| Reply::Accepted)
    }

    pub(super) fn launch_variants(
        &mut self,
        environment: &EnvironmentName,
        group: GroupId,
        order: VariantsOrder,
    ) -> Result<VariantRunId, Reply> {
        let data = self
            .environments
            .data(environment)
            .ok_or_else(|| refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT))?;
        let repo_root = data
            .group(group)
            .and_then(|found| self.git.at(&data.host, &found.root))
            .map(|git| git.repo_root.clone())
            .ok_or_else(|| refused(ErrorCode::Unsupported, NOT_A_REPO_TEXT))?;
        if !order
            .variants
            .iter()
            .all(|spec| runs_as_worker(spec.harness))
        {
            return Err(refused(ErrorCode::Unsupported, NOT_A_WORKER_TEXT));
        }
        let count = order.variants.len();
        let mut started = Err(String::new());
        self.environments.variant_runs(
            environment,
            |runs| {
                started = runs
                    .start(group, repo_root.clone(), order, SystemTime::now())
                    .map_err(|error| error.to_string());
                started.is_ok()
            },
            &mut self.outbox,
        );
        let run = started.map_err(|error| refused(ErrorCode::Invalid, &error))?;
        let launches: Vec<VariantLaunch> = self
            .environments
            .data(environment)
            .and_then(|data| data.variant_runs.run(run))
            .map_or_default(|started| {
                started
                    .variants
                    .iter()
                    .map(|variant| VariantLaunch {
                        environment: environment.clone(),
                        repo_root: repo_root.clone(),
                        run,
                        variant: variant.id,
                        name: started.worktree_name(variant.id),
                    })
                    .collect()
            });
        tracing::info!(run = run.0, variants = count, "variants started");
        let started = AppEvent::VariantsStarted {
            run,
            variants: count,
        };
        self.events
            .publish_own(environment, Some(group), &started, &mut self.outbox);
        for launch in launches {
            self.add_variant_worktree(launch);
        }
        Ok(run)
    }

    fn add_variant_worktree(&mut self, launch: VariantLaunch) {
        let Some(host) = self.environments.host_of(&launch.environment) else {
            return;
        };
        let link = self.hosts.link(&host);
        let failed = launch.clone();
        let started = self.jobs.run(move || {
            let created = add_worktree(&link, &launch.repo_root, &launch.name)
                .map_err(|error| error.to_string());
            JobDone::Variant(VariantJob::WorktreeAdded { launch, created })
        });
        if let Err(error) = started {
            tracing::warn!(%error, "could not start a variant's worktree");
            self.variant_failed(&failed, JOB_NOT_STARTED);
        }
    }

    pub(super) fn variant_worktree_added(
        &mut self,
        launch: &VariantLaunch,
        created: Result<Worktree, String>,
    ) {
        match created {
            Ok(worktree) => self.open_variant(launch, worktree),
            Err(error) => self.variant_failed(launch, &error),
        }
    }

    fn variant_failed(&mut self, launch: &VariantLaunch, error: &str) {
        tracing::warn!(
            run = launch.run.0,
            variant = launch.variant.0,
            "variant could not start"
        );
        let event = VariantEvent::LaunchFailed(error.to_owned());
        self.apply_variant_event(&launch.environment, (launch.run, launch.variant), event);
    }

    fn open_variant(&mut self, launch: &VariantLaunch, worktree: Worktree) {
        let environment = &launch.environment;
        let branch = worktree
            .branch
            .clone()
            .unwrap_or_else(|| launch.name.clone());
        let spec = self
            .environments
            .data(environment)
            .and_then(|data| data.variant_runs.run(launch.run))
            .and_then(|run| {
                Some((
                    run.variant(launch.variant)?.spec.clone(),
                    run.prompt.clone(),
                ))
            });
        let Some((spec, prompt)) = spec else {
            return self.roll_back_variant(launch, (&worktree.path, &branch), None);
        };
        let origin = WorktreeOrigin {
            repo_root: launch.repo_root.clone(),
            branch: branch.clone(),
        };
        let Some(group) = self.add_variant_group(environment, &worktree.path, origin) else {
            self.roll_back_variant(launch, (&worktree.path, &branch), None);
            return self.variant_failed(launch, NO_CHAT_TEXT);
        };
        let harness = self.settings.current.acp.harness_name(spec.harness);
        let task = format!(
            "{}{SESSION_SEPARATOR}{}",
            launch.variant,
            spec.label(&harness)
        );
        let brief = VariantsOrder::brief(&prompt, &spec);
        let Some(terminal) = self.open_variant_chat(environment, group, (&spec, task, brief))
        else {
            self.roll_back_variant(launch, (&worktree.path, &branch), Some(group));
            return self.variant_failed(launch, NO_CHAT_TEXT);
        };
        let event = VariantEvent::Launched {
            worktree: VariantWorktree {
                path: worktree.path,
                branch,
                group,
            },
            terminal,
            at: SystemTime::now(),
        };
        self.apply_variant_event(environment, (launch.run, launch.variant), event);
    }

    fn add_variant_group(
        &mut self,
        environment: &EnvironmentName,
        root: &Path,
        origin: WorktreeOrigin,
    ) -> Option<GroupId> {
        let group = self.environments.data(environment)?.next_group_id();
        let added = GroupEdit::Add {
            group,
            root: root.to_path_buf(),
            worktree: Some(origin),
        };
        self.environments
            .edit(environment.clone(), &added.into(), None, &mut self.outbox);
        self.git.refresh(environment);
        self.group_with_root(environment, root)
    }

    fn open_variant_chat(
        &mut self,
        environment: &EnvironmentName,
        group: GroupId,
        (spec, task, brief): (&krad_domain::VariantSpec, String, String),
    ) -> Option<TerminalId> {
        let data = self.environments.data(environment)?;
        let terminal = TerminalId(data.next_id);
        let session = data.next_session_id();
        let label = self.settings.current.acp.harness_name(spec.harness);
        let opened = NodeEdit::Open {
            node: Box::new(worker_node(terminal, &label, spec.harness, Some(group))),
            place: NodePlace::Worker {
                session,
                spawn: WorkerSpawn { lead: None, task },
                cloud: None,
            },
        };
        self.environments
            .edit(environment.clone(), &opened.into(), None, &mut self.outbox);
        self.environments.data(environment)?.session_of(terminal)?;
        let start = ChatStart {
            harness: spec.harness,
            brief: ChatBrief {
                model: spec.model.clone(),
                role: ChatRole::Agent,
                ..ChatBrief::default()
            },
            prepared: None,
            session: LiveSession::Start(new_session_id()),
            messages: Vec::new(),
            queued: vec![Prompt::from(brief)],
            notices: Vec::new(),
        };
        self.open_chat(&agent(environment, terminal), start);
        Some(terminal)
    }

    fn roll_back_variant(
        &mut self,
        launch: &VariantLaunch,
        (worktree, branch): (&Path, &str),
        group: Option<GroupId>,
    ) {
        if let Some(group) = group {
            self.remove_group(&launch.environment, group);
        }
        let worktrees = vec![(worktree.to_path_buf(), branch.to_owned())];
        self.discard_variant_worktrees(
            &launch.environment,
            launch.repo_root.clone(),
            worktrees,
            VariantRemoval::Forced,
        );
    }

    pub(super) fn apply_variant_event(
        &mut self,
        environment: &EnvironmentName,
        (run, variant): (VariantRunId, VariantId),
        event: VariantEvent,
    ) -> bool {
        let mut finished = None;
        let applied = self.environments.variant_runs(
            environment,
            |runs| {
                let Some(found) = runs.run_mut(run) else {
                    return false;
                };
                match found.variant_mut(variant).map(|target| target.apply(event)) {
                    Some(Ok(())) => {}
                    Some(Err(error)) => {
                        tracing::debug!(%error, run = run.0, variant = variant.0, "variant event ignored");
                        return false;
                    }
                    None => return false,
                }
                if found.take_finished_announcement() {
                    finished = Some(found.group);
                }
                true
            },
            &mut self.outbox,
        );
        if let Some(group) = finished {
            let event = AppEvent::VariantsFinished { run };
            self.events
                .publish_own(environment, Some(group), &event, &mut self.outbox);
        }
        applied == Some(true)
    }
}
