use krad_protocol::{AgentRef, EventKind, WriteBy};

use super::terminal::Written;
use super::{TerminalAction, Terminals};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Typed {
    pub(crate) running: bool,
    pub(crate) hooked: bool,
    pub(crate) bracketed: bool,
}

impl Terminals {
    pub(crate) fn knows(&self, terminal: &AgentRef) -> bool {
        self.live.contains_key(terminal)
    }

    pub(crate) fn typed(&self, terminal: &AgentRef) -> Option<Typed> {
        let live = self.live.get(terminal)?;
        Some(Typed {
            running: live.run().is_running(),
            hooked: live.is_hooked(),
            bracketed: live.bracketed_paste(),
        })
    }

    pub(crate) fn mark_hooked(&mut self, terminal: &AgentRef) {
        if let Some(live) = self.live.get_mut(terminal) {
            live.act(TerminalAction::Hooked);
        }
    }

    pub(crate) fn recent_lines(
        &mut self,
        terminal: &AgentRef,
        lines: usize,
    ) -> Option<Vec<String>> {
        self.live
            .get_mut(terminal)
            .map(|live| live.recent_lines(lines))
    }

    pub(crate) fn type_app(
        &mut self,
        terminal: &AgentRef,
        bytes: Vec<u8>,
        outbox: &mut Vec<EventKind>,
    ) {
        let Some(live) = self.live.get_mut(terminal) else {
            return;
        };
        if live.write(bytes, WriteBy::App) == Written::Lost && !live.run().is_running() {
            self.ended(terminal, outbox, |_| {});
        }
    }
}
