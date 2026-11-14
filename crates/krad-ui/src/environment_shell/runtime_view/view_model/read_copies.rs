use krad_protocol::EventKind;
use std::rc::Rc;

use super::{Heard, ViewModel};

impl ViewModel {
    pub(super) fn fold_read_copies(&mut self, kind: EventKind) {
        match kind {
            EventKind::EnvironmentOpened { environment } => {
                Rc::make_mut(&mut self.environments).opened((*environment).clone());
                self.heard(Heard::Event(EventKind::EnvironmentOpened { environment }));
            }
            EventKind::EnvironmentChanged {
                environment,
                changes,
                answers,
            } => {
                Rc::make_mut(&mut self.environments).changed(&environment, changes.clone());
                self.heard(Heard::Event(EventKind::EnvironmentChanged {
                    environment,
                    changes,
                    answers,
                }));
            }
            EventKind::EnvironmentClosed { environment } => {
                Rc::make_mut(&mut self.environments).closed(&environment);
                self.heard(Heard::Event(EventKind::EnvironmentClosed { environment }));
            }
            EventKind::EnvironmentsOrdered { order } => {
                Rc::make_mut(&mut self.environments).ordered(&order);
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
            kind => self.heard(Heard::Event(kind)),
        }
    }
}
