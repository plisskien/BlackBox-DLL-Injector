//! Method 3: Thread Hijacking - execution redirection via thread context.
//!
//! Payload is `LoadLibraryW(path)` executed inside a hijacked thread. Both bitnesses use the
//! same protocol: the stub sets a done flag and then spins, and the injector suspends the
//! thread and restores the saved context once the flag is set. Returning in-thread before
//! the restore (the old x86 behaviour) replays 100-150 ms of application code and crashes
//! busy or syscall-heavy threads.
//!
//! A thread parked in a kernel wait never runs the stub. Candidates are therefore tried in
//! order, and a timeout is only "recovered" when the thread provably never executed the
//! stub (its instruction pointer is still the stub entry); otherwise the thread is left
//! alone and a hard error is reported, because restoring a context that is inside
//! LoadLibrary would abandon the loader lock.

use super::*;
use crate::inject::shellcode;

/// Per-candidate wait for the done flag.
const HIJACK_TIMEOUT_MS: u32 = 5_000;
const POLL_MS: u32 = 40;
const MAX_CANDIDATES: usize = 8;

/// CONTEXT buffers must be 16-byte aligned on x64; these wrappers guarantee it
/// anywhere they are placed on the stack.
#[repr(align(16))]
pub struct AlignedCtx64(pub CONTEXT_X64);
#[repr(align(16))]
pub struct AlignedCtx86(pub WOW64_CONTEXT);

enum SavedCtx {
    X64(AlignedCtx64),
    X86(AlignedCtx86),
}

impl SavedCtx {
    fn rip(&self) -> usize {
        match self {
            SavedCtx::X64(c) => c.0.Rip as usize,
            SavedCtx::X86(c) => c.0.Eip as usize,
        }
    }
    fn set_rip(&mut self, rip: usize) {
        match self {
            SavedCtx::X64(c) => c.0.Rip = rip as u64,
            SavedCtx::X86(c) => c.0.Eip = rip as u32,
        }
    }
}

fn get_context(th: HANDLE, arch: Arch) -> Result<SavedCtx, String> {
    unsafe {
        match arch {
            Arch::X64 => {
                let mut ctx = AlignedCtx64(CONTEXT_X64::default());
                ctx.0.ContextFlags = CONTEXT_CONTROL_AMD64 | CONTEXT_INTEGER_AMD64;
                if kernel32::GetThreadContext(th, &mut ctx.0) == 0 {
                    return Err(crate::util::last_error("GetThreadContext"));
                }
                Ok(SavedCtx::X64(ctx))
            }
            Arch::X86 => {
                let mut ctx = AlignedCtx86(WOW64_CONTEXT::default());
                ctx.0.ContextFlags = CONTEXT_CONTROL_X86 | CONTEXT_INTEGER_X86;
                if kernel32::Wow64GetThreadContext(th, &mut ctx.0) == 0 {
                    return Err(crate::util::last_error("Wow64GetThreadContext"));
                }
                Ok(SavedCtx::X86(ctx))
            }
        }
    }
}

fn set_context(th: HANDLE, arch: Arch, ctx: &SavedCtx) -> Result<(), String> {
    unsafe {
        let ok = match (arch, ctx) {
            (Arch::X64, SavedCtx::X64(c)) => kernel32::SetThreadContext(th, &c.0),
            (Arch::X86, SavedCtx::X86(c)) => kernel32::Wow64SetThreadContext(th, &c.0),
            _ => return Err("context/architecture mismatch".into()),
        };
        if ok == 0 {
            return Err(crate::util::last_error("SetThreadContext"));
        }
        Ok(())
    }
}

pub fn inject(h: HANDLE, arch: Arch, dll_path: &str, opts: &InjectOptions, log: Logger) -> Result<usize, String> {
    let candidates = pick_threads(h, arch)?;
    if candidates.is_empty() {
        return Err("no hijackable thread found in the target process".into());
    }
    log(&format!(
        "[*] {} candidate thread(s) for hijack",
        candidates.len()
    ));
    let mut last = String::from("no candidate accepted the hijack");
    for tid in candidates {
        log(&format!("[*] Trying thread {tid}..."));
        match try_thread(h, arch, tid, dll_path, log) {
            Ok(base) => {
                if opts.effective_hide_thread() {
                    if let Err(e) = stealth::hide_thread_from_debugger(tid) {
                        log(&format!("[!] ThreadHideFromDebugger({tid}): {e}"));
                    }
                }
                return Ok(base);
            }
            Err(e) => {
                log(&format!("[!] Thread {tid}: {e}"));
                last = format!("thread {tid}: {e}");
            }
        }
    }
    Err(format!("thread hijacking failed ({last})"))
}

