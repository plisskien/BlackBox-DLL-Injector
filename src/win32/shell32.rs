//! shell32 (drag & drop) + comctl32 (common controls) declarations.
#![allow(non_snake_case, dead_code)]

use super::*;

#[link(name = "shell32")]
extern "system" {
    pub fn DragQueryFileW(drop: HANDLE, index: u32, dst: *mut u16, cb: u32) -> u32;
    pub fn DragFinish(drop: HANDLE);
    pub fn ShellExecuteW(
        hwnd: HWND,
        op: *const u16,
        file: *const u16,
        params: *const u16,
        dir: *const u16,
        show: i32,
    ) -> HINSTANCE;
}

/// SW_SHOWNORMAL
pub const SW_SHOWNORMAL: i32 = 1;

// ===== Common controls =====
pub const ICC_WIN95_CLASSES: u32 = 0x0000_00FF;
pub const ICC_STANDARD_CLASSES: u32 = 0x0000_4000;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct INITCOMMONCONTROLSEX {
    pub dwSize: u32,
    pub dwICC: u32,
}

#[link(name = "comctl32")]
extern "system" {
    pub fn InitCommonControlsEx(picce: *const INITCOMMONCONTROLSEX);
}

// Class name atoms for CreateWindowExW (wide strings supplied by caller).
pub const WC_COMBOBOX: &str = "ComboBox";
pub const WC_EDIT: &str = "Edit";
pub const WC_BUTTON: &str = "Button";
pub const WC_STATIC: &str = "Static";
pub const WC_LISTVIEW: &str = "SysListView32";
pub const WC_UPDOWN: &str = "msctls_updown32";
