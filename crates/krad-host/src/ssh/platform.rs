#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub(crate) use unix::*;

#[cfg(not(unix))]
mod windows;
#[cfg(not(unix))]
pub(crate) use windows::*;
