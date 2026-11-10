#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(target_os = "macos")]
pub(crate) use macos::{show_count, BADGE_LABEL, SIGNAL};
#[cfg(not(target_os = "macos"))]
pub(crate) use other::{show_count, BADGE_LABEL, SIGNAL};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BadgeSignal {
    Count,
    Flash,
}