fn try_thread(
    h: HANDLE,
    arch: Arch,
    tid: u32,
    dll_path: &str,
    log: Logger,
) -> Result<usize, String> {
    unsafe {
        let th = kernel32::OpenThread(
            THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT | THREAD_SET_CONTEXT | THREAD_QUERY_INFORMATION,
            0,
            tid,
        );
        if th.is_null() {
            return Err(crate::util::last_error("OpenThread"));
        }
        let _g = HandleGuard(th);

        let prev = kernel32::SuspendThread(th);
        if prev == 0xFFFF_FFFF {
            return Err(crate::util::last_error("SuspendThread"));
        }
        if prev != 0 {
            // Someone else (or a debugger) already suspended it: leave it exactly as found.
            let _ = kernel32::ResumeThread(th);
            return Err("thread is already suspended".into());
        }

        let mut saved = match get_context(th, arch) {
            Ok(c) => c,
            Err(e) => {
                let _ = kernel32::ResumeThread(th);
                return Err(e);
            }
        };
        let orig_rip = saved.rip();
        if orig_rip == 0 {
            let _ = kernel32::ResumeThread(th);
            return Err("thread has a null instruction pointer".into());
        }

        let mods = remote_modules(h, arch)?;
        let kernel = find_module(&mods, "kernel32.dll").ok_or("kernel32.dll not found in target")?;
        let loadlib = remote_export(h, arch, kernel.base, "LoadLibraryW", 0)?;

        let wpath = crate::util::wbuf(dll_path);
        let path_bytes: Vec<u8> = wpath.iter().flat_map(|c| c.to_le_bytes()).chain([0u8, 0]).collect();

        // Remote layout: [done: 8][path][stub]
        let path_addr_off = 8usize;
        let stub_off = path_addr_off + path_bytes.len();
        let total = stub_off + 256;
        let buf = remote_alloc(h, total, PAGE_EXECUTE_READWRITE)?;
        let done_addr = buf;
        let path_addr = buf + path_addr_off;
        let stub_addr = buf + stub_off;

        let setup = (|| -> Result<(), String> {
            remote_write(h, done_addr, &[0u8; 8])?;
            remote_write(h, path_addr, &path_bytes)?;
            Ok(())
        })();
        if let Err(e) = setup {
            let _ = remote_free(h, buf);
            let _ = kernel32::ResumeThread(th);
            return Err(e);
        }

        let stub_code = match arch {
            Arch::X64 => {
                let built = shellcode::hijack_stub_x64(loadlib, path_addr, orig_rip, done_addr);
                let mut code = built.code;
                shellcode::patch_done_rel32(&mut code, stub_addr, done_addr, built.after_flag_off, built.rel32_at);
                code
            }
            Arch::X86 => {
                let built = shellcode::hijack_stub_x86(loadlib, path_addr);
                let mut code = built.code;
                shellcode::patch_done_abs32(&mut code, done_addr, built.done_at);
                code
            }
        };
        if stub_code.len() > 256 {
            let _ = remote_free(h, buf);
            let _ = kernel32::ResumeThread(th);
            return Err("hijack stub overflow".into());
        }
        if let Err(e) = remote_write(h, stub_addr, &stub_code) {
            let _ = remote_free(h, buf);
            let _ = kernel32::ResumeThread(th);
            return Err(e);
        }

        // Redirect execution.
        saved.set_rip(stub_addr);
        if let Err(e) = set_context(th, arch, &saved) {
            let _ = remote_free(h, buf);
            let _ = kernel32::ResumeThread(th);
            return Err(e);
        }
        if kernel32::ResumeThread(th) == 0xFFFF_FFFF {
            let _ = remote_free(h, buf);
            return Err(crate::util::last_error("ResumeThread"));
        }

        // Wait for the stub to set the done flag.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(HIJACK_TIMEOUT_MS as u64);
        let mut done = false;
        while std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(POLL_MS as u64));
            if remote_read_u32(h, done_addr).unwrap_or(0) == 1 {
                done = true;
                break;
            }
        }

        if done {
            // Restore: suspend, put back the pre-hijack context (with the original RIP) from a
            // fresh copy - `saved` was mutated by the redirect.
            let ok = if kernel32::SuspendThread(th) == 0xFFFF_FFFF {
                log("[!] Failed to re-suspend the hijacked thread for context restore");
                false
            } else {
                let mut restore = match &saved {
                    SavedCtx::X64(c) => SavedCtx::X64(AlignedCtx64(c.0)),
                    SavedCtx::X86(c) => SavedCtx::X86(AlignedCtx86(c.0)),
                };
                restore.set_rip(orig_rip);
                let r = set_context(th, arch, &restore);
                if let Err(e) = &r {
                    log(&format!("[!] SetThreadContext(restore) failed: {e}"));
                }
                let _ = kernel32::ResumeThread(th);
                r.is_ok()
            };
            if !ok {
                log("[!] Stub buffer and TLS-style state cannot be freed safely - leaving the stub mapped");
                return Err("the hijacked thread could not be restored".into());
            }
            // Give the thread a moment to leave the stub region before freeing it.
            std::thread::sleep(std::time::Duration::from_millis(120));
            let _ = remote_free(h, buf);

            let fresh = remote_modules(h, arch)?;
            let name = std::path::Path::new(dll_path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            return find_module(&fresh, &name)
                .map(|m| m.base)
                .ok_or_else(|| format!("LoadLibraryW ran but '{name}' is not in the module list"));
        }

        // Timeout. Was the stub ever entered? Compare the thread's current RIP with the stub
        // entry: if it never ran, restoring is safe; if it is inside the loader, restoring
        // would abandon DllMain and the loader lock.
        if kernel32::SuspendThread(th) == 0xFFFF_FFFF {
            return Err(format!(
                "thread {tid} did not set the done flag within {} s and could not be suspended",
                HIJACK_TIMEOUT_MS / 1000
            ));
        }
        let never_ran = match get_context(th, arch) {            Ok(cur) => {
                let at_entry = cur.rip() == stub_addr;
                if at_entry {
                    // Put it back exactly where it was parked.
                    let _ = set_context(th, arch, &saved_with_rip(&saved, orig_rip));
                }
                at_entry
            }
            Err(_) => false,
        };
        let _ = kernel32::ResumeThread(th);

        if never_ran {
            let _ = remote_free(h, buf);
            Err(
                "thread is parked in a kernel wait and never ran the stub (trying the next candidate)"
                    .into(),
            )
        } else {
            log("[!] The thread is still inside LoadLibrary/DllMain - the loader lock is held.");
            log("[!] Leaving the thread and the stub untouched. Restart the target process.");
            Err(format!(
                "thread {tid} is blocked inside the loader - the target must be restarted"
            ))
        }
    }
}

