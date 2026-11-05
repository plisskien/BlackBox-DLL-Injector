//! Process acquisition: enumeration, privilege escalation, architecture detection.

use crate::util::{wstr, last_error};
use crate::win32::advapi32::*;
use crate::win32::kernel32;
use crate::win32::*;

#[derive(Clone, Debug)]
pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
}

/// True when this process has an elevated (administrator) token.
pub fn is_elevated() -> bool {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(kernel32::GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevated = 0u32;
        let mut returned = 0u32;
        let ok = GetTokenInformation(
            token,
            TOKEN_ELEVATION_CLASS,
            &mut elevated as *mut u32 as *mut c_void_raw,
            std::mem::size_of::<u32>() as u32,
            &mut returned,
        );
        let _ = kernel32::CloseHandle(token);
        ok != 0 && elevated != 0
    }
}

/// Toolhelp32 process snapshot.
pub fn enumerate() -> Vec<ProcessEntry> {
    let mut out = Vec::new();
    unsafe {
        let snap = kernel32::CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut pe = PROCESSENTRY32W::default();
        pe.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if kernel32::Process32FirstW(snap, &mut pe) != 0 {
            loop {
                // Skip PID 0 (System Idle Process: idle CPU execution context, not an accessible user-mode process)
                if pe.th32ProcessID != 0 {
                    let len = pe.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
                    out.push(ProcessEntry {
                        pid: pe.th32ProcessID,
                        name: String::from_utf16_lossy(&pe.szExeFile[..len]),
                    });
                }
                if kernel32::Process32NextW(snap, &mut pe) == 0 {
                    break;
                }
            }
        }
        let _ = kernel32::CloseHandle(snap);
    }
    out.sort_by_cached_key(|p| p.name.to_lowercase());
    out
}

/// Find the first process whose executable name matches (case-insensitive, ".exe" optional).
/// Optimally searches the toolhelp snapshot directly to avoid allocating and sorting the entire process list.
pub fn find_by_name(name: &str) -> Option<ProcessEntry> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let wanted = trimmed.strip_suffix(".exe").unwrap_or(trimmed);
    unsafe {
        let snap = kernel32::CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut pe = PROCESSENTRY32W::default();
        pe.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut fallback: Option<ProcessEntry> = None;
        if kernel32::Process32FirstW(snap, &mut pe) != 0 {
            loop {
                // Skip PID 0
                if pe.th32ProcessID != 0 {
                    let len = pe.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
                    let pname = String::from_utf16_lossy(&pe.szExeFile[..len]);
                    let p_trimmed = pname.strip_suffix(".exe").unwrap_or(&pname);
                    // Match process name; prefer a PID with write access to avoid sandboxed child renderers
                    if p_trimmed.eq_ignore_ascii_case(wanted) || pname.eq_ignore_ascii_case(trimmed) {
                        let h = kernel32::OpenProcess(crate::win32::PROCESS_VM_WRITE, 0, pe.th32ProcessID);
                        if !h.is_null() {
                            let _ = kernel32::CloseHandle(h);
                            let _ = kernel32::CloseHandle(snap);
                            return Some(ProcessEntry {
                                pid: pe.th32ProcessID,
                                name: pname,
                            });
                        }
                        if fallback.is_none() {
                            fallback = Some(ProcessEntry {
                                pid: pe.th32ProcessID,
                                name: pname,
                            });
                        }
                    }
                }
                if kernel32::Process32NextW(snap, &mut pe) == 0 {
                    break;
                }
            }
        }
        let _ = kernel32::CloseHandle(snap);
        fallback
    }
}

/// Enable SeDebugPrivilege on current process token.
pub fn enable_debug_privilege() -> Result<(), String> {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(kernel32::GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) == 0 {
            return Err(last_error("OpenProcessToken"));
        }
        let _g = CloseGuard(token);
        let name = wstr(SE_DEBUG_NAME);
        let mut luid = LUID::default();
        if LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) == 0 {
            return Err(last_error("LookupPrivilegeValue"));
        }
        let tp = TOKEN_PRIVILEGES_FULL {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
        };
        if AdjustTokenPrivileges(token, 0, &tp, 0, std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
            return Err(last_error("AdjustTokenPrivileges"));
        }
        // ERROR_NOT_ALL_ASSIGNED means the privilege isn't held (not elevated); caller decides severity.
        Ok(())
    }
}

