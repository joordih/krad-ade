use futures::channel::mpsc::UnboundedReceiver;

use super::SwipeDirection;

pub(crate) struct SwipeWatch {
    pub(crate) swipes: UnboundedReceiver<SwipeDirection>,
}

pub(crate) const fn watch_swipes() -> Option<SwipeWatch> {
    None
}
