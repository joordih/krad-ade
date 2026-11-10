use super::Session;
use crate::environment::EnvironmentData;
use crate::ids::{GroupId, SessionId, TerminalId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhenWorking {
    #[default]
    Refuse,
    CancelTurn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CloseSubject {
    Worker(TerminalId),
    Session(SessionId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CloseRefusal {
    Unknown,
    AlreadyClosed,
    NotOwned,
    Itself,
    Working,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosePlan {
    pub subject: CloseSubject,
    pub terminals: Vec<TerminalId>,
    pub cancel: Vec<TerminalId>,
    pub worktree: Option<GroupId>,
}

fn tiles_of(session: &Session) -> impl Iterator<Item = TerminalId> + '_ {
    session
        .tabs
        .tabs
        .iter()
        .flat_map(|tab| tab.tiles.iter().map(|tile| tile.id))
}

impl EnvironmentData {
    /// # Errors
    ///
    /// Returns the [`CloseRefusal`] that keeps the lead from closing it.
    pub fn plan_close(
        &self,
        lead: SessionId,
        subject: CloseSubject,
        when_working: WhenWorking,
        busy: impl Fn(TerminalId) -> bool,
    ) -> Result<ClosePlan, CloseRefusal> {
        let (terminals, group) = match subject {
            CloseSubject::Worker(terminal) => {
                self.closable_worker(lead, terminal)?;
                let group = self
                    .nodes
                    .iter()
                    .find(|node| node.id == terminal)
                    .and_then(|node| node.group);
                (vec![terminal], group)
            }
            CloseSubject::Session(session) => self.closable_session(lead, session)?,
        };
        let cancel: Vec<TerminalId> = terminals
            .iter()
            .copied()
            .filter(|terminal| busy(*terminal))
            .collect();
        if !cancel.is_empty() && when_working == WhenWorking::Refuse {
            return Err(CloseRefusal::Working);
        }
        let worktree = group.filter(|group| {
            self.group(*group)
                .is_some_and(|group| group.worktree.is_some())
        });
        Ok(ClosePlan {
            subject,
            terminals,
            cancel,
            worktree,
        })
    }

    #[must_use]
    pub fn idle_workers(
        &self,
        lead: SessionId,
        busy: impl Fn(TerminalId) -> bool,
    ) -> Vec<TerminalId> {
        self.open_workers(lead)
            .into_iter()
            .filter(|terminal| !busy(*terminal))
            .collect()
    }

    pub fn drop_terminal(&mut self, id: TerminalId) {
        self.retire_worker(id);
        self.nodes.retain(|node| node.id != id);
        self.meta.remove(&id);
        self.remove_terminal_tiles(id);
    }

    fn lead_terminal(&self, lead: SessionId) -> Option<TerminalId> {
        self.lead(lead)?.terminal
    }

    fn closable_worker(&self, lead: SessionId, terminal: TerminalId) -> Result<(), CloseRefusal> {
        if self.lead_terminal(lead) == Some(terminal) {
            return Err(CloseRefusal::Itself);
        }
        match self.worker(terminal) {
            Some((owner, _)) if owner != lead => Err(CloseRefusal::NotOwned),
            Some((_, worker)) if worker.session.is_none() => Err(CloseRefusal::AlreadyClosed),
            Some(_) => Ok(()),
            None if self.nodes.iter().any(|node| node.id == terminal) => {
                Err(CloseRefusal::NotOwned)
            }
            None => Err(CloseRefusal::Unknown),
        }
    }

    fn closable_session(
        &self,
        lead: SessionId,
        id: SessionId,
    ) -> Result<(Vec<TerminalId>, Option<GroupId>), CloseRefusal> {
        if id == lead {
            return Err(CloseRefusal::Itself);
        }
        let session = self.session(id).ok_or(CloseRefusal::Unknown)?;
        let terminals: Vec<TerminalId> = tiles_of(session).collect();
        let lead_terminal = self.lead_terminal(lead);
        if lead_terminal.is_some_and(|terminal| terminals.contains(&terminal)) {
            return Err(CloseRefusal::Itself);
        }
        let owners: Vec<SessionId> = terminals
            .iter()
            .filter_map(|terminal| self.worker(*terminal))
            .map(|(owner, _)| owner)
            .collect();
        let foreign = session.lead.is_some() || owners.iter().any(|owner| *owner != lead);
        let same_group = self
            .session(lead)
            .is_some_and(|lead| lead.group == session.group);
        if foreign || !(same_group || owners.contains(&lead)) {
            return Err(CloseRefusal::NotOwned);
        }
        Ok((terminals, session.group))
    }
}

#[cfg(test)]
mod tests;