struct CloseGuard(HANDLE);
impl Drop for CloseGuard {
    fn drop(&mut self) {
        unsafe {
            kernel32::CloseHandle(self.0);
        }
    }
}

/// Which side of the WOW64 boundary a process is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X64,
    X86,
}

pub fn process_arch(handle: HANDLE) -> Result<Arch, String> {
    unsafe {
        let mut wow64: BOOL = 0;
        if kernel32::IsWow64Process(handle, &mut wow64) == 0 {
            return Err(last_error("IsWow64Process"));
        }
        Ok(if wow64 != 0 { Arch::X86 } else { Arch::X64 })
    }
}

/// Quickly check process architecture by PID.
pub fn process_arch_quick(pid: u32) -> Option<Arch> {
    unsafe {
        let mut h = kernel32::OpenProcess(crate::win32::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            h = kernel32::OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid);
        }
        if h.is_null() {
            return None;
        }
        let res = process_arch(h).ok();
        kernel32::CloseHandle(h);
        res
    }
}

/// Image machine type of a DLL file (0x14C = x86, 0x8664 = x64).
pub fn dll_machine(path: &str) -> Result<u16, String> {
    let data = kernel32::read_file(path)?;
    if data.len() < 0x40 || &data[0..2] != b"MZ" {
        return Err(format!("{path}: not a PE file"));
    }
    let pe_off = u32::from_le_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]) as usize;
    if data.len() < pe_off + 6 || &data[pe_off..pe_off + 4] != b"PE\0\0" {
        return Err(format!("{path}: bad PE header"));
    }
    Ok(u16::from_le_bytes([data[pe_off + 4], data[pe_off + 5]]))
}

pub fn arch_name(a: Arch) -> &'static str {
    match a {
        Arch::X64 => "x64",
        Arch::X86 => "x86 (WOW64)",
    }
}

/// Open with the rights the injector needs.
///
/// `ERROR_ACCESS_DENIED` is often transient on protected targets: anti-cheat and security
/// software briefly restrict handle access while they scan a freshly started binary, and the
/// same `OpenProcess` succeeds a moment later. Retry a few times before reporting failure.
pub fn open_target(pid: u32) -> Result<HANDLE, String> {
    let rights = PROCESS_CREATE_THREAD
        | PROCESS_QUERY_INFORMATION
        | PROCESS_VM_OPERATION
        | PROCESS_VM_WRITE
        | PROCESS_VM_READ;
    const ERROR_ACCESS_DENIED: u32 = 5;
    unsafe {
        let mut last = 0u32;
        for attempt in 0..30 {
            let h = kernel32::OpenProcess(rights, 0, pid);
            if !h.is_null() {
                return Ok(h);
            }
            last = kernel32::GetLastError();
            if last != ERROR_ACCESS_DENIED {
                break;
            }
            if attempt < 29 {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
        let hint = if last == ERROR_ACCESS_DENIED {
            " (run the injector as Administrator; protected targets can deny access while they start)"
        } else {
            ""
        };
        Err(crate::util::format_win_err(
            last,
            &format!("OpenProcess(pid {pid}){hint}"),
        ))
    }
}

/// Process id behind an already-open handle.
pub fn pid_of(h: HANDLE) -> Result<u32, String> {
    unsafe {
        let mut pbi = PROCESS_BASIC_INFORMATION::default();
        let st = (crate::win32::ntdll::get().NtQueryInformationProcess)(
            h,
            ProcessBasicInformation,
            &mut pbi as *mut _ as _,
            std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
            std::ptr::null_mut(),
        );
        if st != 0 {
            return Err(crate::util::nt_error(st, "NtQueryInformationProcess"));
        }
        Ok(pbi.UniqueProcessId as usize as u32)
    }
}
