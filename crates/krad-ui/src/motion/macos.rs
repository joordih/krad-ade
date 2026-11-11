use objc2_app_kit::NSWorkspace;

pub(super) fn reduces_motion() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}
