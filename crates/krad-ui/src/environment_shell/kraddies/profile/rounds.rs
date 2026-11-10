use chrono::{DateTime, Local};
use gpui_kit::Context;
use krad_chat::Speaker;
use krad_domain::{
    is_quiet_reply, BudgetStatus, CheckInContext, EnvironmentName, GroupId, Hush, KraddieId,
    KraddieWake, KraddieWorkspace, RoundsCheck, RoundsMoment, RunSlot, TerminalId,
    MAX_KRADDIE_RUNS,
};
use std::path::Path;
use std::time::SystemTime;

use super::super::super::chat::{AnswerPart, ChatMessage};
use super::super::super::EnvironmentShell;
use super::super::KraddieEntry;
use super::EndedRun;
use crate::local_clock;

const CONTEXT_RUNS: usize = 8;
const CONTEXT_NOTES: usize = 12;
const CONTEXT_INBOX: usize = 12;
const CONTEXT_SEPARATOR: &str = " · ";
const RUN_FORMAT: &str = "%a %d %b %H:%M";

pub(super) fn final_reply(messages: &[ChatMessage]) -> Option<String> {
    let last = messages
        .iter()
        .rev()
        .find(|message| message.speaker == Speaker::Assistant)?;
    let text: Vec<&str> = last
        .parts
        .iter()
        .filter_map(|part| match part {
            AnswerPart::Text(text) => Some(text.as_str()),
            AnswerPart::Thinking(_) | AnswerPart::Tool(_) | AnswerPart::Ask(_) => None,
        })
        .collect();
    Some(if text.is_empty() {
        last.text.clone()
    } else {
        text.join("\n")
    })
}

