use super::{Worker, WorkerReopen};
use crate::environment::EnvironmentData;
use crate::ids::{SessionId, TerminalId};

const WORKER_SESSION_NAME: &str = "Worker";
const WORKER_NAME_CHARS: usize = 40;
const ELLIPSIS: char = '…';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerSpawn {
    pub lead: Option<SessionId>,
    pub task: String,
}

#[must_use]
pub fn worker_session_name(task: &str) -> String {
    let head = task.lines().map(str::trim).find(|line| !line.is_empty());
    let Some(head) = head else {
        return WORKER_SESSION_NAME.to_owned();
    };
    if head.chars().count() <= WORKER_NAME_CHARS {
        return head.to_owned();
    }
    let mut name: String = head.chars().take(WORKER_NAME_CHARS - 1).collect();
    name.truncate(name.trim_end().len());
    name.push(ELLIPSIS);
    name
}

impl EnvironmentData {
    pub fn open_worker_session(
        &mut self,
        terminal: TerminalId,
        spawn: WorkerSpawn,
    ) -> Option<SessionId> {
        let node = self.nodes.iter().find(|node| node.id == terminal)?;
        let group = self.home_of(node);
        let id = self.push_session(group, worker_session_name(&spawn.task));
        for session in &mut self.sessions {
            session.tabs.remove_terminal(terminal);
        }
        if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
            session.tabs.add_terminal(terminal);
        }
        if let Some(lead) = spawn.lead {
            self.own_worker(lead, terminal, id, spawn.task);
        }
        self.ensure_sessions();
        self.assign_terminals();
        Some(id)
    }

    fn own_worker(&mut self, lead: SessionId, terminal: TerminalId, at: SessionId, task: String) {
        let Some(lead) = self.lead_mut(lead) else {
            return;
        };
        match lead
            .workers
            .iter_mut()
            .find(|worker| worker.terminal == terminal)
        {
            Some(worker) => worker.session = Some(at),
            None => lead.workers.push(Worker {
                terminal,
                task,
                session: Some(at),
                reopen: None,
            }),
        }
    }

    #[must_use]
    pub fn worker(&self, terminal: TerminalId) -> Option<(SessionId, &Worker)> {
        self.sessions.iter().find_map(|session| {
            let lead = session.lead.as_ref()?;
            let worker = lead
                .workers
                .iter()
                .find(|worker| worker.terminal == terminal)?;
            Some((session.id, worker))
        })
    }

    fn worker_mut(&mut self, terminal: TerminalId) -> Option<&mut Worker> {
        self.sessions
            .iter_mut()
            .filter_map(|session| session.lead.as_mut())
            .flat_map(|lead| lead.workers.iter_mut())
            .find(|worker| worker.terminal == terminal)
    }

    #[must_use]
    pub fn open_workers(&self, lead: SessionId) -> Vec<TerminalId> {
        self.lead(lead).map_or_else(Vec::new, |lead| {
            lead.workers
                .iter()
                .filter(|worker| worker.session.is_some())
                .map(|worker| worker.terminal)
                .collect()
        })
    }

    #[must_use]
    pub fn workers_closing_with(&self, terminal: TerminalId) -> Vec<TerminalId> {
        self.session_led_by(terminal)
            .map_or_else(Vec::new, |lead| self.open_workers(lead))
    }

    pub fn retire_worker(&mut self, terminal: TerminalId) -> bool {
        let reopen = self
            .nodes
            .iter()
            .find(|node| node.id == terminal)
            .and_then(|node| {
                Some(WorkerReopen {
                    harness: node.harness?,
                    group: node.group,
                    resume: node.resume.clone()?,
                })
            });
        let Some(worker) = self.worker_mut(terminal) else {
            return false;
        };
        let session = worker.session.take();
        if reopen.is_some() {
            worker.reopen = reopen;
        }
        if let Some(session) = session {
            self.drop_worker_session(session, terminal);
        }
        true
    }

    pub fn forget_worker(&mut self, terminal: TerminalId) {
        let session = self.worker(terminal).and_then(|(_, worker)| worker.session);
        for lead in self
            .sessions
            .iter_mut()
            .filter_map(|session| session.lead.as_mut())
        {
            lead.workers.retain(|worker| worker.terminal != terminal);
        }
        if let Some(session) = session {
            self.drop_worker_session(session, terminal);
        }
    }

    fn drop_worker_session(&mut self, id: SessionId, terminal: TerminalId) {
        let Some(session) = self.session(id) else {
            return;
        };
        let others = session
            .tabs
            .tabs
            .iter()
            .flat_map(|tab| tab.tiles.iter())
            .any(|tile| tile.id != terminal);
        if others {
            return;
        }
        let group = session.group;
        self.sessions.retain(|session| session.id != id);
        if self.active_session == Some(id) {
            self.active_session = self.heir_session(group);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::seed_environment;
    use crate::group::SessionLead;
    use crate::ids::ResumeId;
    use crate::AgentPreset;
    use std::path::PathBuf;

    fn led_environment() -> (EnvironmentData, SessionId, TerminalId) {
        let mut environment = seed_environment();
        let api = environment.add_group(PathBuf::from("/work/api"), None);
        for node in &mut environment.nodes {
            node.group = Some(api);
        }
        environment.enter_sessions();
        let lead = environment
            .active_session
            .unwrap_or_else(|| unreachable!("Sessions mode opens a session"));
        if let Some(session) = environment
            .sessions
            .iter_mut()
            .find(|session| session.id == lead)
        {
            session.lead = Some(SessionLead::new(AgentPreset::ClaudeCode, None));
        }
        let worker = environment.nodes[1].id;
        (environment, lead, worker)
    }

    fn spawn(lead: SessionId) -> WorkerSpawn {
        WorkerSpawn {
            lead: Some(lead),
            task: "Fix the webhook retries\nwith tests".to_owned(),
        }
    }

    #[test]
    fn a_worker_opens_in_a_session_of_its_own_without_taking_the_selection() {
        let (mut environment, lead, worker) = led_environment();
        let opened = environment.open_worker_session(worker, spawn(lead));
        assert_eq!(environment.active_session, Some(lead));
        assert!(opened.is_some());
        assert_eq!(environment.session_of(worker), opened);
        let session = opened.and_then(|id| environment.session(id));
        assert_eq!(
            session.map(|session| session.name.as_str()),
            Some("Fix the webhook retries")
        );
        assert_eq!(environment.open_workers(lead), vec![worker]);
        assert_eq!(
            environment.worker(worker).map(|(owner, _)| owner),
            Some(lead)
        );
    }

    #[test]
    fn a_retired_worker_leaves_its_session_and_keeps_its_resume() {
        let (mut environment, lead, worker) = led_environment();
        let resume: ResumeId = "8d0d5ac1-7a3e-4f26-9b1c-2f4b3f1c9a10"
            .parse()
            .unwrap_or_else(|_| unreachable!("a valid resume id"));
        if let Some(node) = environment.nodes.iter_mut().find(|node| node.id == worker) {
            node.harness = Some(AgentPreset::ClaudeCode);
            node.resume = Some(resume.clone());
        }
        let opened = environment.open_worker_session(worker, spawn(lead));
        assert!(environment.retire_worker(worker));
        assert!(opened.and_then(|id| environment.session(id)).is_none());
        assert_eq!(environment.open_workers(lead), Vec::<TerminalId>::new());
        let reopen = environment
            .worker(worker)
            .and_then(|(_, worker)| worker.reopen.clone());
        assert_eq!(reopen.map(|reopen| reopen.resume), Some(resume));
    }

    #[test]
    fn a_forgotten_worker_is_gone_with_its_session() {
        let (mut environment, lead, worker) = led_environment();
        let opened = environment.open_worker_session(worker, spawn(lead));
        environment.forget_worker(worker);
        assert!(environment.worker(worker).is_none());
        assert!(opened.and_then(|id| environment.session(id)).is_none());
    }

    #[test]
    fn a_reopened_worker_is_the_same_record_in_a_new_session() {
        let (mut environment, lead, worker) = led_environment();
        environment.open_worker_session(worker, spawn(lead));
        environment.retire_worker(worker);
        let reopened = environment.open_worker_session(worker, spawn(lead));
        let workers = environment.lead(lead).map(|lead| lead.workers.len());
        assert_eq!(workers, Some(1));
        assert_eq!(
            environment
                .worker(worker)
                .and_then(|(_, worker)| worker.session),
            reopened
        );
    }

    #[test]
    fn closing_the_leads_tab_closes_its_open_workers() {
        let (mut environment, lead, worker) = led_environment();
        let lead_tab = environment.nodes[0].id;
        if let Some(lead) = environment
            .sessions
            .iter_mut()
            .find(|session| session.id == lead)
            .and_then(|session| session.lead.as_mut())
        {
            lead.terminal = Some(lead_tab);
        }
        environment.open_worker_session(worker, spawn(lead));
        assert_eq!(environment.workers_closing_with(lead_tab), vec![worker]);
        assert_eq!(
            environment.workers_closing_with(worker),
            Vec::<TerminalId>::new()
        );
        environment.retire_worker(worker);
        assert_eq!(
            environment.workers_closing_with(lead_tab),
            Vec::<TerminalId>::new()
        );
    }

    #[test]
    fn a_worker_session_is_named_after_the_first_line_of_its_task() {
        assert_eq!(worker_session_name("  \n  Fix login \nmore"), "Fix login");
        assert_eq!(worker_session_name(""), WORKER_SESSION_NAME);
        let long = "a".repeat(WORKER_NAME_CHARS + 5);
        let name = worker_session_name(&long);
        assert_eq!(name.chars().count(), WORKER_NAME_CHARS);
        assert!(name.ends_with(ELLIPSIS));
    }
}
