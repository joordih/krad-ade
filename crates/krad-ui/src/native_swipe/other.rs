use futures::channel::mpsc::UnboundedReceiver;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SwipeDirection {}

impl SwipeDirection {
    pub(crate) const fn is_forward(self) -> bool {
        match self {}
    }
}

pub(crate) struct SwipeWatch {
    pub(crate) swipes: UnboundedReceiver<SwipeDirection>,
}

pub(crate) const fn watch_swipes() -> Option<SwipeWatch> {
    None
}
