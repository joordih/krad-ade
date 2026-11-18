use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone, Default)]
pub(crate) struct Stopping(Arc<AtomicBool>);

impl Stopping {
    pub(crate) fn stop(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn stopped(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
