//! Method 4: NtCreateThreadEx - native remote thread creation.
//! Same payload as Standard but through the NT API, optionally with the
//! THREAD_CREATE_FLAGS_HIDE_FROM_DEBUGGER create flag.

use super::standard::load_library_capture;
use super::*;

pub fn inject(h: HANDLE, arch: Arch, dll_path: &str, opts: &InjectOptions, log: Logger) -> Result<usize, String> {
    let hide = opts.effective_hide_thread();
    load_library_capture(h, arch, dll_path, log, |start| {
        nt_create_thread_strict(h, start, 0, hide)
    })
}

/// NtCreateThreadEx without fallback - this method exists to use the NT API itself.
fn nt_create_thread_strict(h: HANDLE, start: usize, param: usize, hide: bool) -> Result<HANDLE, String> {
    unsafe {
        let mut th: HANDLE = std::ptr::null_mut();
        let flags = if hide { THREAD_CREATE_FLAGS_HIDE_FROM_DEBUGGER } else { 0 };
        let st = (ntdll::get().NtCreateThreadEx)(
            &mut th,
            THREAD_ALL_ACCESS,
            std::ptr::null_mut(),
            h,
            start as _,
            param as _,
            flags,
            0,
            0,
            0,
            std::ptr::null_mut(),
        );
        if st != 0 {
            return Err(nt_error(st, "NtCreateThreadEx"));
        }
        Ok(th)
    }
}
