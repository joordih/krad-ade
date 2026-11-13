use krad_domain::{
    final_reply, is_quiet_reply, AppEvent, AppTrigger, AttentionTarget, AutomationEvent,
    BudgetStatus, EnvironmentName, EventRecord, GroupId, KraddieDay, KraddieId, KraddieOutcome,
    KraddieWorkspace, NodeEdit, SessionEdit, TerminalId,
};
use krad_protocol::{AgentRef, AttentionCommand, EventKind};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use super::Runtime;
use crate::chats::Launch;
use crate::jobs::JobDone;
use crate::kraddies::LiveRun;

#[derive(Debug)]
pub(crate) struct LinesCounted {
    pub(crate) kraddie: KraddieId,
    pub(crate) counted: Result<HashMap<PathBuf, krad_domain::LineCount>, String>,
}

const fn event_terminal(event: &AppEvent) -> Option<TerminalId> {
    match event {
        AppEvent::AgentStarted { terminal, .. }
        | AppEvent::AgentWaiting { terminal }
        | AppEvent::AgentFinished { terminal } => Some(*terminal),
        _ => None,
    }
}

impl Runtime {
    pub(super) fn catch_up_kraddies(&mut self) {
        let missed = self.kraddies.missed();
        self.wake_kraddies(missed);
    }

    pub(super) fn tick_kraddies(&mut self) {
        self.tick_digests(Instant::now());
        let now = self.kraddies.now();
        if !self.kraddies.schedule_due(now.at) {
            return;
        }
        self.check_in_quiet_kraddies(now);
        let due = self.kraddies.due(now);
        self.wake_kraddies(due);
    }

    fn outcome_now(&self, chat: &AgentRef) -> Option<KraddieOutcome> {
        if !self.chats.is_open(chat) {
            return Some(KraddieOutcome::Interrupted);
        }
        match self.chats.launch(chat) {
            Launch::Pending => None,
            Launch::Failed(reason) => Some(KraddieOutcome::Failed { reason }),
            Launch::Started if self.chats.is_asking(chat) => Some(KraddieOutcome::NeedsYou),
            Launch::Started if self.chats.is_busy(chat) => Some(KraddieOutcome::Working),
            Launch::Started => Some(KraddieOutcome::Finished),
        }
    }

    pub(super) fn follow_kraddie_runs(&mut self) {
        let runs: Vec<(AgentRef, LiveRun)> = self
            .kraddies
            .runs
            .iter()
            .map(|(chat, run)| (chat.clone(), run.clone()))
            .collect();
        for (chat, run) in runs {
            let Some(outcome) = self.outcome_now(&chat) else {
                continue;
            };
            if self.kraddies.outcome_of(&run.owner, run.run) == Some(&outcome) {
                continue;
            }
            let ended = !outcome.is_live();
            let interrupted = outcome == KraddieOutcome::Interrupted;
            self.kraddies.settle_run(&run.owner, run.run, outcome);
            if !ended {
                continue;
            }
            self.kraddies.runs.remove(&chat);
            if interrupted {
                self.kraddies.check_ins.remove(&chat);
                continue;
            }
            if !self.settle_check_in(&chat, &run) {
                self.count_run_spend(&chat, run.owner.kraddie());
            }
        }
    }

    fn settle_check_in(&mut self, chat: &AgentRef, run: &LiveRun) -> bool {
        let Some(kraddie) = self.kraddies.check_ins.remove(chat) else {
            return false;
        };
        let reply = self
            .chats
            .tail(chat)
            .and_then(final_reply)
            .unwrap_or_default();
        if !is_quiet_reply(&reply) {
            tracing::info!(kraddie = %kraddie, "kraddie check-in suggested something");
            return true;
        }
        tracing::debug!(kraddie = %kraddie, "kraddie check-in had nothing to suggest");
        let target = AttentionTarget::new(chat.environment.clone(), chat.terminal);
        drop(
            self.attention
                .execute(&AttentionCommand::Dismiss { target }, &mut self.outbox),
        );
        let session = self
            .environments
            .data(&chat.environment)
            .and_then(|data| data.session_of(chat.terminal));
        self.chats.close(chat, &mut self.outbox);
        self.terminals.close(chat);
        let closed = NodeEdit::Close {
            terminal: chat.terminal,
        };
        self.environments.edit(
            chat.environment.clone(),
            &closed.into(),
            None,
            &mut self.outbox,
        );
        if let Some(session) = session {
            self.environments.edit(
                chat.environment.clone(),
                &SessionEdit::Close { session }.into(),
                None,
                &mut self.outbox,
            );
        }
        self.kraddies.forget_check_in(&kraddie, run.run);
        true
    }

