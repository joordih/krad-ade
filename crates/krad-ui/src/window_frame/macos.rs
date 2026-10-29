use gpui_kit::Window;
use objc2::runtime::{AnyClass, NSObjectProtocol};
use objc2::{ClassType, MainThreadMarker};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSGlassEffectView, NSGlassEffectViewStyle, NSWindowOrderingMode,
    NSWorkspace,
};

use crate::appkit::gpui_view;

const GLASS_CLASS: &std::ffi::CStr = c"NSGlassEffectView";

pub(super) fn has_glass() -> bool {
    AnyClass::get(GLASS_CLASS).is_some()
}

pub(super) fn reduces_transparency() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency()
}

pub(super) fn apply_glass(window: &Window, glass: bool) {
    if !has_glass() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(view) = gpui_view(window) else {
        return;
    };
    // SAFETY: a view's superview is a plain AppKit getter, called on the
    let Some(content) = (unsafe { view.superview() }) else {
        return;
    };
    for subview in content.subviews() {
        if subview.isKindOfClass(NSGlassEffectView::class()) {
            subview.removeFromSuperview();
        }
    }
    if !glass {
        return;
    }
    let effect = NSGlassEffectView::initWithFrame(mtm.alloc(), content.bounds());
    effect.setStyle(NSGlassEffectViewStyle::Regular);
    effect.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    content.addSubview_positioned_relativeTo(&effect, NSWindowOrderingMode::Below, Some(&view));
}
