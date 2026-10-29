use std::ffi::c_void;
use std::ptr;

use gpui_kit::{Window, WindowBackgroundAppearance};

type Hwnd = *mut c_void;
type Hkey = isize;
type LParam = isize;
type Bool = i32;

const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
const DWMSBT_NONE: u32 = 1;
const DWMSBT_MAINWINDOW: u32 = 2;
const DWMSBT_TRANSIENTWINDOW: u32 = 3;
const DWMSBT_TABBEDWINDOW: u32 = 4;
const FIRST_SYSTEM_BACKDROP_BUILD: u32 = 22_621;
const CONTINUE: Bool = 1;
const SUCCESS: i32 = 0;
const HKEY_CURRENT_USER: Hkey = 0x8000_0001_u32.cast_signed() as Hkey;
const RRF_RT_REG_DWORD: u32 = 0x0000_0010;
const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const TRANSPARENCY_VALUE: &str = "EnableTransparency";
const FIRST_MICA_BUILD: u32 = 22_000;
const CSD_VERSION_LEN: usize = 128;

#[repr(C)]
struct OsVersionInfo {
    size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform: u32,
    csd_version: [u16; CSD_VERSION_LEN],
}

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

#[link(name = "advapi32")]
extern "system" {
    fn RegGetValueW(
        key: Hkey,
        sub_key: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        data_size: *mut u32,
    ) -> i32;
}

#[link(name = "ntdll")]
extern "system" {
    fn RtlGetVersion(info: *mut OsVersionInfo) -> i32;
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

pub(super) fn transparency_enabled() -> bool {
    let key = wide(PERSONALIZE_KEY);
    let value = wide(TRANSPARENCY_VALUE);
    let mut enabled: u32 = 1;
    let mut size = u32::try_from(std::mem::size_of::<u32>()).unwrap_or(u32::MAX);
    // SAFETY: `key` and `value` are NUL-terminated UTF-16 strings that live
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            ptr::null_mut(),
            ptr::from_mut(&mut enabled).cast(),
            &raw mut size,
        )
    };
    status != SUCCESS || enabled != 0
}

fn windows_build() -> u32 {
    let mut info = OsVersionInfo {
        size: u32::try_from(std::mem::size_of::<OsVersionInfo>()).unwrap_or(u32::MAX),
        major: 0,
        minor: 0,
        build: 0,
        platform: 0,
        csd_version: [0; CSD_VERSION_LEN],
    };
    // SAFETY: `info` is an `OSVERSIONINFOW` with its size set, as the call
    let status = unsafe { RtlGetVersion(&raw mut info) };
    if status == SUCCESS {
        info.build
    } else {
        0
    }
}

pub(super) fn has_mica() -> bool {
    windows_build() >= FIRST_MICA_BUILD
}

unsafe extern "system" fn set_dark_frame(hwnd: Hwnd, dark: LParam) -> Bool {
    let value = Bool::from(dark != 0);
    let size = u32::try_from(std::mem::size_of::<Bool>()).unwrap_or(u32::MAX);
    // SAFETY: `hwnd` comes from `EnumThreadWindows`, so it is a live window of
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            ptr::from_ref(&value).cast(),
            size,
        );
    }
    CONTINUE
}

pub(super) fn match_frame_to(dark: bool) {
    // SAFETY: `GetCurrentThreadId` has no preconditions, and `set_dark_frame`
    unsafe {
        EnumThreadWindows(GetCurrentThreadId(), set_dark_frame, LParam::from(dark));
    }
}

const fn backdrop_type(wanted: WindowBackgroundAppearance) -> u32 {
    match wanted {
        WindowBackgroundAppearance::Blurred => DWMSBT_TRANSIENTWINDOW,
        WindowBackgroundAppearance::MicaBackdrop => DWMSBT_MAINWINDOW,
        WindowBackgroundAppearance::MicaAltBackdrop => DWMSBT_TABBEDWINDOW,
        WindowBackgroundAppearance::Opaque | WindowBackgroundAppearance::Transparent => DWMSBT_NONE,
    }
}

unsafe extern "system" fn set_backdrop(hwnd: Hwnd, backdrop: LParam) -> Bool {
    let backdrop = u32::try_from(backdrop).unwrap_or(DWMSBT_NONE);
    let size = u32::try_from(std::mem::size_of::<u32>()).unwrap_or(u32::MAX);
    // SAFETY: `hwnd` comes from `EnumThreadWindows`, so it is a live window of
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            ptr::from_ref(&backdrop).cast(),
            size,
        );
    }
    CONTINUE
}

fn set_system_backdrop(wanted: WindowBackgroundAppearance) {
    let backdrop = LParam::try_from(backdrop_type(wanted)).unwrap_or(0);
    // SAFETY: `GetCurrentThreadId` has no preconditions, and `set_backdrop`
    unsafe {
        EnumThreadWindows(GetCurrentThreadId(), set_backdrop, backdrop);
    }
}

pub(super) fn apply_background(
    window: &mut Window,
    wanted: WindowBackgroundAppearance,
    dark: bool,
) -> bool {
    if windows_build() < FIRST_SYSTEM_BACKDROP_BUILD {
        return false;
    }
    match_frame_to(dark);
    window.set_background_appearance(WindowBackgroundAppearance::Opaque);
    match wanted {
        WindowBackgroundAppearance::Opaque => {}
        WindowBackgroundAppearance::Transparent => {
            window.set_background_appearance(WindowBackgroundAppearance::Transparent);
        }
        _ => window.set_background_appearance(WindowBackgroundAppearance::MicaBackdrop),
    }
    set_system_backdrop(wanted);
    true
}
