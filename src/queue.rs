//! The injection queue: runs the enabled files in order with their per-file options,
//! validates a queue before touching the target, unloads modules, and provides the small
//! process helpers auto-inject needs (window waiting, child processes, module waiting).

use crate::config::Module;
use crate::diag;
use crate::inject::{self, InjectOptions, RemoteModule};
use crate::process::{self, Arch};
use crate::util::last_error;
use crate::win32::kernel32;
use crate::win32::user32;
use crate::win32::*;

pub type Logger<'a> = &'a dyn Fn(&str);

#[derive(Clone)]
pub struct QueueOptions {
    pub start_delay: u32,
    pub between_delay: u32,
    pub base: InjectOptions,
    /// Extra attempts per file when an injection fails
    pub retries: u32,
}

#[derive(Default)]
pub struct QueueResult {
    pub injected: usize,
    pub skipped: usize,
    pub failed: Vec<(String, String)>,
    pub aborted: bool,
    /// One entry per injected file, with the module/process verification
    pub verifications: Vec<diag::Verification>,
    /// Set when the target died during the run
    pub crash: Option<diag::CrashInfo>,
}

fn crash_info(exit_code: u32, after_ms: u64, target_name: &str, pid: u32) -> diag::CrashInfo {
    let rec = diag::crash_events(target_name, pid, diag::now_unix().saturating_sub(900));
    diag::CrashInfo {
        exit_code,
        after_ms,
        events: rec.lines,
        faulting_module: rec.faulting_module,
        exception_code: rec.exception_code,
        fault_offset: rec.fault_offset,
    }
}

struct HandleGuard(HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = kernel32::CloseHandle(self.0);
        }
    }
}

/// Opens the target for module scanning (PEB read).
fn open_for_scan(pid: u32) -> Result<(HANDLE, Arch), String> {
    unsafe {
        let h = kernel32::OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if h.is_null() {
            return Err(last_error("OpenProcess"));
        }
        let guard = HandleGuard(h);
        let arch = process::process_arch(h)?;
        std::mem::forget(guard); // caller owns the handle
        Ok((h, arch))
    }
}

/// Modules currently loaded in the target, or an error.
pub fn module_snapshot(pid: u32) -> Result<Vec<RemoteModule>, String> {
    let (h, arch) = open_for_scan(pid)?;
    let _g = HandleGuard(h);
    inject::remote_modules(h, arch)
}

/// Base address of a loaded module, or None.
pub fn module_loaded(pid: u32, name: &str) -> Option<usize> {
    module_snapshot(pid)
        .ok()
        .and_then(|mods| inject::find_module(&mods, name).map(|m| m.base))
}

