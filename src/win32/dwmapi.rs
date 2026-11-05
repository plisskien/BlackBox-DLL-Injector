//! dwmapi - dark title bar (DWMWA_USE_IMMERSIVE_DARK_MODE).
#![allow(non_snake_case, dead_code)]

use super::*;

pub const DWMWA_USE_IMMERSIVE_DARK_MODE_NEW: u32 = 20;
pub const DWMWA_USE_IMMERSIVE_DARK_MODE_OLD: u32 = 19;

#[link(name = "dwmapi")]
extern "system" {
    pub fn DwmSetWindowAttribute(hwnd: HWND, attr: u32, value: *const c_void_raw, size: u32) -> i32;
}

/// Force a dark title bar; tries the modern attribute then the pre-20H1 one.
pub fn dark_title_bar(hwnd: HWND) {
    unsafe {
        let on: i32 = 1;
        if DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE_NEW,
            &on as *const i32 as _,
            4,
        ) != 0
        {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE_OLD,
                &on as *const i32 as _,
                4,
            );
        }
    }
}
