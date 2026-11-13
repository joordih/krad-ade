use krad_domain::Host;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ShellSite {
    Native,
    Wsl,
    Ssh,
}

impl ShellSite {
    pub(super) fn of(host: &Host, in_wsl: bool) -> Self {
        match host {
            Host::Ssh { .. } | Host::Extension { .. } => Self::Ssh,
            Host::Local if in_wsl => Self::Wsl,
            Host::Local => Self::Native,
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
        assert_eq!(ShellSite::of(&Host::Local, false), ShellSite::Native);
        assert_eq!(ShellSite::of(&Host::Local, true), ShellSite::Wsl);
    }
}
