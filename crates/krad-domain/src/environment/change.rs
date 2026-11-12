use crate::archive::Archive;
use crate::environment::{EnvironmentData, TerminalMeta, TerminalNode};
use crate::group::{Session, TerminalGroup};
use crate::host::Host;
use crate::ids::{GroupId, SessionId, TerminalId};
use crate::usage::RetiredSpend;
use crate::variants::VariantRuns;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum EnvironmentChange {
    Node {
        index: usize,
        node: TerminalNode,
    },
    NodeRemoved {
        id: TerminalId,
    },
    Meta {
        terminal: TerminalId,
        meta: TerminalMeta,
    },
    MetaRemoved {
        terminal: TerminalId,
    },
    Group {
        index: usize,
        group: TerminalGroup,
    },
    GroupRemoved {
        id: GroupId,
    },
    Session {
        index: usize,
        session: Session,
    },
    SessionRemoved {
        id: SessionId,
    },
    ActiveSession {
        session: Option<SessionId>,
    },
    NextId {
        next: u64,
    },
    Title {
        title: Option<String>,
    },
    Host {
        host: Host,
    },
    RetiredSpend {
        spend: RetiredSpend,
    },
    VariantRuns {
        runs: VariantRuns,
    },
    Archive {
        archive: Archive,
    },
}

struct Placed<T> {
    index: usize,
    item: T,
}

struct ListChanges<T, K> {
    removed: Vec<K>,
    placed: Vec<Placed<T>>,
}

fn list_changes<T: Clone + PartialEq, K: PartialEq>(
    before: &[T],
    after: &[T],
    key: impl Fn(&T) -> K,
) -> ListChanges<T, K> {
    let removed: Vec<K> = before
        .iter()
        .map(&key)
        .filter(|gone| !after.iter().any(|item| key(item) == *gone))
        .collect();
    let mut current: Vec<T> = before
        .iter()
        .filter(|item| !removed.contains(&key(item)))
        .cloned()
        .collect();
    let mut placed = Vec::new();
    for (index, item) in after.iter().enumerate() {
        if current.get(index) == Some(item) {
            continue;
        }
        place(&mut current, index, item.clone(), &key);
        placed.push(Placed {
            index,
            item: item.clone(),
        });
    }
    ListChanges { removed, placed }
}

fn place<T, K: PartialEq>(list: &mut Vec<T>, index: usize, item: T, key: impl Fn(&T) -> K) {
    let wanted = key(&item);
    list.retain(|kept| key(kept) != wanted);
    list.insert(index.min(list.len()), item);
}

fn node_key(node: &TerminalNode) -> TerminalId {
    node.id
}

fn group_key(group: &TerminalGroup) -> GroupId {
    group.id
}

fn session_key(session: &Session) -> SessionId {
    session.id
}

impl EnvironmentChange {
    #[must_use]
    pub fn between(before: &EnvironmentData, after: &EnvironmentData) -> Vec<Self> {
        let mut changes = Vec::new();
        let nodes = list_changes(&before.nodes, &after.nodes, node_key);
        changes.extend(nodes.removed.into_iter().map(|id| Self::NodeRemoved { id }));
        changes.extend(nodes.placed.into_iter().map(|placed| Self::Node {
            index: placed.index,
            node: placed.item,
        }));
        let groups = list_changes(&before.groups, &after.groups, group_key);
        changes.extend(
            groups
                .removed
                .into_iter()
                .map(|id| Self::GroupRemoved { id }),
        );
        changes.extend(groups.placed.into_iter().map(|placed| Self::Group {
            index: placed.index,
            group: placed.item,
        }));
        let sessions = list_changes(&before.sessions, &after.sessions, session_key);
        changes.extend(
            sessions
                .removed
                .into_iter()
                .map(|id| Self::SessionRemoved { id }),
        );
        changes.extend(sessions.placed.into_iter().map(|placed| Self::Session {
            index: placed.index,
            session: placed.item,
        }));
        changes.extend(meta_changes(before, after));
        changes.extend(scalar_changes(before, after));
        changes
    }
}

fn meta_changes(
    before: &EnvironmentData,
    after: &EnvironmentData,
) -> impl Iterator<Item = EnvironmentChange> {
    let removed: Vec<EnvironmentChange> = before
        .meta
        .keys()
        .filter(|terminal| !after.meta.contains_key(terminal))
        .map(|&terminal| EnvironmentChange::MetaRemoved { terminal })
        .collect();
    let changed: Vec<EnvironmentChange> = after
        .meta
        .iter()
        .filter(|(terminal, meta)| before.meta.get(terminal) != Some(meta))
        .map(|(&terminal, meta)| EnvironmentChange::Meta {
            terminal,
            meta: meta.clone(),
        })
        .collect();
    removed.into_iter().chain(changed)
}

fn scalar_changes(before: &EnvironmentData, after: &EnvironmentData) -> Vec<EnvironmentChange> {
    let mut changes = Vec::new();
    if before.active_session != after.active_session {
        changes.push(EnvironmentChange::ActiveSession {
            session: after.active_session,
        });
    }
    if before.next_id != after.next_id {
        changes.push(EnvironmentChange::NextId {
            next: after.next_id,
        });
    }
    if before.title != after.title {
        changes.push(EnvironmentChange::Title {
            title: after.title.clone(),
        });
    }
    if before.host != after.host {
        changes.push(EnvironmentChange::Host {
            host: after.host.clone(),
        });
    }
    if before.retired_spend != after.retired_spend {
        changes.push(EnvironmentChange::RetiredSpend {
            spend: after.retired_spend.clone(),
        });
    }
    if before.variant_runs != after.variant_runs {
        changes.push(EnvironmentChange::VariantRuns {
            runs: after.variant_runs.clone(),
        });
    }
    if before.archive != after.archive {
        changes.push(EnvironmentChange::Archive {
            archive: after.archive.clone(),
        });
    }
    changes
}

impl EnvironmentData {
    pub fn apply(&mut self, change: EnvironmentChange) {
        match change {
            EnvironmentChange::Node { index, node } => {
                place(&mut self.nodes, index, node, node_key);
            }
            EnvironmentChange::NodeRemoved { id } => self.nodes.retain(|node| node.id != id),
            EnvironmentChange::Meta { terminal, meta } => {
                self.meta.insert(terminal, meta);
            }
            EnvironmentChange::MetaRemoved { terminal } => {
                self.meta.remove(&terminal);
            }
            EnvironmentChange::Group { index, group } => {
                place(&mut self.groups, index, group, group_key);
            }
            EnvironmentChange::GroupRemoved { id } => self.groups.retain(|group| group.id != id),
            EnvironmentChange::Session { index, session } => {
                place(&mut self.sessions, index, session, session_key);
            }
            EnvironmentChange::SessionRemoved { id } => {
                self.sessions.retain(|session| session.id != id);
            }
            EnvironmentChange::ActiveSession { session } => self.active_session = session,
            EnvironmentChange::NextId { next } => self.next_id = next,
            EnvironmentChange::Title { title } => self.title = title,
            EnvironmentChange::Host { host } => self.host = host,
            EnvironmentChange::RetiredSpend { spend } => self.retired_spend = spend,
            EnvironmentChange::VariantRuns { runs } => self.variant_runs = runs,
            EnvironmentChange::Archive { archive } => self.archive = archive,
        }
    }
}

#[cfg(test)]
mod tests;