/// Waits until `name` shows up in the target's module list.
pub fn wait_for_module(pid: u32, name: &str, timeout_ms: u32, _log: Logger) -> Result<(), String> {
    let (h, arch) = open_for_scan(pid)?;
    let _g = HandleGuard(h);
    let start = std::time::Instant::now();
    loop {
        if let Ok(mods) = inject::remote_modules(h, arch) {
            if inject::find_module(&mods, name).is_some() {
                return Ok(());
            }
        }
        if start.elapsed().as_millis() as u32 >= timeout_ms {
            return Err(format!("timed out waiting for module '{name}'"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Runs the enabled entries of a queue in order.
pub fn run(pid: u32, modules: &[Module], opts: &QueueOptions, log: Logger) -> QueueResult {
    let mut result = QueueResult::default();
    let scan = open_for_scan(pid);
    // Report the VC++ runtime state for the target bitness (never installs silently).
    if let Ok((_, arch)) = &scan {
        crate::vcredist::ensure_for_arch(*arch, log);
    }
    if let Ok((h, _)) = &scan {
        let mut flags = 0u32;
        let mut permanent = 0i32;
        if unsafe { kernel32::GetProcessDEPPolicy(*h, &mut flags, &mut permanent) } != 0 {
            let state = if flags & 1 != 0 { "enabled" } else { "disabled" };
            let perm = if permanent != 0 { ", permanent" } else { "" };
            log(&format!("[*] Target DEP: {state}{perm}"));
        }
    }
    let watched = diag::Watched::new(pid);
    let target_name = process::enumerate()
        .into_iter()
        .find(|p| p.pid == pid)
        .map(|p| p.name)
        .unwrap_or_default();
    let mut injected_index = 0usize;

    // If the target disappeared before we started, there is nothing useful to do. This is a
    // plain failure, not a crash caused by the payload: the process was already gone.
    if let Some(w) = &watched {
        if !w.alive() {
            let code = w.exit_code().unwrap_or(0);
            log(&format!(
                "[!] The target process has already exited ({}) - nothing was injected",
                diag::exit_text(code)
            ));
            for m in modules.iter().filter(|m| m.enabled) {
                result.failed.push((
                    m.path.clone(),
                    format!("target process already exited before injection ({})", diag::exit_text(code)),
                ));
            }
            result.aborted = true;
            return result;
        }
    }

    for m in modules.iter().filter(|m| m.enabled) {
        // Conditional files: skip instead of failing when their condition is not met
        let required = m.require_module.trim();
        if !required.is_empty() && module_loaded(pid, required).is_none() {
            log(&format!("[*] Skipped {}: missing module '{}'", m.file_name(), required));
            result.skipped += 1;
            continue;
        }
        if m.require_window && !process_has_window(pid) {
            log(&format!("[*] Skipped {}: target has no window", m.file_name()));
            result.skipped += 1;
            continue;
        }

        // Per-file target process: only inject this file when the target name matches
        let wanted_process = m.require_process.trim();
        if !wanted_process.is_empty() {
            let norm = |s: &str| s.trim().trim_end_matches(".exe").to_ascii_lowercase();
            if norm(&target_name) != norm(wanted_process) {
                log(&format!(
                    "[*] Skipped {}: target is not '{}'",
                    m.file_name(),
                    wanted_process
                ));
                result.skipped += 1;
                continue;
            }
        }

        // Wait for a module this file depends on, when one is configured
        let wait_for = m.wait_for_module.trim();
        if !wait_for.is_empty() {
            log(&format!("[*] Waiting for {} before {}...", wait_for, m.file_name()));
            let waited = match &scan {
                Ok((h, arch)) => wait_for_module_on(*h, *arch, wait_for, 60_000),
                Err(_) => wait_for_module(pid, wait_for, 60_000, log).map(|_| ()),
            };
            if let Err(e) = waited {
                result.failed.push((m.path.clone(), e.clone()));
                log(&format!("[!] {e}"));
                if m.ignore_errors {
                    result.skipped += 1;
                    continue;
                }
                result.aborted = true;
                break;
            }
        }

        let wait_ms = m.wait_before(injected_index, opts.start_delay, opts.between_delay);
        if wait_ms > 0 {
            log(&format!("[*] Waiting {} ms before {}...", wait_ms, m.file_name()));
            std::thread::sleep(std::time::Duration::from_millis(wait_ms as u64));
        }

        let mut file_opts = opts.base.clone();
        file_opts.method = m.effective_method(opts.base.method);

        let mut attempt = 0u32;
        loop {
            match inject::inject(pid, &m.path, &file_opts, log) {
                Ok(base) => {
                    let method = file_opts.method;
                    let verification = match &watched {
                        Some(w) => diag::verify_module(w, &m.path, method, base, 1200),
                        None => diag::Verification { path: m.path.clone(), method, base, alive: true, ..Default::default() },
                    };
                    log(&diag::verification_line(&verification));
                    if method == 2 {
                        diag::record_manual_map(pid, &target_name, &m.path, base);
                    }
                    result.injected += 1;
                    injected_index += 1;

                    if !verification.alive {
                        let code = verification.exit_code.unwrap_or(0);
                        result.crash = Some(crash_info(code, verification.elapsed_ms, &target_name, pid));
                        log(&format!("[!] Target crashed after {} ({})", m.file_name(), diag::exit_text(code)));
                        result.verifications.push(verification);
                        result.aborted = true;
                        break;
                    }
                    result.verifications.push(verification);

                    if m.delay_after_ms > 0 {
                        log(&format!("[*] Waiting {} ms after {}...", m.delay_after_ms, m.file_name()));
                        std::thread::sleep(std::time::Duration::from_millis(m.delay_after_ms as u64));
                    }
                    // A short guard window catches DLLs that crash the target from a worker
                    // thread just after DllMain returned
                    if let Some(w) = &watched {
                        if let Some(code) = w.wait_death(400) {
                            result.crash = Some(crash_info(code, 400, &target_name, pid));
                            log(&format!("[!] Target crashed after {} ({})", m.file_name(), diag::exit_text(code)));
                            result.aborted = true;
                        }
                    }
                    break;
                }
                Err(e) => {
                    if attempt < opts.retries {
                        attempt += 1;
                        log(&format!("[*] Retrying {} ({}/{}): {}", m.file_name(), attempt, opts.retries, e));
                        std::thread::sleep(std::time::Duration::from_millis(800));
                        continue;
                    }
                    // A DLL that crashes the target inside DllMain makes the injection call
                    // fail too: check the process before treating this as an ordinary error.
                    // Errors raised before the target was ever touched (handle, arch, file
                    // validation) must not be blamed on the payload: the target may exit on
                    // its own at the same moment, which used to be reported as a crash.
                    let never_reached_target = e.starts_with("OpenProcess(")
                        || e.contains("architecture mismatch")
                        || e.contains("is not a PE file")
                        || e.contains("not found in target")
                        || e.contains("unknown method");
                    if let Some(w) = &watched {
                        if !never_reached_target {
                            if let Some(code) = w.exit_code() {
                                log(&format!(
                                    "[!] Target crashed while injecting {} ({})",
                                    m.file_name(),
                                    diag::exit_text(code)
                                ));
                                result.failed.push((m.path.clone(), e));
                                result.crash = Some(crash_info(code, 0, &target_name, pid));
                                result.aborted = true;
                                break;
                            }
                        }
                    }
                    log(&format!("[!] Inject failed for {}: {}", m.file_name(), e));
                    result.failed.push((m.path.clone(), e));
                    if m.ignore_errors {
                        result.skipped += 1;
                        log("[*] Ignoring failure, continuing queue");
                        break;
                    }
                    result.aborted = true;
                    break;
                }
            }
        }
        if result.aborted {
            break;
        }
    }

    if let Ok((h, _)) = scan {
        let _ = unsafe { kernel32::CloseHandle(h) };
    }
    result
}

fn wait_for_module_on(h: HANDLE, arch: Arch, name: &str, timeout_ms: u32) -> Result<(), String> {
    let start = std::time::Instant::now();
    loop {
        if let Ok(mods) = inject::remote_modules(h, arch) {
            if inject::find_module(&mods, name).is_some() {
                return Ok(());
            }
        }
        if start.elapsed().as_millis() as u32 >= timeout_ms {
            return Err(format!("timed out waiting for module '{name}'"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

// ===== pre-inject validation =====

#[derive(Clone, Debug)]
pub struct Problem {
    pub path: String,
    pub fatal: bool,
    pub message: String,
}

impl Problem {
    pub fn line(&self) -> String {
        let name = std::path::Path::new(&self.path)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(self.path.as_str());
        format!("{}: {}", name, self.message)
    }
}

/// Checks the enabled files before anything is injected. Fatal problems stop the queue;
/// warnings are informational.
pub fn validate(pid: u32, modules: &[Module]) -> Vec<Problem> {
    let mut problems = Vec::new();

    let target = open_for_scan(pid);
    let (arch, remote_mods) = match &target {
        Ok((h, arch)) => {
            let mods = inject::remote_modules(*h, *arch).unwrap_or_default();
            (Some(*arch), mods)
        }
        Err(_) => (None, Vec::new()),
    };

    let redist = crate::vcredist::detect();

    for m in modules.iter().filter(|m| m.enabled) {
        let path = m.path.clone();
        if !std::path::Path::new(&m.path).is_file() {
            problems.push(Problem { path, fatal: true, message: "file does not exist".into() });
            continue;
        }
        let data = match std::fs::read(&m.path) {
            Ok(d) => d,
            Err(e) => {
                problems.push(Problem { path, fatal: true, message: format!("cannot read file: {e}") });
                continue;
            }
        };
        let pe = match crate::pe::parse::PeImage::parse(&data) {
            Ok(pe) => pe,
            Err(e) => {
                problems.push(Problem { path, fatal: true, message: format!("not a valid DLL: {e}") });
                continue;
            }
        };
        if let Err(e) = crate::pe::parse::validate_image(&data, &pe) {
            problems.push(Problem {
                path: path.clone(),
                fatal: true,
                message: format!("corrupt image: {e}"),
            });
            continue;
        }

        match arch {
            Some(Arch::X64) if pe.machine != 0x8664 => problems.push(Problem {
                path: path.clone(),
                fatal: true,
                message: "is not a 64-bit DLL (target is x64)".into(),
            }),
            Some(Arch::X86) if pe.machine != 0x14C => problems.push(Problem {
                path: path.clone(),
                fatal: true,
                message: "is not a 32-bit DLL (target is x86)".into(),
            }),
            _ => {}
        }

        let file_name = m.file_name();
        if inject::find_module(&remote_mods, &file_name).is_some() {
            problems.push(Problem {
                path: path.clone(),
                fatal: true,
                message: "is already loaded in the target".into(),
            });
        }

        // Dependency hints: the VC++ runtime is the usual reason a DLL fails to load
        let imports = crate::pe::parse::import_dlls(&data, &pe);
        let vc_deps: Vec<&str> = imports
            .iter()
            .map(|s| s.as_str())
            .filter(|s| {
                let s = s.to_ascii_lowercase();
                s.starts_with("vcruntime140") || s.starts_with("msvcp140")
            })
            .collect();
        if !vc_deps.is_empty() {
            let installed = match arch {
                Some(Arch::X86) => redist.x86.is_some(),
                _ => redist.x64.is_some(),
            };
            if !installed {
                problems.push(Problem {
                    path: path.clone(),
                    fatal: false,
                    message: format!(
                        "needs {} - the VC++ runtime is not installed (run --check-redist)",
                        vc_deps.join(", ")
                    ),
                });
            }
        }
    }

    if let Ok((h, _)) = target {
        let _ = unsafe { kernel32::CloseHandle(h) };
    }
    problems
}

/// Inspects all enabled modules in the queue. If any module requires the VC++ runtime
/// (e.g. imports vcruntime140 / msvcp140) and that runtime is not installed for the target
/// process architecture, returns the target architecture and the list of missing DLL names.
pub fn check_missing_vcredist(pid: u32, modules: &[Module]) -> Option<(Arch, Vec<String>)> {
    let target = open_for_scan(pid);
    let arch = match &target {
        Ok((_, a)) => *a,
        Err(_) => crate::process::process_arch_quick(pid)?,
    };
    if let Ok((h, _)) = target {
        let _ = unsafe { kernel32::CloseHandle(h) };
    }

    if crate::vcredist::is_installed(arch) {
        return None;
    }

    let mut missing_deps = Vec::new();
    for m in modules.iter().filter(|m| m.enabled) {
        if let Ok(data) = std::fs::read(&m.path) {
            if let Ok(pe) = crate::pe::parse::PeImage::parse(&data) {
                let imports = crate::pe::parse::import_dlls(&data, &pe);
                for imp in imports {
                    let imp_lower = imp.to_ascii_lowercase();
                    if (imp_lower.starts_with("vcruntime140") || imp_lower.starts_with("msvcp140"))
                        && !missing_deps.iter().any(|d: &String| d.eq_ignore_ascii_case(&imp))
                    {
                        missing_deps.push(imp);
                    }
                }
            }
        }
    }

    if !missing_deps.is_empty() {
        Some((arch, missing_deps))
    } else {
        None
    }
}

// ===== unload / eject =====

/// Unloads a module from the target with a remote FreeLibrary. Optionally zeroes the PE
/// headers first. Manual-mapped or hidden modules do not show up in the PEB and cannot be
/// unloaded this way.
pub fn unload(pid: u32, module_name: &str, wipe_headers: bool, log: Logger) -> Result<(), String> {
    let rights = PROCESS_CREATE_THREAD
        | PROCESS_QUERY_INFORMATION
        | PROCESS_VM_OPERATION
        | PROCESS_VM_READ
        | PROCESS_VM_WRITE;
    let h = unsafe { kernel32::OpenProcess(rights, 0, pid) };
    if h.is_null() {
        return Err(last_error("OpenProcess"));
    }
    let _g = HandleGuard(h);

    let arch = process::process_arch(h)?;
    let mods = inject::remote_modules(h, arch)?;
    let target = match inject::find_module(&mods, module_name) {
        Some(t) => (t.base, t.name.clone()),
        None => {
            // Manual-mapped modules are not in the PEB. If this injector mapped it, the log
            // still knows the base address, so at least its headers can be erased.
            let file = std::path::Path::new(module_name)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(module_name);
            if let Some(base) = diag::manual_map_base(pid, file) {
                log(&format!(
                    "[*] {module_name} is not in the PEB (manual map); erasing its PE headers at base {base:#x}"
                ));
                wipe_remote_headers(h, base, log);
                log(&format!(
                    "[!] {module_name} stays mapped in the target: a manual-mapped module cannot be FreeLibrary'd"
                ));
                return Ok(());
            }
            return Err(format!(
                "'{module_name}' is not loaded in the target (manual-mapped modules can only be cleaned up when this injector mapped them in this session)"
            ));
        }
    };
    let base = target.0;
    let name = target.1;

    if wipe_headers {
        wipe_remote_headers(h, base, log);
    }

    let kernel32_mod = inject::find_module(&mods, "kernel32.dll").ok_or("kernel32.dll not found in the target")?;
    let free_library = inject::remote_export(h, arch, kernel32_mod.base, "FreeLibrary", 0)?;
    let thread = inject::create_remote_thread(h, arch, free_library, base, false)?;
    let status = inject::wait_thread_status(thread, 30_000);
    let _ = inject::close_handle(thread);
    let code = match status {
        inject::ThreadWait::Done(c) => c,
        inject::ThreadWait::TimedOut => {
            return Err("FreeLibrary did not return within 30 s (DllMain detach is blocking)".into())
        }
        inject::ThreadWait::Failed(e) => return Err(e),
    };
    if code == 0 {
        return Err("FreeLibrary returned FALSE (the DLL refused to unload in DllMain)".into());
    }
    log(&format!("[+] Unloaded {name} (base {base:#x})"));
    Ok(())
}

fn wipe_remote_headers(h: HANDLE, base: usize, log: Logger) {
    unsafe {
        let dos = match inject::remote_read(h, base, 0x40) {
            Ok(d) => d,
            Err(_) => return,
        };
        if &dos[0..2] != b"MZ" {
            return;
        }
        let e_lfanew = u32::from_le_bytes(dos[0x3C..0x40].try_into().unwrap()) as usize;
        let nt = match inject::remote_read(h, base + e_lfanew, 4 + 20 + 64) {
            Ok(n) => n,
            Err(_) => return,
        };
        if &nt[0..4] != b"PE\0\0" {
            return;
        }
        let opt_off = 4 + 20;
        let size_of_headers = u32::from_le_bytes(nt[opt_off + 60..opt_off + 64].try_into().unwrap()) as usize;
        if size_of_headers == 0 {
            return;
        }
        let len = size_of_headers.min(0x1000);
        let mut old = 0u32;
        if kernel32::VirtualProtectEx(h, base as *mut c_void_raw, len, PAGE_READWRITE, &mut old) != 0 {
            let zeros = vec![0u8; len];
            let _ = inject::remote_write(h, base, &zeros);
            let mut old2 = 0u32;
            let _ = kernel32::VirtualProtectEx(h, base as *mut c_void_raw, len, old, &mut old2);
            log("[*] Erased the PE headers from memory");
        }
    }
}

// ===== auto-inject helpers =====

unsafe extern "system" fn enum_window_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = &mut *(lparam as *mut (u32, bool));
    let mut wpid = 0u32;
    user32::GetWindowThreadProcessId(hwnd, &mut wpid);
    if wpid == state.0 && user32::IsWindowVisible(hwnd) != 0 {
        state.1 = true;
        return 0;
    }
    1
}

/// True when the process owns at least one visible top-level window.
pub fn process_has_window(pid: u32) -> bool {
    let mut state = (pid, false);
    unsafe {
        user32::EnumWindows(Some(enum_window_cb), &mut state as *mut _ as LPARAM);
    }
    state.1
}

unsafe extern "system" fn enum_window_title_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = &mut *(lparam as *mut (u32, Vec<String>));
    let mut wpid = 0u32;
    user32::GetWindowThreadProcessId(hwnd, &mut wpid);
    if wpid == state.0 && user32::IsWindowVisible(hwnd) != 0 {
        let mut buf = [0u16; 512];
        let n = user32::GetWindowTextW(hwnd, buf.as_mut_ptr(), 512);
        if n > 0 {
            state.1.push(String::from_utf16_lossy(&buf[..n as usize]));
        }
    }
    1
}

/// Titles of the process' visible top-level windows.
pub fn window_titles(pid: u32) -> Vec<String> {
    let mut state = (pid, Vec::new());
    unsafe {
        user32::EnumWindows(Some(enum_window_title_cb), &mut state as *mut _ as LPARAM);
    }
    state.1
}

/// Waits for a visible window whose title contains `needle` (case-insensitive).
pub fn wait_for_window_title(pid: u32, needle: &str, timeout_ms: u32, _log: Logger) -> bool {
    let needle = needle.to_lowercase();
    let start = std::time::Instant::now();
    loop {
        if window_titles(pid).iter().any(|t| t.to_lowercase().contains(&needle)) {
            return true;
        }
        if start.elapsed().as_millis() as u32 >= timeout_ms {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

/// Waits for the process to open a window, up to `timeout_ms`. Returns true when one appeared.
pub fn wait_for_window(pid: u32, timeout_ms: u32, _log: Logger) -> bool {
    let start = std::time::Instant::now();
    loop {
        if process_has_window(pid) {
            return true;
        }
        if start.elapsed().as_millis() as u32 >= timeout_ms {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

/// Parent process id of `pid`, when it can be queried.
pub fn parent_pid(pid: u32) -> Option<u32> {
    unsafe {
        let h = kernel32::OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let _g = HandleGuard(h);
        let mut pbi = PROCESS_BASIC_INFORMATION::default();
        let st = (crate::win32::ntdll::get().NtQueryInformationProcess)(
            h,
            ProcessBasicInformation,
            &mut pbi as *mut _ as *mut c_void_raw,
            std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
            std::ptr::null_mut(),
        );
        if st != 0 {
            return None;
        }
        Some(pbi.InheritedFromUniqueProcessId as usize as u32)
    }
}

/// Processes whose parent is `parent`.
pub fn child_pids(parent: u32) -> Vec<u32> {
    process::enumerate()
        .into_iter()
        .filter(|p| p.pid != parent && parent_pid(p.pid) == Some(parent))
        .map(|p| p.pid)
        .collect()
}
