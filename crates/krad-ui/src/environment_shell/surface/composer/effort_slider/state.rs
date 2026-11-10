use krad_domain::{Effort, TerminalId};

use super::layout::{popover_key, stepped, PopoverKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PopoverMove {
    Close,
    Choose(Effort),
    Redraw,
    Ignore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PopoverState {
    pub(super) chat: TerminalId,
    pub(super) help: bool,
    pub(super) dragging: bool,
}

impl PopoverState {
    pub(super) const fn new(chat: TerminalId) -> Self {
        Self {
            chat,
            help: false,
            dragging: false,
        }
    }

    pub(super) fn press(
        &mut self,
        key: &str,
        steps: &[Effort],
        current: Option<Effort>,
    ) -> PopoverMove {
        match popover_key(key) {
            None => PopoverMove::Ignore,
            Some(PopoverKey::Close) => PopoverMove::Close,
            Some(PopoverKey::Help) => {
                self.help = !self.help;
                PopoverMove::Redraw
            }
            Some(PopoverKey::Slide(slide)) => stepped(steps, current, slide)
                .filter(|effort| current != Some(*effort))
                .map_or(PopoverMove::Ignore, PopoverMove::Choose),
        }
    }

    pub(super) fn grab(&mut self, at: Effort, current: Option<Effort>) -> PopoverMove {
        self.dragging = true;
        Self::choose(at, current)
    }

    pub(super) fn drag_over(&self, at: Effort, current: Option<Effort>, held: bool) -> PopoverMove {
        if !(self.dragging && held) {
            return PopoverMove::Ignore;
        }
        Self::choose(at, current)
    }

    pub(super) fn release(&mut self) {
        self.dragging = false;
    }

    fn choose(at: Effort, current: Option<Effort>) -> PopoverMove {
        if current == Some(at) {
            PopoverMove::Ignore
        } else {
            PopoverMove::Choose(at)
        }
    }
}

#[cfg(test)]
mod tests;
