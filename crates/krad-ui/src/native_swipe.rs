#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(target_os = "macos")]
pub(crate) use macos::watch_swipes;
#[cfg(not(target_os = "macos"))]
pub(crate) use other::watch_swipes;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SwipeDirection {
    Left,
    Right,
}
