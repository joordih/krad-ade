use portable_pty::{Child, ChildKiller, MasterPty, PtySize};
use std::sync::{Arc, Mutex, PoisonError};

use crate::{GridSize, PtyError};

pub trait PtyResize: Send {
    /// # Errors
    ///
    /// Returns [`PtyError`] when the terminal cannot take the new size.
    fn resize(&self, size: GridSize) -> Result<(), PtyError>;
}

pub trait PtyWait: Send {
    fn wait(&mut self) -> Option<i32>;
}

pub trait PtyKill: Send {
    fn kill(&mut self);
}

impl PtyResize for Box<dyn MasterPty + Send> {
    fn resize(&self, size: GridSize) -> Result<(), PtyError> {
        resize_master(self.as_ref(), size)
    }
}

impl PtyWait for Box<dyn Child + Send + Sync> {
    fn wait(&mut self) -> Option<i32> {
        Child::wait(self.as_mut())
            .ok()
            .and_then(|status| i32::try_from(status.exit_code()).ok())
    }
}

impl PtyKill for Box<dyn ChildKiller + Send + Sync> {
    fn kill(&mut self) {
        drop(ChildKiller::kill(self.as_mut()));
    }
}

/// # Errors
///
/// Returns [`PtyError`] when the pseudo-terminal refuses the new size.
pub fn resize_master(master: &(dyn MasterPty + Send), size: GridSize) -> Result<(), PtyError> {
    let (pixel_width, pixel_height) = size.pixels();
    master
        .resize(PtySize {
            rows: size.rows,
            cols: size.cols,
            pixel_width,
            pixel_height,
        })
        .map_err(PtyError::platform)
}

#[derive(Clone)]
pub struct SharedMaster {
    inner: Arc<Mutex<Option<Box<dyn PtyResize>>>>,
}

impl SharedMaster {
    #[must_use]
    pub fn new(master: Box<dyn PtyResize>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(master))),
        }
    }

    /// # Errors
    ///
    /// Returns [`PtyError`] when the terminal cannot take the new size.
    pub fn resize(&self, size: GridSize) -> Result<(), PtyError> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map_or(Ok(()), |master| master.resize(size))
    }

    pub fn close(&self) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
}

pub struct SharedKiller {
    inner: Mutex<Box<dyn PtyKill>>,
}

impl SharedKiller {
    #[must_use]
    pub fn new(killer: Box<dyn PtyKill>) -> Self {
        Self {
            inner: Mutex::new(killer),
        }
    }

    pub fn kill(&self) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .kill();
    }
}
