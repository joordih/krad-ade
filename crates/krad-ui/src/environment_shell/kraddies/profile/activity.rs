use gpui_kit::{AppContext as _, Context};
use krad_domain::{KraddieDay, KraddieId, KraddieWake};
use krad_repo::{line_counts, LineCount};

use super::super::super::EnvironmentShell;
use super::{today, EndedRun};

const fn counts_as_routine(wake: KraddieWake) -> bool {
    !wake.by_user() && !matches!(wake, KraddieWake::Rounds)
}

pub(super) fn wake_day(wake: KraddieWake) -> KraddieDay {
    KraddieDay {
        runs: 1,
        routines: u32::from(counts_as_routine(wake)),
        ..KraddieDay::default()
    }
}

fn lines_day(counts: impl Iterator<Item = LineCount>) -> KraddieDay {
    let total = counts.fold(LineCount::default(), |sum, count| sum + count);
    KraddieDay {
        lines_added: u64::from(total.added),
        lines_removed: u64::from(total.removed),
        ..KraddieDay::default()
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn count_kraddie_day(
        &mut self,
        id: &KraddieId,
        delta: &KraddieDay,
        cx: &mut Context<Self>,
    ) {
        let profile = &mut self.kraddies.profile;
        profile
            .activity
            .entry(id.clone())
            .or_default()
            .record(today(), delta);
        profile.touched_activity(id);
        self.save_kraddie_profiles(cx);
    }

    pub(in crate::environment_shell) fn count_kraddie_wake(
        &mut self,
        id: &KraddieId,
        wake: KraddieWake,
        cx: &mut Context<Self>,
    ) {
        if wake == KraddieWake::Rounds {
            return;
        }
        self.count_kraddie_day(id, &wake_day(wake), cx);
    }

    pub(in crate::environment_shell) fn count_kraddie_session(
        &mut self,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) {
        let session = KraddieDay {
            sessions: 1,
            ..KraddieDay::default()
        };
        self.count_kraddie_day(id, &session, cx);
    }

    pub(in crate::environment_shell) fn settle_ended_kraddie_runs(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        for ended in std::mem::take(&mut self.kraddies.profile.ended) {
            if self.settle_check_in(&ended, cx) {
                continue;
            }
            self.count_run_spend(&ended, cx);
        }
    }

    fn count_run_spend(&mut self, ended: &EndedRun, cx: &mut Context<Self>) {
        let Some(live) = self.envs.get(&ended.env) else {
            return;
        };
        let spend = live.data.chat_spend(ended.terminal);
        let root = live
            .data
            .group_of_chat(ended.terminal)
            .and_then(|group| live.data.group(group))
            .map(|group| group.root.clone());
        if !spend.is_empty() {
            let paid = KraddieDay {
                spend,
                ..KraddieDay::default()
            };
            self.count_kraddie_day(&ended.kraddie, &paid, cx);
        }
        let Some(root) = root else {
            return;
        };
        let link = self.env_link(&ended.env);
        let count = cx.background_spawn(async move { line_counts(&link, &root) });
        let kraddie = ended.kraddie.clone();
        cx.spawn(async move |this, cx| {
            let counted = count.await;
            this.update(cx, |shell, cx| match counted {
                Ok(counts) => {
                    let day = lines_day(counts.into_values());
                    if day.lines_changed() > 0 {
                        shell.count_kraddie_day(&kraddie, &day, cx);
                    }
                }
                Err(error) => {
                    tracing::debug!(kraddie = %kraddie, %error, "kraddie run lines not counted");
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn reset_kraddie_conversation(
        &mut self,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) {
        self.kraddies.profile.resetting = None;
        if let Some(entry) = self.kraddies.entry_mut(id) {
            entry.check_ins = krad_domain::KraddieHistory::default();
            let keys: Vec<krad_domain::AutomationKey> = entry
                .automations
                .iter_mut()
                .map(|routine| {
                    routine.history = krad_domain::KraddieHistory::default();
                    krad_domain::AutomationKey::new(id.clone(), routine.automation.id)
                })
                .collect();
            self.kraddies.dirty.extend(keys);
        }
        self.kraddies.profile.last_check_in.remove(id);
        self.save_histories(cx);
        tracing::info!(kraddie = %id, "kraddie conversation reset");
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::KraddieEvent;

    #[test]
    fn automation_trigger_wakes_count_as_routines() {
        assert_eq!(wake_day(KraddieWake::Schedule).routines, 1);
        assert_eq!(wake_day(KraddieWake::CatchUp).routines, 1);
        assert_eq!(
            wake_day(KraddieWake::Event(KraddieEvent::CiFailed)).routines,
            1
        );
        assert_eq!(wake_day(KraddieWake::Manual).routines, 0);
        assert_eq!(wake_day(KraddieWake::Rounds).routines, 0);
        assert_eq!(wake_day(KraddieWake::Cli).runs, 1);
    }

    #[test]
    fn line_counts_of_a_worktree_add_up() {
        let day = lines_day(
            [
                LineCount {
                    added: 3,
                    removed: 1,
                },
                LineCount {
                    added: 7,
                    removed: 0,
                },
            ]
            .into_iter(),
        );
        assert_eq!((day.lines_added, day.lines_removed), (10, 1));
    }
}
