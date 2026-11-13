use krad_host::HostLink;

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

    pub(super) const fn is_wsl(self) -> bool {
        matches!(self, Self::Wsl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_site_follows_the_link_before_the_folder() {
        assert_eq!(ShellSite::of(&HostLink::Local, false), ShellSite::Native);
        assert_eq!(ShellSite::of(&HostLink::Local, true), ShellSite::Wsl);
    }
}
