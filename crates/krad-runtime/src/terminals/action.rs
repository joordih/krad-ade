use futures::channel::oneshot;
use krad_session::{CellPoint, KeyModifiers, MouseAction, SearchDirection};

#[derive(Debug)]
pub enum TerminalAction {
    Key {
        key: String,
        text: Option<String>,
        held: KeyModifiers,
    },
    Wheel {
        lines: i32,
        at: CellPoint,
    },
    Press {
        at: CellPoint,
        report: bool,
    },
    Mouse {
        action: MouseAction,
        at: CellPoint,
    },
    ExtendSelection(CellPoint),
    ClearSelection,
    Search {
        query: String,
        direction: SearchDirection,
        restart: bool,
        found: oneshot::Sender<bool>,
    },
    CopySelection(oneshot::Sender<Option<String>>),
    RecentLines {
        lines: usize,
        tail: oneshot::Sender<Option<Vec<String>>>,
    },
    Hooked,
}

impl TerminalAction {
    pub(crate) fn unanswered(self) {
        let heard = match self {
            Self::Search { found, .. } => found.send(false).is_ok(),
            Self::CopySelection(text) => text.send(None).is_ok(),
            Self::RecentLines { tail, .. } => tail.send(None).is_ok(),
            Self::Key { .. }
            | Self::Wheel { .. }
            | Self::Press { .. }
            | Self::Mouse { .. }
            | Self::ExtendSelection(_)
            | Self::ClearSelection
            | Self::Hooked => true,
        };
        if !heard {
            tracing::trace!("a client left before the terminal answered");
        }
    }
}
