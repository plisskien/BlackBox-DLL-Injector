//! Post-inject verification, crash diagnostics and DLL dependency reports.
//!
//! Provides comprehensive post-injection diagnostics: verifying loaded modules in the PEB,
//! monitoring target process lifecycle, capturing exit codes, and inspecting matching
//! Windows Application-log crash records (faulting module, offset, exception code).

use crate::process::Arch;
use crate::queue;
use crate::win32::advapi32::*;
use crate::win32::kernel32;
use crate::win32::ntdll;
use crate::win32::*;

/// A handle that stays valid across the target's death, so the exit code can still be read.
pub struct Watched {
    handle: HANDLE,
    pid: u32,
}

impl Watched {
    pub fn new(pid: u32) -> Option<Watched> {
        unsafe {
            let h = kernel32::OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid);
            if h.is_null() {
                None
            } else {
                Some(Watched { handle: h, pid })
            }
        }
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn alive(&self) -> bool {
        unsafe { kernel32::WaitForSingleObject(self.handle, 0) == WAIT_TIMEOUT }
    }

    pub fn exit_code(&self) -> Option<u32> {
        unsafe {
            let mut code = 0u32;
            if kernel32::GetExitCodeProcess(self.handle, &mut code) == 0 {
                return None;
            }
            if code == STILL_ACTIVE {
                None
            } else {
                Some(code)
            }
        }
    }

    /// Waits up to `ms` for the process to exit; returns its exit code if it did.
    pub fn wait_death(&self, ms: u32) -> Option<u32> {
        if self.alive() {
            let r = unsafe { kernel32::WaitForSingleObject(self.handle, ms) };
            if r != WAIT_OBJECT_0 {
                return None;
            }
        }
        self.exit_code()
    }
}

impl Drop for Watched {
    fn drop(&mut self) {
        unsafe {
            let _ = kernel32::CloseHandle(self.handle);
        }
    }
}

