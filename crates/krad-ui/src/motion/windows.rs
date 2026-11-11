use std::ffi::c_void;
use std::ptr;

type Bool = i32;

const SPI_GETCLIENTAREAANIMATION: u32 = 0x1042;
const NO_WININI_UPDATE: u32 = 0;
const UNUSED_PARAM: u32 = 0;
const FALSE: Bool = 0;

#[link(name = "user32")]
extern "system" {
    fn SystemParametersInfoW(action: u32, param: u32, value: *mut c_void, wininit: u32) -> Bool;
}

pub(super) fn reduces_motion() -> bool {
    let mut animates: Bool = 1;
    // SAFETY: `SPI_GETCLIENTAREAANIMATION` writes one `BOOL` through the pointer, which points
    // at `animates`, a live, aligned `BOOL` on this stack frame for the whole call.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            UNUSED_PARAM,
            ptr::from_mut(&mut animates).cast(),
            NO_WININI_UPDATE,
        )
    };
    read != FALSE && animates == FALSE
}