fn quiet_since(
    entry: &KraddieEntry,
    opened_at: SystemTime,
    last_check_in: Option<SystemTime>,
) -> SystemTime {
    [entry.last_started(), last_check_in]
        .into_iter()
        .flatten()
        .fold(opened_at, SystemTime::max)
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn check_in_quiet_kraddies(&mut self, cx: &mut Context<Self>) {
        let now = SystemTime::now();
        let quiet_hours = self
            .settings
            .attention
            .quiet_hours
            .contains(local_clock::now());
        let at_capacity = self.kraddies.limiter.len() >= MAX_KRADDIE_RUNS;
        let due: Vec<KraddieId> = self
            .kraddies
            .entries
            .iter()
            .filter(|entry| {
                let id = entry.kraddie.id();
                let running = self.kraddies.limiter.has_kraddie_running(&id)
                    || self
                        .kraddies
                        .profile
                        .check_ins
                        .values()
                        .any(|kraddie| *kraddie == id);
                let moment = RoundsMoment {
                    quiet_since: quiet_since(
                        entry,
                        self.kraddies.opened_at,
                        self.kraddies.profile.last_check_in.get(&id).copied(),
                    ),
                    now,
                    kraddie: entry.kraddie.switch,
                    slot: match (running, at_capacity) {
                        (true, _) => RunSlot::KraddieRunning,
                        (false, true) => RunSlot::AllBusy,
                        (false, false) => RunSlot::Free,
                    },
                    hush: if quiet_hours {
                        Hush::QuietHours
                    } else if self.kraddie_over_budget(entry.kraddie.workspace.as_ref()) {
                        Hush::OverBudget
                    } else {
                        Hush::None
                    },
                };
                entry.kraddie.settings.rounds.check(&moment) == RoundsCheck::Due
            })
            .map(|entry| entry.kraddie.id())
            .collect();
        for kraddie in due {
            self.kraddies
                .profile
                .last_check_in
                .insert(kraddie.clone(), now);
            self.check_in_kraddie(&kraddie, cx);
        }
    }

    pub(in crate::environment_shell) fn next_check_in(
        &self,
        entry: &KraddieEntry,
    ) -> Option<SystemTime> {
        if !entry.kraddie.switch.is_on() {
            return None;
        }
        let id = entry.kraddie.id();
        let since = quiet_since(
            entry,
            self.kraddies.opened_at,
            self.kraddies.profile.last_check_in.get(&id).copied(),
        );
        entry.kraddie.settings.rounds.next_after(since)
    }

    fn kraddie_over_budget(&self, workspace: Option<&KraddieWorkspace>) -> bool {
        let Some(workspace) = workspace else {
            return false;
        };
        let Some(live) = self.envs.get(&workspace.environment) else {
            return false;
        };
        let group = self.repo_group(&workspace.environment, &workspace.repo);
        let spent = live.data.group_spend(group).cost;
        self.settings.spending.status(spent) == Some(BudgetStatus::Exceeded)
    }

    pub(in crate::environment_shell) fn repo_group(
        &self,
        env: &EnvironmentName,
        repo: &Path,
    ) -> Option<GroupId> {
        let live = self.envs.get(env)?;
        live.data
            .groups
            .iter()
            .find(|group| group.root == repo && group.worktree.is_none())
            .map(|group| group.id)
    }

    pub(in crate::environment_shell) fn check_in_context(&self, id: &KraddieId) -> CheckInContext {
        let Some(entry) = self.kraddies.entry(id) else {
            return CheckInContext::default();
        };
        let runs = entry.runs_newest_first();
        let recent_work = runs
            .iter()
            .filter(|run| run.wake != KraddieWake::Rounds)
            .take(CONTEXT_RUNS)
            .map(|run| {
                let at = DateTime::<Local>::from(run.started)
                    .format(RUN_FORMAT)
                    .to_string();
                [at.as_str(), run.wake.label(), run.outcome.label()].join(CONTEXT_SEPARATOR)
            })
            .collect();
        let sessions_left = runs
            .iter()
            .filter_map(|run| run.session.as_ref())
            .filter_map(|link| {
                let live = self.envs.get(&link.environment)?;
                let session = live.data.session(link.session)?;
                Some(session.name.clone())
            })
            .collect();
        let notes = self
            .notes
            .index
            .notes()
            .iter()
            .filter(|note| !note.generated)
            .take(CONTEXT_NOTES)
            .map(|note| note.title.clone())
            .collect();
        let inbox = self
            .attention_records()
            .take(CONTEXT_INBOX)
            .map(|(kind, title)| [kind, title].join(CONTEXT_SEPARATOR))
            .collect();
        CheckInContext {
            recent_work,
            sessions_left,
            notes,
            inbox,
        }
    }

    pub(in crate::environment_shell) fn note_check_in(
        &mut self,
        env: &EnvironmentName,
        terminal: TerminalId,
        kraddie: &KraddieId,
    ) {
        self.kraddies
            .profile
            .check_ins
            .insert((env.clone(), terminal), kraddie.clone());
    }

    pub(super) fn settle_check_in(&mut self, ended: &EndedRun, cx: &mut Context<Self>) -> bool {
        let key = (ended.env.clone(), ended.terminal);
        let Some(kraddie) = self.kraddies.profile.check_ins.remove(&key) else {
            return false;
        };
        let reply = self
            .envs
            .get(&ended.env)
            .and_then(|live| live.chats.get(&ended.terminal))
            .and_then(|chat| final_reply(&chat.messages))
            .unwrap_or_default();
        if !is_quiet_reply(&reply) {
            tracing::info!(kraddie = %kraddie, "kraddie check-in suggested something");
            return true;
        }
        tracing::debug!(kraddie = %kraddie, "kraddie check-in had nothing to suggest");
        self.forget_attention(&ended.env, ended.terminal);
        let session = self
            .envs
            .get(&ended.env)
            .and_then(|live| live.data.session_of(ended.terminal));
        self.close_terminal_in(&ended.env, ended.terminal, cx);
        if let (Some(session), Some(live)) = (session, self.envs.get_mut(&ended.env)) {
            live.data.close_session(session);
        }
        if let Some(entry) = self.kraddies.entry_mut(&ended.kraddie) {
            entry.check_ins.forget(ended.run);
        }
        self.store();
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AgentPreset, KraddieOutcome};
    use std::time::Duration;

    #[test]
    fn a_kraddie_has_been_quiet_since_its_last_run_check_in_or_the_app_opening() {
        let opened = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let mut entry = KraddieEntry::new(
            krad_domain::Kraddie::new(
                "Quiet".parse().unwrap_or_else(|_| unreachable!()),
                AgentPreset::ClaudeCode,
            ),
            Vec::new(),
        );
        assert_eq!(quiet_since(&entry, opened, None), opened);
        let ran = opened + Duration::from_secs(50);
        entry
            .check_ins
            .start(ran, KraddieWake::Rounds, KraddieOutcome::Finished);
        assert_eq!(quiet_since(&entry, opened, None), ran);
        let checked = ran + Duration::from_secs(10);
        assert_eq!(quiet_since(&entry, opened, Some(checked)), checked);
        let before = SystemTime::UNIX_EPOCH;
        let mut stale = KraddieEntry::new(entry.kraddie.clone(), Vec::new());
        stale
            .check_ins
            .start(before, KraddieWake::Rounds, KraddieOutcome::Finished);
        assert_eq!(quiet_since(&stale, opened, None), opened);
    }
}
