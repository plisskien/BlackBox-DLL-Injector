//! Small shared helpers: UTF-16 strings, error formatting, NTSTATUS pretty-printing.

use crate::win32;
use std::fmt::Write as _;

/// Zero-terminated UTF-16 buffer for wide Win32 APIs.
pub fn wstr(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Non-terminated UTF-16 (e.g. for UNICODE_STRING bodies).
pub fn wbuf(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Lossy read of a zero-terminated UTF-16 buffer into String.
pub unsafe fn from_wsz(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    while *p.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

/// GetLastError() -> readable message.
pub fn last_error(context: &str) -> String {
    let err = unsafe { win32::kernel32::GetLastError() };
    format_win_err(err, context)
}

pub fn format_win_err(err: u32, context: &str) -> String {
    let mut buf = [0u16; 1024];
    let n = unsafe {
        win32::kernel32::FormatMessageW(
            win32::FORMAT_MESSAGE_FROM_SYSTEM | win32::FORMAT_MESSAGE_IGNORE_INSERTS,
            std::ptr::null(),
            err,
            0,
            buf.as_mut_ptr(),
            buf.len() as u32,
            std::ptr::null_mut(),
        )
    } as usize;
    let mut msg = String::from_utf16_lossy(&buf[..n.min(buf.len())]);
    while msg.ends_with('\r') || msg.ends_with('\n') || msg.ends_with(' ') {
        msg.pop();
    }
    if msg.is_empty() {
        let _ = write!(msg, "error {err}");
    }
    format!("{context}: {msg} (0x{err:08X})")
}

/// Translates an NTSTATUS code into a human-readable Win32 error message.
pub fn nt_error(status: i32, context: &str) -> String {
    let dos = unsafe { (win32::ntdll::get().RtlNtStatusToDosError)(status) };
    if dos == 0 {
        format!("{context}: NTSTATUS 0x{:08X}", status as u32)
    } else {
        format_win_err(dos, context)
    }
}

pub fn hiword(l: u32) -> u16 {
    ((l >> 16) & 0xFFFF) as u16
}
pub fn loword(l: u32) -> u16 {
    (l & 0xFFFF) as u16
}