fn saved_with_rip(ctx: &SavedCtx, rip: usize) -> SavedCtx {
    let mut c = match ctx {
        SavedCtx::X64(c) => SavedCtx::X64(AlignedCtx64(c.0)),
        SavedCtx::X86(c) => SavedCtx::X86(AlignedCtx86(c.0)),
    };
    c.set_rip(rip);
    c
}

/// Candidate thread ids, most likely to run first: threads started in the main executable,
/// then threads started in other loaded modules.
fn pick_threads(h: HANDLE, arch: Arch) -> Result<Vec<u32>, String> {
    unsafe {
        let target_pid = crate::process::pid_of(h)?;
        let snap = kernel32::CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snap == INVALID_HANDLE_VALUE {
            return Err(crate::util::last_error("CreateToolhelp32Snapshot"));
        }
        let _g = super::HandleGuard(snap);

        let mods = remote_modules(h, arch)?;
        let exe = mods.first().map(|m| (m.base, m.size)).unwrap_or((0, 0));

        let mut primary: Vec<u32> = Vec::new();
        let mut secondary: Vec<u32> = Vec::new();
        let mut te = THREADENTRY32::default();
        te.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        if kernel32::Thread32First(snap, &mut te) != 0 {
            loop {
                if te.th32OwnerProcessID == target_pid {
                    let tid = te.th32ThreadID;
                    if let Some(start) = thread_start_address(tid) {
                        if exe.1 != 0 && start >= exe.0 && start < exe.0 + exe.1 as usize {
                            primary.push(tid);
                        } else if mods.iter().any(|m| start >= m.base && start < m.base + m.size as usize) {
                            secondary.push(tid);
                        }
                    }
                }
                if kernel32::Thread32Next(snap, &mut te) == 0 {
                    break;
                }
            }
        }
        primary.extend(secondary);
        primary.truncate(MAX_CANDIDATES);
        Ok(primary)
    }
}

fn thread_start_address(tid: u32) -> Option<usize> {
    unsafe {
        let th = kernel32::OpenThread(THREAD_QUERY_INFORMATION, 0, tid);
        if th.is_null() {
            return None;
        }
        let _g = super::HandleGuard(th);
        let mut start: usize = 0;
        let st = (ntdll::get().NtQueryInformationThread)(
            th,
            ThreadQuerySetWin32StartAddress,
            &mut start as *mut usize as _,
            std::mem::size_of::<usize>() as u32,
            std::ptr::null_mut(),
        );
        if st != 0 {
            return None;
        }
        Some(start)
    }
}

