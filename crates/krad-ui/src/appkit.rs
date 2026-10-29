use gpui_kit::Window;
use objc2::rc::Retained;
use objc2_app_kit::NSView;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub(crate) fn gpui_view(window: &Window) -> Option<Retained<NSView>> {
    let RawWindowHandle::AppKit(handle) = HasWindowHandle::window_handle(window).ok()?.as_raw()
    else {
        return None;
    };
    // SAFETY: GPUI hands out its window's `NSView`, which lives as long as
    unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<NSView>()) }
}
