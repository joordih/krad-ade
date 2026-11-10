use krad_domain::{
    AppEvent, AppTrigger, AutomationEvent, CodeHost, CodeHostTrigger, CodeHostWake,
    EnvironmentName, GroupId, KraddieOutcome, KraddieWake, KraddieWorkspace, SessionId, TerminalId,
};

use super::super::EnvironmentShell;
use super::{KraddieWakeOrder, LiveRun};

impl EnvironmentShell {
    pub(in crate::environment_shell) fn observe_kraddie_event(
        &mut self,
        env: &EnvironmentName,
        group: Option<GroupId>,
        event: &AppEvent,
    ) {
        self.follow_kraddie_run(env, event);
        let Some(trigger) = AppTrigger::of(event) else {
            return;
        };
        if self.raised_by_a_kraddie(env, group, event) {
            return;
        }
        self.wake_on(env, group, AutomationEvent::App(trigger));
    }

    pub(in crate::environment_shell) fn observe_code_host_event(
        &mut self,
        env: &EnvironmentName,
        group: GroupId,
        host: CodeHost,
        event: &AppEvent,
    ) {
        let Some(event) = CodeHostTrigger::of(event) else {
            return;
        };
        let wake = CodeHostWake { host, event };
        self.wake_on(env, Some(group), AutomationEvent::CodeHost(wake));
    }

    pub(in crate::environment_shell) fn automations_want_code_host(&self) -> bool {
        self.kraddies.entries.iter().any(|kraddie| {
            kraddie.automations.iter().any(|entry| {
                entry.automation.switch.is_on()
                    && entry
                        .automation
                        .triggers
                        .iter()
                        .any(|trigger| trigger.code_host().is_some())
            })
        })
    }

    fn wake_on(&mut self, env: &EnvironmentName, group: Option<GroupId>, event: AutomationEvent) {
        let Some(repo) = group.and_then(|group| self.event_repo(env, group)) else {
            return;
        };
        let place = KraddieWorkspace {
            environment: env.clone(),
            repo,
        };
        let woken: Vec<KraddieWakeOrder> = self
            .kraddies
            .entries
            .iter()
            .filter(|kraddie| kraddie.kraddie.works_in(&place))
            .flat_map(|kraddie| {
                kraddie
                    .automations
                    .iter()
                    .map(move |entry| (kraddie, entry))
            })
            .filter(|(_, entry)| entry.automation.listens_for(event))
            .map(|(kraddie, entry)| KraddieWakeOrder {
                key: kraddie.key(entry.automation.id),
                wake: KraddieWake::from(event),
                prompt: None,
            })
            .collect();
        self.kraddies.wakes.extend(woken);
    }

    fn raised_by_a_kraddie(
        &self,
        env: &EnvironmentName,
        group: Option<GroupId>,
        event: &AppEvent,
    ) -> bool {
        let from_kraddie_group =
            group.is_some_and(|group| self.kraddies.groups.contains(&(env.clone(), group)));
        let from_kraddie_terminal = event_terminal(event)
            .is_some_and(|terminal| self.kraddies.runs.contains_key(&(env.clone(), terminal)));
        from_kraddie_group || from_kraddie_terminal
    }

    fn event_repo(&self, env: &EnvironmentName, group: GroupId) -> Option<std::path::PathBuf> {
        let root = &self.envs.get(env)?.data.group(group)?.root;
        Some(
            self.git_at(env, root)
                .map_or_else(|| root.clone(), |git| git.repo_root.clone()),
        )
    }

    fn follow_kraddie_run(&mut self, env: &EnvironmentName, event: &AppEvent) {
        let (terminal, outcome) = match event {
            AppEvent::AgentStarted { terminal, .. } => (*terminal, KraddieOutcome::Working),
            AppEvent::AgentWaiting { terminal } => (*terminal, KraddieOutcome::NeedsYou),
            AppEvent::AgentFinished { terminal } => (*terminal, KraddieOutcome::Finished),
            AppEvent::SessionClosed { session } => {
                return self.close_kraddie_runs_in(env, *session);
            }
            _ => return,
        };
        let Some(run) = self.kraddies.runs.get(&(env.clone(), terminal)).cloned() else {
            return;
        };
        let ended = !outcome.is_live();
        self.kraddies.settle(&run.owner, run.run, outcome);
        if ended {
            self.kraddies.runs.remove(&(env.clone(), terminal));
            self.kraddies.profile.ended.push(super::profile::EndedRun {
                env: env.clone(),
                terminal,
                kraddie: run.owner.kraddie().clone(),
                run: run.run,
            });
        }
    }

    fn close_kraddie_runs_in(&mut self, env: &EnvironmentName, session: SessionId) {
        let closed: Vec<(TerminalId, LiveRun)> = self
            .kraddies
            .runs
            .iter()
            .filter(|((run_env, _), run)| {
                run_env == env
                    && self
                        .kraddies
                        .owner_history(&run.owner)
                        .is_some_and(|history| {
                            history
                                .newest_first()
                                .find(|past| past.id == run.run)
                                .and_then(|past| past.session.as_ref())
                                .is_some_and(|link| link.session == session)
                        })
            })
            .map(|((_, terminal), run)| (*terminal, run.clone()))
            .collect();
        for (terminal, run) in closed {
            self.kraddies
                .settle(&run.owner, run.run, KraddieOutcome::Interrupted);
            self.kraddies.runs.remove(&(env.clone(), terminal));
            self.kraddies.profile.forget_check_in(env, terminal);
        }
    }
}

const fn event_terminal(event: &AppEvent) -> Option<TerminalId> {
    match event {
        AppEvent::AgentStarted { terminal, .. }
        | AppEvent::AgentWaiting { terminal }
        | AppEvent::AgentFinished { terminal } => Some(*terminal),
        _ => None,
    }
}
