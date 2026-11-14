use krad_domain::EnvironmentName;
use krad_protocol::EventKind;
use std::rc::Rc;

use super::super::NodesMoved;
use super::{Heard, ViewModel};

impl ViewModel {
    pub(super) fn fold_read_copies(&mut self, kind: EventKind) {
        match kind {
            EventKind::EnvironmentOpened { environment } => {
                Rc::make_mut(&mut self.environments).opened((*environment).clone());
                let moved = self.shown.opened(&environment);
                self.nodes_moved(&environment.name, moved);
                self.heard(Heard::Event(EventKind::EnvironmentOpened { environment }));
            }
            EventKind::EnvironmentChanged {
                environment,
                changes,
                answers,
            } => {
                Rc::make_mut(&mut self.environments).changed(&environment, changes.clone());
                if let Some(runtime) = self.environments.get(&environment) {
                    let moved = self.shown.heard(&environment, &changes, answers, runtime);
                    self.nodes_moved(&environment, moved);
                }
                self.heard(Heard::Event(EventKind::EnvironmentChanged {
                    environment,
                    changes,
                    answers,
                }));
            }
            EventKind::EnvironmentClosed { environment } => {
                Rc::make_mut(&mut self.environments).closed(&environment);
                self.shown.closed(&environment);
                self.heard(Heard::Event(EventKind::EnvironmentClosed { environment }));
            }
            EventKind::EnvironmentsOrdered { order } => {
                Rc::make_mut(&mut self.environments).ordered(&order);
                self.shown.ordered(&order);
                self.heard(Heard::Event(EventKind::EnvironmentsOrdered { order }));
            }
            EventKind::KraddieChanged { kraddie } => {
                self.kraddies.changed(&kraddie);
                self.heard(Heard::Event(EventKind::KraddieChanged { kraddie }));
            }
            EventKind::KraddieRemoved { kraddie } => {
                self.kraddies.removed(&kraddie);
                self.heard(Heard::Event(EventKind::KraddieRemoved { kraddie }));
            }
            EventKind::GitRead(read) => {
                if let Some(moved) = self.git.read(read) {
                    self.heard(Heard::GitMoved(moved));
                }
            }
            EventKind::DigestsChanged(owned) => {
                self.kraddies.digests(&owned);
                self.heard(Heard::Event(EventKind::DigestsChanged(owned)));
            }
            kind => {
                if kind.chat().is_some() {
                    Rc::make_mut(&mut self.chats).heard(&kind);
                }
                self.heard(Heard::Event(kind));
            }
        }
    }

    pub(super) fn nodes_moved(&mut self, environment: &EnvironmentName, moved: NodesMoved) {
        if !moved.is_empty() {
            self.heard(Heard::NodesMoved {
                environment: environment.clone(),
                moved,
            });
        }
    }
}

#[cfg(test)]
mod tests;
