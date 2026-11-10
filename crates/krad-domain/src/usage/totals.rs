use super::ledger::ChatUsage;
use super::prices::ModelPrices;
use super::report::{Spend, UsageReport};
use crate::environment::EnvironmentData;
use crate::group::{Session, Worker};
use crate::ids::{GroupId, TerminalId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupSpend {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<GroupId>,
    pub spent: Spend,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RetiredSpend(Vec<GroupSpend>);

impl RetiredSpend {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn of(&self, group: Option<GroupId>) -> Spend {
        self.0
            .iter()
            .filter(|kept| kept.group == group)
            .map(|kept| kept.spent)
            .sum()
    }

    fn entry(&mut self, group: Option<GroupId>) -> &mut Spend {
        let at = self
            .0
            .iter()
            .position(|kept| kept.group == group)
            .unwrap_or_else(|| {
                self.0.push(GroupSpend {
                    group,
                    spent: Spend::default(),
                });
                self.0.len() - 1
            });
        &mut self.0[at].spent
    }

    fn add(&mut self, group: Option<GroupId>, spent: Spend) {
        if spent.is_empty() {
            return;
        }
        *self.entry(group) += spent;
    }

    fn take(&mut self, group: Option<GroupId>, spent: Spend) {
        let kept = self.entry(group);
        *kept = kept.less(spent);
        self.0.retain(|kept| !kept.spent.is_empty());
    }

    pub(crate) fn forget(&mut self, group: GroupId) {
        self.0.retain(|kept| kept.group != Some(group));
    }
}

impl EnvironmentData {
    pub fn record_usage(&mut self, chat: TerminalId, report: &UsageReport, prices: &ModelPrices) {
        if let Some(node) = self.nodes.iter_mut().find(|node| node.id == chat) {
            node.usage.record(report, prices);
        }
    }

    #[must_use]
    pub fn chat_spend(&self, chat: TerminalId) -> Spend {
        self.nodes
            .iter()
            .find(|node| node.id == chat)
            .map_or_else(Spend::default, |node| node.usage.spent())
    }

    #[must_use]
    pub fn session_spend(&self, session: &Session) -> Spend {
        session
            .tabs
            .tabs
            .iter()
            .flat_map(|tab| tab.tiles.iter())
            .map(|tile| self.chat_spend(tile.id))
            .sum()
    }

    #[must_use]
    pub fn group_spend(&self, group: Option<GroupId>) -> Spend {
        let open: Spend = self
            .sessions_of(group)
            .map(|session| self.session_spend(session))
            .sum();
        open + self.retired_spend.of(group)
    }

    #[must_use]
    pub fn worker_spend(&self, worker: &Worker) -> Spend {
        if self.nodes.iter().any(|node| node.id == worker.terminal) {
            return self.chat_spend(worker.terminal);
        }
        worker.spent
    }

    #[must_use]
    pub fn group_of_chat(&self, chat: TerminalId) -> Option<GroupId> {
        match self.session_of(chat) {
            Some(session) => self.session(session).and_then(|session| session.group),
            None => self
                .nodes
                .iter()
                .find(|node| node.id == chat)
                .and_then(|node| self.home_of(node)),
        }
    }

    pub fn retire_chat_usage(&mut self, chat: TerminalId) {
        let spent = self.chat_spend(chat);
        if spent.is_empty() {
            return;
        }
        let group = self.group_of_chat(chat);
        self.retired_spend.add(group, spent);
        if let Some(worker) = self.worker_mut(chat) {
            worker.spent = spent;
        }
        if let Some(node) = self.nodes.iter_mut().find(|node| node.id == chat) {
            node.usage = ChatUsage::default();
        }
    }

    pub fn resume_worker_usage(&mut self, chat: TerminalId) {
        let Some(spent) = self
            .worker_mut(chat)
            .map(|worker| std::mem::take(&mut worker.spent))
        else {
            return;
        };
        if spent.is_empty() {
            return;
        }
        let group = self.group_of_chat(chat);
        self.retired_spend.take(group, spent);
        if let Some(node) = self.nodes.iter_mut().find(|node| node.id == chat) {
            node.usage = ChatUsage::continuing(spent);
        }
    }
}

#[cfg(test)]
mod tests;
