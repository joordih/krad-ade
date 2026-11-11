use std::env;

const WAYLAND_DISPLAY: &str = "WAYLAND_DISPLAY";
const CURRENT_DESKTOP: &str = "XDG_CURRENT_DESKTOP";
const DESKTOP_SEPARATOR: char = ':';
const BLURRING_DESKTOPS: &[&str] = &["KDE"];

pub(super) fn compositor_blurs() -> bool {
    let wayland = env::var_os(WAYLAND_DISPLAY).is_some();
    let desktop = env::var(CURRENT_DESKTOP).unwrap_or_default();
    let blurs = wayland && desktop_blurs(&desktop);
    tracing::debug!(wayland, blurs, "window blur support");
    blurs
}

fn desktop_blurs(desktop: &str) -> bool {
    desktop.split(DESKTOP_SEPARATOR).any(|name| {
        BLURRING_DESKTOPS
            .iter()
            .any(|known| name.eq_ignore_ascii_case(known))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_compositor_that_blurs_keeps_the_blur() {
        assert!(desktop_blurs("KDE"));
        assert!(desktop_blurs("ubuntu:KDE"));
        assert!(!desktop_blurs("GNOME"));
        assert!(!desktop_blurs(""));
    }
}
