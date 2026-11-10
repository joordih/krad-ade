use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;
use objc2_foundation::NSString;

use super::BadgeSignal;

pub(crate) const SIGNAL: BadgeSignal = BadgeSignal::Count;
pub(crate) const BADGE_LABEL: &str = "Dock badge";

pub(crate) fn show_count(count: usize) {
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let label = (count > 0).then(|| NSString::from_str(&count.to_string()));
    NSApplication::sharedApplication(main_thread)
        .dockTile()
        .setBadgeLabel(label.as_deref());
}