    fn count_run_spend(&mut self, chat: &AgentRef, kraddie: &KraddieId) {
        let Some(data) = self.environments.data(&chat.environment) else {
            return;
        };
        let spend = data.chat_spend(chat.terminal);
        let root = data
            .group_of_chat(chat.terminal)
            .and_then(|group| data.group(group))
            .map(|group| group.root.clone());
        let host = data.host.clone();
        if !spend.is_empty() {
            let paid = KraddieDay {
                spend,
                ..KraddieDay::default()
            };
            self.kraddies.count_day(kraddie, &paid);
        }
        let Some(root) = root else {
            return;
        };
        let link = self.hosts.link(&host);
        let kraddie = kraddie.clone();
        let started = self.jobs.run(move || {
            JobDone::KraddieLines(LinesCounted {
                kraddie,
                counted: krad_repo::line_counts(&link, &root).map_err(|error| error.to_string()),
            })
        });
        if let Err(error) = started {
            tracing::debug!(%error, "kraddie run lines not counted");
        }
    }

    pub(super) fn kraddie_lines_counted(&mut self, counted: LinesCounted) {
        match counted.counted {
            Ok(counts) => {
                let day = KraddieDay::of_lines(counts.into_values());
                if day.lines_changed() > 0 {
                    self.kraddies.count_day(&counted.kraddie, &day);
                }
            }
            Err(error) => {
                tracing::debug!(kraddie = %counted.kraddie, %error, "kraddie run lines not counted");
            }
        }
    }

    pub(super) fn hear_kraddie_events(&mut self, unread: usize) {
        let records: Vec<EventRecord> = self
            .outbox
            .get(unread..)
            .unwrap_or_default()
            .iter()
            .filter_map(|kind| match kind {
                EventKind::AppEvent(record) => Some(record.clone()),
                _ => None,
            })
            .collect();
        for record in records {
            let Some(trigger) = AppTrigger::of(&record.event) else {
                continue;
            };
            if self.raised_by_a_kraddie(&record.environment, record.group, &record.event) {
                continue;
            }
            self.wake_on(
                &record.environment,
                record.group,
                AutomationEvent::App(trigger),
            );
        }
    }

    fn raised_by_a_kraddie(
        &self,
        environment: &EnvironmentName,
        group: Option<GroupId>,
        event: &AppEvent,
    ) -> bool {
        let from_group =
            group.is_some_and(|group| self.kraddies.groups.contains(&(environment.clone(), group)));
        let from_terminal = event_terminal(event).is_some_and(|terminal| {
            self.kraddies.runs.contains_key(&AgentRef {
                environment: environment.clone(),
                terminal,
            })
        });
        from_group || from_terminal
    }

    pub(super) fn wake_on(
        &mut self,
        environment: &EnvironmentName,
        group: Option<GroupId>,
        event: AutomationEvent,
    ) {
        let Some(repo) = group.and_then(|group| self.event_repo(environment, group)) else {
            return;
        };
        let place = KraddieWorkspace {
            environment: environment.clone(),
            repo,
        };
        let woken = self.kraddies.woken_by(&place, event);
        self.wake_kraddies(woken);
    }

    fn event_repo(&self, environment: &EnvironmentName, group: GroupId) -> Option<PathBuf> {
        let data = self.environments.data(environment)?;
        let root = &data.group(group)?.root;
        Some(
            self.git
                .at(&data.host, root)
                .map_or_else(|| root.clone(), |git| git.repo_root.clone()),
        )
    }

    pub(super) fn kraddie_over_budget(&self, workspace: Option<&KraddieWorkspace>) -> bool {
        let Some(workspace) = workspace else {
            return false;
        };
        let Some(data) = self.environments.data(&workspace.environment) else {
            return false;
        };
        let group = self.repo_group(&workspace.environment, &workspace.repo);
        let spent = data.group_spend(group).cost;
        self.settings.current.spending.status(spent) == Some(BudgetStatus::Exceeded)
    }
}
