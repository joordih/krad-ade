use krad_domain::ProcessRun;
use krad_session::{CellPixels, ImagePixels, TermScreen};
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, PoisonError, RwLock};

use crate::input::Input;

#[derive(Default)]
pub struct TerminalFrame {
    pub screen: TermScreen,
    pub images: HashMap<u32, ShownImage>,
    pub cell: Option<CellPixels>,
}

#[derive(Clone, Debug)]
pub struct ShownImage {
    pub generation: u64,
    pub pixels: Arc<ImagePixels>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TerminalStatus {
    pub run: ProcessRun,
    pub hooked: bool,
    pub bracketed_paste: bool,
    pub mouse_tracking: bool,
}

pub(crate) struct Shared {
    frame: RwLock<Arc<TerminalFrame>>,
    status: RwLock<TerminalStatus>,
    wanted: AtomicBool,
    stale: AtomicBool,
    wake: SyncSender<Input>,
}

impl fmt::Debug for Shared {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Shared").finish_non_exhaustive()
    }
}

impl Shared {
    pub(crate) fn new(wake: SyncSender<Input>) -> Arc<Self> {
        Arc::new(Self {
            frame: RwLock::new(Arc::new(TerminalFrame::default())),
            status: RwLock::new(TerminalStatus::default()),
            wanted: AtomicBool::new(true),
            stale: AtomicBool::new(false),
            wake,
        })
    }

    pub(crate) fn publish(&self, frame: TerminalFrame) {
        *self.frame.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(frame);
        self.stale.store(false, Ordering::Release);
        self.wanted.store(false, Ordering::Release);
    }

    pub(crate) fn set_status(&self, status: TerminalStatus) {
        *self.status.write().unwrap_or_else(PoisonError::into_inner) = status;
    }

    pub(crate) fn mark_stale(&self) {
        self.stale.store(true, Ordering::Release);
    }

    pub(crate) fn is_wanted(&self) -> bool {
        self.wanted.load(Ordering::Acquire)
    }
}

#[derive(Clone)]
pub struct TerminalView {
    shared: Arc<Shared>,
}

impl fmt::Debug for TerminalView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TerminalView")
            .finish_non_exhaustive()
    }
}

impl TerminalView {
    pub(crate) const fn new(shared: Arc<Shared>) -> Self {
        Self { shared }
    }

    #[must_use]
    pub fn paint_frame(&self) -> Arc<TerminalFrame> {
        let was_wanted = self.shared.wanted.swap(true, Ordering::AcqRel);
        if !was_wanted && self.shared.stale.load(Ordering::Acquire) {
            if let Err(error) = self.shared.wake.try_send(Input::Wake) {
                tracing::trace!(%error, "the runtime is busy; it renders on its next batch");
            }
        }
        self.frame()
    }

    #[must_use]
    pub fn frame(&self) -> Arc<TerminalFrame> {
        Arc::clone(
            &self
                .shared
                .frame
                .read()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }

    #[must_use]
    pub fn status(&self) -> TerminalStatus {
        self.shared
            .status
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        self.shared
            .status
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .run
            .is_running()
    }
}
