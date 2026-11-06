use krad_domain::ShellQuoting;
use krad_host::HostLink;
use krad_session::{clear_screen_prefix, SHELL_QUOTING};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ShellSite {
    Native,
    Wsl,
    Ssh,
}

impl ShellSite {
    pub(super) const fn of(link: &HostLink, in_wsl: bool) -> Self {
        match link {
            HostLink::Remote(_) => Self::Ssh,
            HostLink::Local if in_wsl => Self::Wsl,
            HostLink::Local => Self::Native,
        }
    }

    pub(super) const fn quoting(self) -> ShellQuoting {
        match self {
            Self::Native => SHELL_QUOTING,
            Self::Wsl | Self::Ssh => ShellQuoting::Posix,
        }
    }

    pub(super) const fn clear_prefix(self) -> &'static str {
        clear_screen_prefix(!matches!(self, Self::Native))
    }

    pub(super) const fn is_wsl(self) -> bool {
        matches!(self, Self::Wsl)
    }

    pub(super) const fn reaches_bus(self) -> bool {
        !matches!(self, Self::Ssh)
    }

    pub(super) const fn reads_local_files(self) -> bool {
        matches!(self, Self::Native)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shells_off_this_computer_quote_for_posix() {
        assert_eq!(ShellSite::Wsl.quoting(), ShellQuoting::Posix);
        assert_eq!(ShellSite::Ssh.quoting(), ShellQuoting::Posix);
        assert_eq!(ShellSite::Native.quoting(), SHELL_QUOTING);
    }

    #[test]
    fn only_remote_shells_lose_the_local_bus_and_files() {
        assert!(!ShellSite::Ssh.reaches_bus());
        assert!(ShellSite::Wsl.reaches_bus());
        assert!(ShellSite::Native.reads_local_files());
        assert!(!ShellSite::Wsl.reads_local_files());
        assert!(!ShellSite::Ssh.reads_local_files());
    }

    #[test]
    fn the_site_follows_the_link_before_the_folder() {
        assert_eq!(ShellSite::of(&HostLink::Local, false), ShellSite::Native);
        assert_eq!(ShellSite::of(&HostLink::Local, true), ShellSite::Wsl);
    }
}
