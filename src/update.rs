//! Minimal update check: fetch a plain version.txt from GitHub and compare semver.
//! Any network/URL error is silent - the app behaves as if no update exists.

use crate::win32::wininet::*;
use crate::win32::user32::PostMessageW;
use crate::win32::HWND;

/// Change these three to the real project locations.
pub const GITHUB_URL: &str = "https://github.com/plisskien/BlackBox-DLL-Injector";
pub const VERSION_URL: &str =
    "https://raw.githubusercontent.com/plisskien/BlackBox-DLL-Injector/main/version.txt";
#[allow(dead_code)]
pub const FORUM_URL: &str = "https://hackvshack.net";
pub const THREAD_URL: &str =
    "https://hackvshack.net/threads/blackbox-advanced-windows-dll-injector-6-injection-methods-crash-triage-open-source.14818/";
pub const DEV_URL: &str = "https://hackvshack.net/members/plisskien.1/";

pub const WM_APP_UPDATE: u32 = 0x8000 + 8;

static FOUND: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let core = text.trim().trim_start_matches(['v', 'V']).split(['-', '+', ' ']).next()?;
    let mut it = core.split('.');
    let a = it.next()?.parse().ok()?;
    let b = it.next().unwrap_or("0").parse().ok()?;
    let c = it.next().unwrap_or("0").parse().ok()?;
    Some((a, b, c))
}

fn fetch(url: &str) -> Option<String> {
    unsafe {
        let agent = crate::util::wstr("BlackBox-Injector-UpdateCheck");
        let session = InternetOpenW(agent.as_ptr(), 0, std::ptr::null(), std::ptr::null(), 0);
        if session.is_null() {
            return None;
        }
        let wurl = crate::util::wstr(url);
        // INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE
        let handle = InternetOpenUrlW(session, wurl.as_ptr(), std::ptr::null(), 0, 0x8000_0000 | 0x0200_0000, 0);
        if handle.is_null() {
            let _ = InternetCloseHandle(session);
            return None;
        }
        let mut out = Vec::new();
        let mut buf = [0u8; 1024];
        loop {
            let mut read = 0u32;
            if InternetReadFile(handle, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut read) == 0 || read == 0 {
                break;
            }
            out.extend_from_slice(&buf[..read as usize]);
            if out.len() > 8192 {
                break;
            }
        }
        let _ = InternetCloseHandle(handle);
        let _ = InternetCloseHandle(session);
        String::from_utf8(out).ok()
    }
}

pub fn get_found() -> Option<String> {
    FOUND.lock().ok().and_then(|g| g.clone())
}

pub fn take_found() -> Option<String> {
    FOUND.lock().ok().and_then(|g| g.clone())
}

pub fn check_async(hwnd: HWND) {
    let hwnd_usize = hwnd as usize;
    std::thread::spawn(move || {
        let url = std::env::var("BLACKBOX_UPDATE_URL").unwrap_or_else(|_| VERSION_URL.to_string());
        let Some(body) = fetch(&url) else { return };
        let Some(line) = body.lines().map(str::trim).find(|l| !l.is_empty()) else { return };
        let Some(remote) = parse_version(line) else { return };
        let current = parse_version(crate::version::VERSION).unwrap_or((0, 0, 0));
        if remote > current {
            if let Ok(mut g) = FOUND.lock() {
                *g = Some(line.trim_start_matches(['v', 'V']).to_string());
            }
            unsafe {
                let _ = PostMessageW(hwnd_usize as HWND, WM_APP_UPDATE, 0, 0);
            }
        }
    });
}

