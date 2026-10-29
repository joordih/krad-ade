use std::ffi::c_void;

type Hwnd = *mut c_void;
type LParam = isize;
type Bool = i32;

const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;
const CONTINUE: Bool = 1;
const NO_CONTEXT: LParam = 0;

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(hwnd: Hwnd, attribute: u32, value: *const c_void, size: u32) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn EnumThreadWindows(
        thread: u32,
        callback: unsafe extern "system" fn(Hwnd, LParam) -> Bool,
        context: LParam,
    ) -> Bool;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThreadId() -> u32;
}

unsafe extern "system" fn round_window(hwnd: Hwnd, _: LParam) -> Bool {
    let preference = DWMWCP_ROUND;
    let size = u32::try_from(std::mem::size_of::<u32>()).unwrap_or(u32::MAX);
    // SAFETY: `hwnd` comes from `EnumThreadWindows`, so it is a live window
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::from_ref(&preference).cast(),
            size,
        );
    }
    CONTINUE
}

pub fn round_window_corners() {
    // SAFETY: `GetCurrentThreadId` has no preconditions, and `round_window`
    unsafe {
        EnumThreadWindows(GetCurrentThreadId(), round_window, NO_CONTEXT);
    }
}
