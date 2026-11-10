#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(target_os = "macos")]
pub(crate) use macos::{play, MISSING_NOTE};
#[cfg(not(target_os = "macos"))]
pub(crate) use other::{play, MISSING_NOTE};
