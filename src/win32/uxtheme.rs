//! uxtheme - visual styles + the well-known dark-mode entry points (undocumented
//! ordinals, stable since Windows 10 1809). All calls fail gracefully on older systems.
#![allow(non_snake_case, dead_code)]

use super::*;

#[link(name = "uxtheme")]
extern "system" {
    pub fn SetWindowTheme(hwnd: HWND, pszSubAppName: *const u16, pszSubIdList: *const u16) -> i32;
}

pub const DARKMODE_EXPLORER: &str = "DarkMode_Explorer";
pub const DARKMODE_CFD: &str = "DarkMode_CFD";

/// SetWindowTheme wrapper with UTF-16 class name.
pub fn set_theme(hwnd: HWND, theme: &str) {
    unsafe {
        let t = crate::util::wstr(theme);
        SetWindowTheme(hwnd, t.as_ptr(), std::ptr::null());
    }
}

type AllowDarkModeForWindowFn = unsafe extern "system" fn(HWND, i32) -> i32;
type SetPreferredAppModeFn = unsafe extern "system" fn(i32) -> i32;
type RefreshPolicyFn = unsafe extern "system" fn() -> ();

const ORD_ALLOW_DARK_MODE_FOR_WINDOW: usize = 133;
const ORD_SET_PREFERRED_APP_MODE: usize = 135;
const ORD_REFRESH_IMMERSIVE_COLOR_POLICY: usize = 104;

// PreferredAppMode values
pub const PAM_DEFAULT: i32 = 0;
pub const PAM_ALLOW_DARK: i32 = 1;
pub const PAM_FORCE_DARK: i32 = 2;

fn uxtheme_proc(ordinal: usize) -> usize {
    unsafe {
        let name = crate::util::wstr("uxtheme.dll");
        let base = crate::win32::kernel32::GetModuleHandleW(name.as_ptr());
        if base.is_null() {
            return 0;
        }
        // GetProcAddress with MAKEINTRESOURCE ordinal
        match crate::win32::kernel32::GetProcAddress(base, ordinal as *const u8) {
            Some(f) => f as usize,
            None => 0,
        }
    }
}

/// Opt the process into dark-mode-capable rendering. Call once before window creation.
pub fn enable_app_dark_mode() {
    unsafe {
        let set_mode = uxtheme_proc(ORD_SET_PREFERRED_APP_MODE);
        if set_mode != 0 {
            std::mem::transmute::<usize, SetPreferredAppModeFn>(set_mode)(PAM_ALLOW_DARK);
        }
        let refresh = uxtheme_proc(ORD_REFRESH_IMMERSIVE_COLOR_POLICY);
        if refresh != 0 {
            std::mem::transmute::<usize, RefreshPolicyFn>(refresh)();
        }
    }
}

pub fn allow_dark_mode_for_window(hwnd: HWND) {
    unsafe {
        let f = uxtheme_proc(ORD_ALLOW_DARK_MODE_FOR_WINDOW);
        if f != 0 {
            std::mem::transmute::<usize, AllowDarkModeForWindowFn>(f)(hwnd, 1);
        }
    }
}