/// Human-readable name for common process exit codes.
pub fn exit_text(code: u32) -> String {
    let label = match code {
        0xC000_0005 => "ACCESS_VIOLATION",
        0xC000_0006 => "IN_PAGE_ERROR",
        0x8000_0002 => "DATATYPE_MISALIGNMENT",
        0x8000_0003 => "BREAKPOINT",
        0xC000_001D => "ILLEGAL_INSTRUCTION",
        0xC000_008C => "ARRAY_BOUNDS_EXCEEDED",
        0xC000_0094 => "INTEGER_DIVIDE_BY_ZERO",
        0xC000_0096 => "PRIVILEGED_INSTRUCTION",
        0xC000_00FD => "STACK_OVERFLOW",
        0xC000_0135 => "DLL_NOT_FOUND",
        0xC000_0138 => "ORDINAL_NOT_FOUND",
        0xC000_0139 => "ENTRYPOINT_NOT_FOUND",
        0xC000_0142 => "DLL_INIT_FAILED",
        0xC000_0374 => "HEAP_CORRUPTION",
        0xC000_0409 => "STACK_BUFFER_OVERRUN / fail-fast",
        0xC000_0417 => "INVALID_CRUNTIME_PARAMETER",
        0xE043_4352 => ".NET unhandled exception",
        0xC000_007B => "INVALID_IMAGE_FORMAT",
        _ => "",
    };
    if label.is_empty() {
        format!("0x{code:08X}")
    } else {
        format!("0x{code:08X} {label}")
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct Verification {
    pub path: String,
    pub method: u32,
    pub base: usize,
    pub listed: bool,
    pub module_base: usize,
    pub module_size: u32,
    pub alive: bool,
    pub exit_code: Option<u32>,
    pub elapsed_ms: u64,
    pub note: String,
}

/// Waits (briefly) for the module to show up in the target's PEB, then records whether the
/// target is still alive. `base` is the injection result.
pub fn verify_module(watched: &Watched, path: &str, method: u32, base: usize, wait_ms: u32) -> Verification {
    let file = file_name(path);
    let start = std::time::Instant::now();
    let mut found: Option<(usize, u32)> = None;
    loop {
        if let Ok(mods) = queue::module_snapshot(watched.pid()) {
            if let Some(m) = crate::inject::find_module(&mods, &file) {
                found = Some((m.base, m.size));
                break;
            }
        }
        if !watched.alive() {
            break;
        }
        if start.elapsed().as_millis() as u32 >= wait_ms {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
    }

    let alive = watched.alive();
    let exit_code = if alive { None } else { watched.exit_code() };
    let note = if found.is_some() {
        String::new()
    } else if method == 2 {
        "not listed in the PEB (manual map / hidden module)".to_string()
    } else if !alive {
        "the target exited before the module could be seen".to_string()
    } else {
        "not present in the target's PEB module list".to_string()
    };

    Verification {
        path: path.to_string(),
        method,
        base,
        listed: found.is_some(),
        module_base: found.map(|v| v.0).unwrap_or(0),
        module_size: found.map(|v| v.1).unwrap_or(0),
        alive,
        exit_code,
        elapsed_ms: start.elapsed().as_millis() as u64,
        note,
    }
}

#[derive(Clone, Debug, Default)]
pub struct CrashInfo {
    pub exit_code: u32,
    pub after_ms: u64,
    pub events: Vec<String>,
    pub faulting_module: Option<String>,
    pub exception_code: Option<String>,
    pub fault_offset: Option<String>,
}

/// Structured form of the Application-log records that describe a crash.
#[derive(Default, Clone)]
pub struct CrashRecords {
    pub lines: Vec<String>,
    pub faulting_module: Option<String>,
    pub exception_code: Option<String>,
    pub fault_offset: Option<String>,
}

/// Heuristic diagnostic hints indicating failure causes.
/// `injected` holds (path, effective method) for each module in the queue.
pub fn diagnose(crash: &CrashInfo, target: &str, injected: &[(String, u32)]) -> Vec<String> {
    let mut hints = Vec::new();
    let code = crash.exception_code.as_deref().unwrap_or("").to_uppercase();
    let module = crash
        .faulting_module
        .as_deref()
        .map(file_name)
        .map(|m| m.to_lowercase());
    let target_name = file_name(target).to_lowercase();
    let injected_names: Vec<(String, u32)> = injected
        .iter()
        .map(|(p, method)| (file_name(p).to_lowercase(), *method))
        .collect();

    if code.contains("C0000005") {
        hints.push("The exception is an access violation (0xC0000005): code wrote to or read from an invalid address.".to_string());
        if let Some(offset) = &crash.fault_offset {
            let clean = offset.trim().trim_start_matches("0x").trim_start_matches("0X");
            if let Ok(val) = u64::from_str_radix(clean, 16) {
                if val < 0x1000 {
                    hints.push(format!(
                        "Fault offset (+0x{val:X}) is within the NULL page (0x0 - 0x1000), indicating a NULL pointer dereference or calling an uninitialized function/vtable pointer."
                    ));
                }
            }
        }
    }
    if code.contains("C0000409") {
        hints.push(
            "Fail-fast / stack-buffer-overrun (0xC0000409) indicates an integrity check failure (CFG / guard) or corrupted stack."
                .to_string(),
        );
        hints.push(
            "Common causes: Control Flow Guard (CFG) rejecting an indirect call target, corrupted security cookie (__security_check_cookie), or an explicit __fastfail() call."
                .to_string(),
        );
    }
    if code.contains("C00000FD") {
        hints.push(
            "Stack overflow (0xC00000FD): thread call stack exhausted. Frequently caused by infinite recursion or excessively large stack buffers in hooks."
                .to_string(),
        );
    }
    if code.contains("C0000142") {
        hints.push(
            "DLL initialization failed (0xC0000142): DllMain returned FALSE, or an unhandled exception occurred during TLS callbacks or static CRT initialization."
                .to_string(),
        );
    }
    if code.contains("C0000374") {
        hints.push(
            "Heap corruption (0xC0000374): Windows Heap Manager detected metadata corruption (e.g. double-free, heap buffer overrun, or cross-CRT allocator mismatch)."
                .to_string(),
        );
    }
    if code.contains("E0434352") {
        hints.push(
            "Unhandled .NET CLR exception (0xE0434352): managed runtime threw an unhandled exception during execution."
                .to_string(),
        );
    }
    if code.contains("80000003") {
        hints.push("A breakpoint exception (0x80000003) indicates an active debugger check or assertion failure.".to_string());
    }
    if code.contains("C0000135") || code.contains("C0000139") || code.contains("C000007B") {
        hints.push(
            "The loader failed on a missing DLL/export or an image-format mismatch: module dependencies or architecture do not match the target."
                .to_string(),
        );
    }

    if let Some(m) = &module {
        if let Some((_, method)) = injected_names.iter().find(|(n, _)| n == m) {
            hints.push(format!(
                "The faulting module is one of the injected DLLs ({m}): crash occurred inside the payload, likely due to an invalid offset, missing dependency, or build version mismatch."
            ));
            if let Some(offset) = &crash.fault_offset {
                hints.push(format!(
                    "Disassembly triage: Fault offset {offset} is the Relative Virtual Address (RVA) in {m}. In IDA Pro, Ghidra, or x64dbg, navigate to (ImageBase + {offset}) to locate the crashing instruction."
                ));
            }
            match method {
                2 => hints.push(
                    "It was injected with Manual Map. Try Standard, LdrLoadDll or NtCreateThreadEx to rule out mapping artifacts (e.g. missing relocations, TLS callbacks, or exception handler registration)."
                        .to_string(),
                ),
                5 => hints.push(
                    "It was injected with APC (threadless). If the module expects immediate startup execution, try Standard or NtCreateThreadEx."
                        .to_string(),
                ),
                _ => hints.push(
                    "It used a loader-based method; verify the module's target-version compatibility."
                        .to_string(),
                ),
            }
        } else if m == &target_name {
            hints.push(
                "The faulting module is the target executable. An invalid memory patch or hook write is the typical cause; if the crash is immediate, test an alternative injection method."
                    .to_string(),
            );
            if let Some(offset) = &crash.fault_offset {
                hints.push(format!(
                    "Disassembly triage: Crash occurred at target offset {offset} (RVA from target base). Inspect hook installations or patched opcodes near that offset."
                ));
            }
        } else if m == "ntdll.dll" || m == "kernelbase.dll" || m == "kernel32.dll" || m == "win32u.dll" {
            hints.push(format!(
                "The fault is in system module {m}; this typically indicates an OS integrity check (CFG) or invalid system call parameters."
            ));
        } else {
            hints.push(format!(
                "The faulting module is {m} (not the target and not an injected file); verify whether the payload references or hooks that module."
            ));
        }
    } else {
        hints.push(
            "No Windows Error Reporting record with a faulting module was found for this crash; the exit code and run context above are the available evidence."
                .to_string(),
        );
    }

    // Clean exit codes are not faults: distinguishing them avoids blaming the injector for a
    // payload guard (or an anti-cheat / launcher) calling ExitProcess.
    let clean_exit = crash.exit_code < 0xC000_0000 && crash.exception_code.is_none();
    if clean_exit {
        hints.push(format!(
            "The target exited with {} instead of faulting: nothing was corrupted by the mapping. \
             A DllMain / early-init guard (or an external shutdown) called ExitProcess - validate \
             the payload against this exact target version, or try another payload version.",
            exit_text(crash.exit_code)
        ));
    }
    if crash.after_ms < 1500 {
        if clean_exit {
            hints.push(format!(
                "The exit happened about {} ms after injection, i.e. inside DllMain / early initialisation of the payload.",
                crash.after_ms
            ));
        } else {
            hints.push(format!(
                "The crash happened about {} ms after injection, pointing to DllMain or early initialization routine failure.",
                crash.after_ms
            ));
        }
    } else if crash.after_ms > 60_000 {
        hints.push(
            "The crash occurred long after injection; it was likely triggered by subsequent target execution rather than the injection sequence itself."
                .to_string(),
        );
    }
    hints
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(path)
        .to_string()
}

// ===== Windows Application log: crash records =====

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Formats a Unix timestamp as `YYYY-MM-DD HH:MM:SSZ` without external crates.
pub fn fmt_unix(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn ansi_cstr(buf: &[u8], at: usize, end: usize) -> (String, usize) {
    if at >= end {
        return (String::new(), end);
    }
    let limit = end.min(buf.len());
    let mut p = at;
    while p < limit && buf[p] != 0 {
        p += 1;
    }
    let s = String::from_utf8_lossy(&buf[at..p]).to_string();
    let next = if p < limit { p + 1 } else { limit };
    (s, next)
}

/// Reads recent "Application Error" / WER records for this process from the Application log.
/// Returns at most a few formatted lines with source, event id and the event strings
/// (app name, module name, exception code, fault offset, ...).
pub fn crash_events(process_name: &str, pid: u32, since_unix: u64) -> CrashRecords {
    let mut out = CrashRecords::default();
    unsafe {
        let log = OpenEventLogA(std::ptr::null(), b"Application\0".as_ptr());
        if log.is_null() {
            return out;
        }
        let mut buf = vec![0u8; 64 * 1024];
        let mut done = false;
        for _ in 0..8 {
            if done {
                break;
            }
            let mut read = 0u32;
            let mut needed = 0u32;
            let ok = ReadEventLogA(
                log,
                EVENTLOG_SEQUENTIAL_READ | EVENTLOG_BACKWARDS_READ,
                0,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut read,
                &mut needed,
            );
            if ok == 0 {
                let e = kernel32::GetLastError();
                if e == 122 && (needed as usize) > buf.len() && (needed as usize) < 4 * 1024 * 1024 {
                    buf.resize(needed as usize, 0);
                    continue;
                }
                break;
            }

            let mut off = 0usize;
            let total = read as usize;
            while off + 56 <= total {
                let len = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as usize;
                if len < 56 || off + len > total {
                    break;
                }
                let time = u32::from_le_bytes(buf[off + 12..off + 16].try_into().unwrap()) as u64;
                let event_id = u32::from_le_bytes(buf[off + 20..off + 24].try_into().unwrap()) & 0xFFFF;
                let num_strings = u16::from_le_bytes(buf[off + 26..off + 28].try_into().unwrap()) as usize;
                let string_off = u32::from_le_bytes(buf[off + 36..off + 40].try_into().unwrap()) as usize;

                if time < since_unix {
                    done = true;
                    break;
                }
                if event_id == 1000 || event_id == 1001 {
                    let base = off + string_off;
                    if base < off + len {
                        let (source, mut p) = ansi_cstr(&buf, base, off + len);
                        let mut strings = Vec::new();
                        for _ in 0..num_strings {
                            if p >= off + len || p >= total {
                                break;
                            }
                            let (s, n) = ansi_cstr(&buf, p, off + len);
                            strings.push(s);
                            p = n;
                        }
                        let hay = format!("{} {}", strings.join(" "), source).to_lowercase();
                        let wanted_name = process_name.to_lowercase();
                        let pid_text = pid.to_string();
                        let matches = (!wanted_name.is_empty() && hay.contains(&wanted_name))
                            || (pid != 0 && hay.contains(&pid_text));
                        if matches {
                            if event_id == 1000 && strings.len() >= 7 {
                                // Application Error insertion strings (the provider name is the
                                // source and is not part of this list):
                                // [0] app version, [1] app timestamp, [2] module,
                                // [3] module version, [4] module timestamp,
                                // [5] exception code, [6] fault offset, [7] process id, ...
                                out.faulting_module.get_or_insert_with(|| strings[2].clone());
                                out.exception_code.get_or_insert_with(|| strings[5].clone());
                                out.fault_offset.get_or_insert_with(|| strings[6].clone());
                            }
                            out.lines.push(format!(
                                "[{}] {} (event {}) {}",
                                fmt_unix(time),
                                source,
                                event_id,
                                strings.join(" | ")
                            ));
                            if out.lines.len() >= 6 {
                                done = true;
                                break;
                            }
                        }
                    }
                }
                off += len;
            }
        }
        let _ = CloseEventLog(log);
    }
    out
}

// ===== Manual-map tracking =====

fn manual_map_log() -> Option<std::path::PathBuf> {
    let mut buf = [0u16; MAX_PATH];
    unsafe {
        let n = kernel32::GetTempPathW(MAX_PATH as u32, buf.as_mut_ptr());
        if n == 0 {
            return None;
        }
        let temp = String::from_utf16_lossy(&buf[..n as usize]);
        let dir = std::path::Path::new(temp.trim_end_matches('\\')).join("BlackBoxInjector");
        let _ = std::fs::create_dir_all(&dir);
        Some(dir.join("manual_map.log"))
    }
}

/// Records a successful manual-map injection so a later unload can at least wipe its headers.
pub fn record_manual_map(pid: u32, target: &str, path: &str, base: usize) {
    if let Some(log) = manual_map_log() {
        use std::io::Write as _;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
            let _ = writeln!(f, "{}|{}|{}|{}|0x{:X}", now_unix(), pid, target, path, base);
        }
    }
}

/// Base address of a manual-mapped module recorded for this pid and file name.
pub fn manual_map_base(pid: u32, module: &str) -> Option<usize> {
    let text = std::fs::read_to_string(manual_map_log()?).ok()?;
    for line in text.lines().rev() {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 5 {
            continue;
        }
        let line_pid = parts[1].parse::<u32>().ok();
        if line_pid != Some(pid) {
            continue;
        }
        let file = std::path::Path::new(parts[3])
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(parts[3]);
        if file.eq_ignore_ascii_case(module) || parts[3].eq_ignore_ascii_case(module) {
            return usize::from_str_radix(parts[4].trim_start_matches("0x"), 16).ok();
        }
    }
    None
}

// ===== Report file =====

pub fn report_dir() -> Option<std::path::PathBuf> {
    unsafe {
        let mut buf = [0u16; MAX_PATH];
        let n = kernel32::GetTempPathW(MAX_PATH as u32, buf.as_mut_ptr());
        if n != 0 {
            let temp = String::from_utf16_lossy(&buf[..n as usize]);
            let dir = std::path::Path::new(temp.trim_end_matches('\\'))
                .join("BlackBoxInjector")
                .join("reports");
            if std::fs::create_dir_all(&dir).is_ok() {
                let test_file = dir.join(".write_test");
                if std::fs::write(&test_file, b"ok").is_ok() {
                    let _ = std::fs::remove_file(&test_file);
                    return Some(dir);
                }
            }
        }
        let local = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("reports");
        let _ = std::fs::create_dir_all(&local);
        Some(local)
    }
}

pub struct Report<'a> {
    pub target: &'a str,
    pub pid: u32,
    pub arch: &'a str,
    pub header: Vec<String>,
    pub queue: Vec<String>,
    pub results: Vec<String>,
    pub crash: Option<CrashInfo>,
    pub diagnosis: Vec<String>,
}

pub struct ReportOut {
    pub path: String,
    pub body: String,
}

/// Writes the full crash report and returns its path and text.
pub fn write_report(r: &Report) -> Option<ReportOut> {
    let dir = report_dir()?;
    let stamp = fmt_unix(now_unix()).replace(['-', ':', ' '], "").replace('Z', "");
    let prefix = if r.crash.is_some() { "crash" } else { "report" };
    let path = dir.join(format!("{prefix}-{stamp}.txt"));

    let (maj, min, build) = ntdll::os_version();
    let mut s = String::new();
    s.push_str("=== BlackBox Injector crash report ===\n");
    s.push_str(&format!("generated: {}\n", fmt_unix(now_unix())));
    s.push_str(&format!(
        "windows: {}.{}.{} ({}-bit)\n",
        maj,
        min,
        build,
        if cfg!(target_pointer_width = "64") { 64 } else { 32 }
    ));
    s.push_str(&format!("injector: v{}\n", crate::version::VERSION));
    s.push_str(&format!("target: {} (pid {}) {}\n", r.target, r.pid, r.arch));
    s.push_str("\n--- run context ---\n");
    for line in &r.header {
        s.push_str(&format!("{line}\n"));
    }
    s.push_str("\n--- queue ---\n");
    for line in &r.queue {
        s.push_str(&format!("{line}\n"));
    }
    s.push_str("\n--- results ---\n");
    for line in &r.results {
        s.push_str(&format!("{line}\n"));
    }
    if let Some(c) = &r.crash {
        s.push_str("\n--- crash ---\n");
        s.push_str(&format!("exit code: {}\n", exit_text(c.exit_code)));
        s.push_str(&format!("detected about {} ms after the last injection\n", c.after_ms));
        if let Some(m) = &c.faulting_module {
            s.push_str(&format!("faulting module: {m}\n"));
        }
        if let Some(e) = &c.exception_code {
            s.push_str(&format!("exception code: {e}\n"));
        }
        if let Some(o) = &c.fault_offset {
            s.push_str(&format!("fault offset: {o}\n"));
        }
        if c.events.is_empty() {
            s.push_str("application log: no matching crash record was found\n");
        } else {
            s.push_str("application log:\n");
            for e in &c.events {
                s.push_str(&format!("  {e}\n"));
            }
        }
    }
    if !r.diagnosis.is_empty() {
        s.push_str("\n--- diagnosis ---\n");
        for hint in &r.diagnosis {
            s.push_str(&format!("- {hint}\n"));
        }
    }
    std::fs::write(&path, &s).ok()?;
    Some(ReportOut { path: path.display().to_string(), body: s })
}

// ===== DLL details (imports / dependencies) =====

fn machine_text(machine: u16) -> String {
    match machine {
        0x8664 => "x64 (0x8664)".to_string(),
        0x14C => "x86 (0x14C)".to_string(),
        0xAA64 => "ARM64 (0xAA64)".to_string(),
        m => format!("0x{m:04X}"),
    }
}

fn system32_has(name: &str) -> bool {
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    std::path::Path::new(&windir).join("System32").join(name).is_file()
        || std::path::Path::new(&windir).join("SysWOW64").join(name).is_file()
}

/// A readable report for one DLL: header info, exports, TLS callbacks and every imported
/// DLL with whether it is already loaded / available / missing.
pub fn dll_report(path: &str, pid: u32) -> Result<Vec<String>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("cannot read file: {e}"))?;
    let data = std::fs::read(path).map_err(|e| format!("cannot read file: {e}"))?;
    let pe = crate::pe::parse::PeImage::parse(&data)?;

    let mut lines = Vec::new();
    lines.push(format!("file: {path}"));
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| fmt_unix(d.as_secs()))
        .unwrap_or_else(|| "unknown".to_string());
    lines.push(format!("size: {} bytes   modified: {}", meta.len(), modified));
    lines.push(format!(
        "machine: {}   format: {}   sections: {}",
        machine_text(pe.machine),
        if pe.pe32 { "PE32" } else { "PE32+" },
        pe.sections.len()
    ));
    lines.push(format!(
        "image base: 0x{:X}   entry point: 0x{:X}   image size: {} bytes",
        pe.image_base, pe.address_of_entry_point, pe.size_of_image
    ));
    match crate::pe::parse::export_count(&data, &pe) {
        Some(n) => lines.push(format!("exports: {n}")),
        None => lines.push("exports: none".to_string()),
    }
    let tls = crate::pe::parse::tls_callbacks(&data, &pe).map(|v| v.len()).unwrap_or(0);
    lines.push(format!("TLS callbacks: {tls}"));
    let reloc_dir = crate::pe::parse::data_dir(&data, &pe, 5);
    let has_relocs = reloc_dir.map(|(_, sz)| sz > 0).unwrap_or(false);
    lines.push(format!(
        "relocations: {}",
        if has_relocs {
            "present (.reloc)"
        } else {
            "STRIPPED / NONE (fixed base only; manual mapping will fail if preferred base is occupied)"
        }
    ));
    if pe.opt_off + 72 <= data.len() {
        let dll_chars = u16::from_le_bytes(data[pe.opt_off + 70..pe.opt_off + 72].try_into().unwrap());
        let mut sec_flags = Vec::new();
        if dll_chars & 0x0040 != 0 {
            sec_flags.push("ASLR (dynamic base)");
        }
        if dll_chars & 0x0100 != 0 {
            sec_flags.push("DEP (NX)");
        }
        if dll_chars & 0x4000 != 0 {
            sec_flags.push("Guard CF");
        }
        if dll_chars & 0x0400 != 0 {
            sec_flags.push("No SEH");
        }
        if !sec_flags.is_empty() {
            lines.push(format!("security: {}", sec_flags.join(", ")));
        }
    }

    let imports = crate::pe::parse::import_dlls(&data, &pe);
    lines.push(format!("imports ({} DLLs):", imports.len()));
    let mods = if pid != 0 { queue::module_snapshot(pid).ok() } else { None };
    let redist = crate::vcredist::detect();
    let mut missing = Vec::new();
    for dll in &imports {
        let lower = dll.to_lowercase();
        let status = if let Some(mods) = &mods {
            if crate::inject::find_module(mods, dll).is_some() {
                "loaded in target".to_string()
            } else if lower.starts_with("api-ms-win-") {
                "API set (resolved by the loader)".to_string()
            } else if system32_has(dll) {
                "available in System32".to_string()
            } else {
                missing.push(dll.clone());
                "NOT FOUND (missing)".to_string()
            }
        } else if lower.starts_with("api-ms-win-") {
            "API set (resolved by the loader)".to_string()
        } else if system32_has(dll) {
            "available in System32".to_string()
        } else {
            missing.push(dll.clone());
            "NOT FOUND (missing)".to_string()
        };
        lines.push(format!("  {dll:<34} {status}"));
    }

    let vc_needed: Vec<&String> = imports
        .iter()
        .filter(|d| {
            let d = d.to_lowercase();
            d.starts_with("vcruntime140") || d.starts_with("msvcp140")
        })
        .collect();
    if !vc_needed.is_empty() {
        let installed = match pe.machine {
            0x14C => redist.x86.is_some(),
            _ => redist.x64.is_some(),
        };
        lines.push(format!(
            "VC++ runtime: needed ({}) - {}",
            vc_needed.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
            if installed { "installed" } else { "NOT installed" }
        ));
    }
    if !missing.is_empty() {
        lines.push(format!("missing dependencies: {}", missing.join(", ")));
    }
    Ok(lines)
}

/// Short one-line description of a verification for logs.
pub fn verification_line(v: &Verification) -> String {
    let name = file_name(&v.path);
    if !v.alive {
        return format!(
            "[!] {name}: injected, then the target died ({}) after {} ms",
            v.exit_code.map(exit_text).unwrap_or_else(|| "unknown exit code".to_string()),
            v.elapsed_ms
        );
    }
    if v.listed {
        format!(
            "[+] {name}: verified in the target's PEB (0x{:X}, {} ms)",
            v.module_base, v.elapsed_ms
        )
    } else {
        format!("[*] {name}: injected, but {}", v.note)
    }
}

pub fn arch_name(a: Arch) -> &'static str {
    match a {
        Arch::X64 => "x64",
        Arch::X86 => "x86",
    }
}
