use krad_chat::{AttachRefusal, ChatEvent, CloudRun, LiveEvent};
use krad_domain::{AgentPreset, CloudRunnerId};

use super::backend::Unready;
use super::chat::{Batch, Chat};
use super::{ChatPlace, ChatWork};
use crate::cloud_runs::{
    placement_of, saved_process, site_of, CloudSite, NodeSite, StartWait, CONNECTING_PREFIX,
    REMOVED_RUNNER,
};

const TRIMMED_TEXT: &str =
    "The cloud runner trimmed this chat's journal, so it attached again from its start";

#[derive(Clone, Debug)]
pub(super) struct CloudChat {
    pub(super) site: Box<CloudSite>,
    pub(super) run: Option<CloudRun>,
}

impl From<StartWait> for Unready {
    fn from(wait: StartWait) -> Self {
        match wait {
            StartWait::Waiting => Self::Waiting,
            StartWait::Failed(problem) => Self::Problem(problem),
        }
    }
}

pub(super) fn cloud_place(site: &CloudSite) -> ChatPlace {
    ChatPlace {
        host: site.host.clone(),
        link: site.link.clone(),
        root: Some(site.placement.worktree.clone()),
        cloud: Some(site.placement.clone()),
    }
}

pub(super) fn runner_wait(runner: CloudRunnerId, chat: &mut Chat, work: &mut ChatWork<'_>) -> Unready {
    let Some(found) = work.facts.settings.cloud.find(runner) else {
        return Unready::Problem(REMOVED_RUNNER.to_owned());
    };
    work.cloud.want(runner);
    chat.problem = Some(format!("{CONNECTING_PREFIX} {}", found.name.as_str()));
    Unready::Waiting
}

fn waits_for_runner(problem: Option<&str>) -> bool {
    problem.is_some_and(|problem| problem.starts_with(CONNECTING_PREFIX))
}

pub(super) fn reach(chat: &mut Chat, work: &mut ChatWork<'_>) -> Result<(), Unready> {
    chat.cloud = None;
    let placement = placement_of(work.facts.environments, &chat.id);
    let site = match site_of(placement, work.facts.runners) {
        NodeSite::Env => return Ok(()),
        NodeSite::Waiting(runner) => return Err(runner_wait(runner, chat, work)),
        NodeSite::Cloud(site) => site,
    };
    if work.facts.runners.buses.is_preparing(site.placement.runner) {
        return Err(Unready::Waiting);
    }
    if waits_for_runner(chat.problem.as_deref()) {
        chat.problem = None;
    }
    chat.place = cloud_place(&site);
    if let Some(reason) = chat.harness.unavailable_in_cloud() {
        return Err(Unready::Problem(reason.to_owned()));
    }
    let run = match chat.harness {
        AgentPreset::Opencode => None,
        harness => {
            let program = work.facts.cloud_program(harness)?;
            let saved = saved_process(work.facts.environments, &chat.id);
            let start = work.cloud.start(&chat.id, &site, saved, work.jobs)?;
            let run = site.run(program, &start);
            work.cloud.attach(chat.id.clone(), run.clone());
            Some(run)
        }
    };
    chat.cloud = Some(CloudChat { site, run });
    Ok(())
}

const fn ends_attach(event: &LiveEvent) -> bool {
    matches!(
        event,
        LiveEvent::Chat(ChatEvent::Error(_) | ChatEvent::Done)
    )
}

const fn past_end(refusal: Option<AttachRefusal>) -> bool {
    matches!(refusal, Some(AttachRefusal::PastEnd { .. }))
}

pub(super) fn reattaches(
    chat: &mut Chat,
    event: &LiveEvent,
    work: &mut ChatWork<'_>,
) -> Option<(Batch, ChatEvent)> {
    let refusal = chat
        .cloud
        .as_ref()
        .and_then(|cloud| cloud.run.as_ref())
        .and_then(CloudRun::refusal);
    if !ends_attach(event) || !past_end(refusal) || !work.cloud.rewind(&chat.id) {
        return None;
    }
    tracing::info!("attaching to the cloud chat again from the journal's start");
    chat.live = None;
    chat.outstanding = 0;
    chat.working = false;
    let batch = chat.last_batch.take().unwrap_or_default();
    Some((batch, ChatEvent::Error(TRIMMED_TEXT.to_owned())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::JournalOffset;

    #[test]
    fn only_an_ended_attach_past_the_journal_end_attaches_again() {
        let trimmed = Some(AttachRefusal::PastEnd {
            end: JournalOffset(10),
        });
        assert!(past_end(trimmed));
        assert!(!past_end(Some(AttachRefusal::Refused)));
        assert!(!past_end(None));
        assert!(ends_attach(&LiveEvent::Chat(ChatEvent::Done)));
        assert!(!ends_attach(&LiveEvent::Notice(String::new())));
    }

    #[test]
    fn only_the_connecting_note_clears_once_the_runner_is_ready() {
        assert!(waits_for_runner(Some("Connecting to cloud runner gpu")));
        assert!(!waits_for_runner(Some("claude is not on PATH")));
        assert!(!waits_for_runner(None));
    }
}
