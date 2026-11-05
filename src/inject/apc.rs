//! Method 5: APC injection (threadless).
//!
//! No new thread is created: `LoadLibraryW` is queued as a user-mode APC on every thread of
//! the target. As soon as any of those threads enters an alertable wait (GUI message loops,
//! `SleepEx`, `WaitForSingleObjectEx`, ...) the loader runs and the DLL is mapped. Targets
//! without an alertable thread keep the APC queued and this method reports that honestly.

use super::*;

pub fn inject(
    pid: u32,
    h: HANDLE,
    arch: Arch,
    dll_path: &str,
    _opts: &InjectOptions,
    log: Logger,
) -> Result<usize, String> {
    // Cross-bitness APC is not viable: queueing a 32-bit routine on a WOW64 thread from a
    // 64-bit process faults inside the 32-bit APC dispatcher (verified), so 32-bit targets
    // get a clean error instead of a crashing target.
    if arch == Arch::X86 {
        return Err("APC (method 5) is not supported for 32-bit (WOW64) targets - use Standard, LdrLoadDll, Manual Map, Thread Hijacking or NtCreateThreadEx".into());
    }
    let mods = remote_modules(h, arch)?;
    let kernel = find_module(&mods, "kernel32.dll").ok_or("kernel32.dll not found in target")?;
    let loadlib = remote_export(h, arch, kernel.base, "LoadLibraryW", 0)?;
    log(&format!("[*] Remote kernel32!LoadLibraryW @ {:#x}", loadlib));

    let path = wstr(dll_path);
    let path_addr = remote_alloc(h, path.len() * 2, PAGE_READWRITE)?;
    remote_write(h, path_addr, &path_as_bytes(&path))?;

    let (queued, total) = queue_to_threads(pid, loadlib, path_addr);
    log(&format!(
        "[*] Queued LoadLibraryW as an APC on {} of {} thread(s)",
        queued, total
    ));
    if queued == 0 {
        return Err("no thread accepted the APC (access denied, or the process has no threads)".into());
    }

    // The APC runs when any of those threads becomes alertable. Poll for the module, since an
    // APC gives us no return value to wait on.
    let name = std::path::Path::new(dll_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    loop {
        if let Ok(fresh) = remote_modules(h, arch) {
            if find_module(&fresh, &name).is_some() {
                // The loader registers the module before running DllMain, so a module that is
                // found may still be rejected moments later (DllMain returning FALSE). Give
                // the loader time to finish and verify the module is really still there.
                std::thread::sleep(std::time::Duration::from_millis(250));
                let still = remote_modules(h, arch)
                    .ok()
                    .and_then(|f| find_module(&f, &name).map(|x| x.base));
                return match still {
                    Some(base) => Ok(base),
                    None => Err(
                        "the APC ran, but the loader rejected the module (DllMain returned FALSE)"
                            .into(),
                    ),
                };
            }
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(80));
    }

    Err("the APC was queued but no thread became alertable within 6 s (LoadLibraryW did not run)".into())
}

/// Queues the APC on every thread of `pid`. Returns (queued, total threads seen).
fn queue_to_threads(pid: u32, func: usize, data: usize) -> (usize, usize) {
    unsafe {
        let snap = kernel32::CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snap == INVALID_HANDLE_VALUE {
            return (0, 0);
        }
        let mut te = THREADENTRY32::default();
        te.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        let mut queued = 0usize;
        let mut total = 0usize;
        if kernel32::Thread32First(snap, &mut te) != 0 {
            loop {
                if te.th32OwnerProcessID == pid {
                    total += 1;
                    let th = kernel32::OpenThread(THREAD_SET_CONTEXT, 0, te.th32ThreadID);
                    if !th.is_null() {
                        if kernel32::QueueUserAPC(func, th, data) != 0 {
                            queued += 1;
                        }
                        let _ = kernel32::CloseHandle(th);
                    }
                }
                if kernel32::Thread32Next(snap, &mut te) == 0 {
                    break;
                }
            }
        }
        let _ = kernel32::CloseHandle(snap);
        (queued, total)
    }
}
