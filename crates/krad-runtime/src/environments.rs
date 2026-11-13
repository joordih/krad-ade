use futures::channel::oneshot;
use krad_domain::{
    BudgetStatus, EnvironmentChange, EnvironmentData, EnvironmentName, Host, RuntimeState,
    SpendSettings, TerminalId, TerminalMeta, UsageReport,
};
use krad_protocol::{AgentRef, EnvironmentCommand, ErrorCode, EventKind, Reply, SpendingCrossed};
use krad_store::{config_dir, LayoutFile, LayoutWriter};
use std::path::PathBuf;

use crate::runtime::refused;

const UNKNOWN_ENVIRONMENT: &str = "this runtime has no environment of that name";

#[derive(Clone, Debug, Default)]
pub struct EnvironmentsSetup {
    pub state: RuntimeState,
    pub file: Option<PathBuf>,
}

impl EnvironmentsSetup {
    #[must_use]
    pub fn in_config_folder(state: RuntimeState) -> Self {
        Self {
            state,
            file: config_dir().map(|dir| LayoutFile::State.path_in(&dir)),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Environments {
    state: RuntimeState,
    writer: Option<LayoutWriter>,
    dirty: bool,
}

struct UsageRecord {
    chat: TerminalId,
    report: UsageReport,
    spending: SpendSettings,
}

impl Environments {
    pub(crate) fn start(setup: EnvironmentsSetup) -> Self {
        if !setup.state.unreadable.is_empty() {
            tracing::warn!(
                count = setup.state.unreadable.len(),
                "kept environments this version cannot read; they stay in the state file"
            );
        }
        Self {
            state: setup.state,
            writer: setup.file.and_then(LayoutWriter::spawn_at),
            dirty: false,
        }
    }

    pub(crate) fn execute(
        &mut self,
        command: EnvironmentCommand,
        outbox: &mut Vec<EventKind>,
    ) -> Option<Reply> {
        let reply = match command {
            EnvironmentCommand::Open { environment } => {
                self.open(*environment, outbox);
                Reply::Accepted
            }
            EnvironmentCommand::Close { environment } => self.close(environment, outbox),
            EnvironmentCommand::Order { order } => {
                self.order(&order, outbox);
                Reply::Accepted
            }
            EnvironmentCommand::Change {
                environment,
                changes,
            } => self.change(environment, changes, outbox),
            EnvironmentCommand::RecordUsage {
                environment,
                chat,
                report,
                spending,
            } => self.record_usage(
                environment,
                &UsageRecord {
                    chat,
                    report,
                    spending,
                },
                outbox,
            ),
            EnvironmentCommand::Unknown => return None,
        };
        Some(reply)
    }

    pub(crate) fn flush(&mut self) {
        if !std::mem::take(&mut self.dirty) {
            return;
        }
        if let Some(writer) = &self.writer {
            writer.persist(&self.state);
        }
    }

    pub(crate) fn settle(&mut self, reply: oneshot::Sender<()>) {
        self.flush();
        let settled = move || {
            if reply.send(()).is_err() {
                tracing::trace!("nobody waited for the state to settle");
            }
        };
        match &self.writer {
            Some(writer) => writer.then(settled),
            None => settled(),
        }
    }

    pub(crate) fn snapshot(&self) -> Vec<EnvironmentData> {
        self.state.environments.clone()
    }

    pub(crate) fn knows(&self, name: &EnvironmentName) -> bool {
        self.state
            .environments
            .iter()
            .any(|environment| environment.name == *name)
    }

    pub(crate) fn host_of(&self, name: &EnvironmentName) -> Option<Host> {
        self.state
            .environments
            .iter()
            .find(|environment| environment.name == *name)
            .map(|environment| environment.host.clone())
    }

    pub(crate) fn uses_host(&self, host: &Host) -> bool {
        self.state
            .environments
            .iter()
            .any(|environment| environment.host == *host)
    }

    fn find_mut(&mut self, name: &EnvironmentName) -> Option<&mut EnvironmentData> {
        self.state
            .environments
            .iter_mut()
            .find(|environment| environment.name == *name)
    }

    fn open(&mut self, environment: EnvironmentData, outbox: &mut Vec<EventKind>) {
        let heard = Box::new(environment.clone());
        match self.find_mut(&environment.name) {
            Some(kept) => *kept = environment,
            None => self.state.environments.push(environment),
        }
        self.dirty = true;
        outbox.push(EventKind::EnvironmentOpened { environment: heard });
    }

    fn close(&mut self, environment: EnvironmentName, outbox: &mut Vec<EventKind>) -> Reply {
        let before = self.state.environments.len();
        self.state
            .environments
            .retain(|kept| kept.name != environment);
        if self.state.environments.len() == before {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        }
        self.dirty = true;
        outbox.push(EventKind::EnvironmentClosed { environment });
        Reply::Accepted
    }

    fn order(&mut self, order: &[EnvironmentName], outbox: &mut Vec<EventKind>) {
        let rank = |name: &EnvironmentName| {
            order
                .iter()
                .position(|wanted| wanted == name)
                .unwrap_or(order.len())
        };
        let before: Vec<EnvironmentName> = self.names();
        self.state
            .environments
            .sort_by_key(|environment| rank(&environment.name));
        let after = self.names();
        if after == before {
            return;
        }
        self.dirty = true;
        outbox.push(EventKind::EnvironmentsOrdered { order: after });
    }

    fn names(&self) -> Vec<EnvironmentName> {
        self.state
            .environments
            .iter()
            .map(|environment| environment.name.clone())
            .collect()
    }

    fn change(
        &mut self,
        environment: EnvironmentName,
        changes: Vec<EnvironmentChange>,
        outbox: &mut Vec<EventKind>,
    ) -> Reply {
        let Some(kept) = self.find_mut(&environment) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let (owned, mut changes): (Vec<_>, Vec<_>) = changes
            .into_iter()
            .partition(EnvironmentChange::is_runtime_owned);
        if !owned.is_empty() {
            tracing::debug!(
                count = owned.len(),
                "a client changed fields the runtime owns; ignored"
            );
        }
        for change in &changes {
            kept.apply(change.clone());
        }
        let pruned: Vec<EnvironmentChange> = changes
            .iter()
            .filter_map(|change| match change {
                EnvironmentChange::NodeRemoved { id } => kept
                    .meta
                    .remove(id)
                    .map(|_| EnvironmentChange::MetaRemoved { terminal: *id }),
                _ => None,
            })
            .collect();
        changes.extend(pruned);
        if changes.is_empty() {
            return Reply::Accepted;
        }
        self.dirty = true;
        outbox.push(EventKind::EnvironmentChanged {
            environment,
            changes,
        });
        Reply::Accepted
    }

    pub(crate) fn put_meta(
        &mut self,
        terminal: AgentRef,
        meta: TerminalMeta,
        outbox: &mut Vec<EventKind>,
    ) {
        let Some(kept) = self.find_mut(&terminal.environment) else {
            return;
        };
        if kept.meta.get(&terminal.terminal) == Some(&meta) {
            return;
        }
        kept.meta.insert(terminal.terminal, meta.clone());
        self.dirty = true;
        outbox.push(EventKind::EnvironmentChanged {
            environment: terminal.environment,
            changes: vec![EnvironmentChange::Meta {
                terminal: terminal.terminal,
                meta,
            }],
        });
    }

    fn record_usage(
        &mut self,
        environment: EnvironmentName,
        record: &UsageRecord,
        outbox: &mut Vec<EventKind>,
    ) -> Reply {
        let Some(kept) = self.find_mut(&environment) else {
            return refused(ErrorCode::NotFound, UNKNOWN_ENVIRONMENT);
        };
        let group = kept.group_of_chat(record.chat);
        let before = kept.group_spend(group).cost;
        kept.record_usage(record.chat, &record.report, &record.spending.model_prices());
        let after = kept.group_spend(group).cost;
        let Some((index, node)) = kept
            .nodes
            .iter()
            .enumerate()
            .find(|(_, node)| node.id == record.chat)
        else {
            return Reply::Accepted;
        };
        let changes = vec![EnvironmentChange::Node {
            index,
            node: node.clone(),
        }];
        self.dirty = true;
        let spending = &record.spending;
        let crossed = spending
            .status(before)
            .zip(spending.status(after))
            .and_then(|(before, after)| BudgetStatus::crossed(before, after))
            .zip(spending.group_budget);
        outbox.push(EventKind::EnvironmentChanged {
            environment: environment.clone(),
            changes,
        });
        if let Some((status, budget)) = crossed {
            outbox.push(EventKind::SpendingCrossed(SpendingCrossed {
                environment,
                group,
                status,
                spent: after,
                budget,
            }));
        }
        Reply::Accepted
    }
}
