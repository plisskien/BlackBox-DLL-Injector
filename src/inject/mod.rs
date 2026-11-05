//! Injection engine - supported injection techniques:
//! 0 Standard, 1 LdrLoadDll, 2 Manual Map, 3 Thread Hijack, 4 NtCreateThreadEx.

pub mod apc;
pub mod hijack;
pub mod ldrload;
pub mod manual_map;
pub mod ntthread;
pub mod shellcode;
pub mod standard;
pub mod stealth;

use crate::config::ScrambleOptions;
use crate::process::Arch;
use crate::util::{format_win_err, nt_error, wbuf, wstr};
use crate::win32::kernel32;
use crate::win32::ntdll;
use crate::win32::*;

#[derive(Clone, Debug)]
pub struct InjectOptions {
    pub method: u32,
    pub erase_pe: bool,
    pub hide_module: bool,
    pub stealth_inject: bool,
    pub hide_from_debugger: bool,
    pub manual_resolve_imports: bool,
    pub disable_seh_validation: bool,
    pub disable_exception_support: bool,
    pub scramble: ScrambleOptions,
}

impl Default for InjectOptions {
    fn default() -> Self {
        InjectOptions {
            method: 0,
            erase_pe: false,
            hide_module: false,
            stealth_inject: false,
            hide_from_debugger: false,
            manual_resolve_imports: false,
            disable_seh_validation: false,
            disable_exception_support: false,
            scramble: ScrambleOptions::default(),
        }
    }
}

impl InjectOptions {
    /// Evaluates effective cloaking settings when composite stealth is enabled.
    pub fn effective_erase_pe(&self) -> bool {
        self.erase_pe || self.stealth_inject
    }
    pub fn effective_hide_module(&self) -> bool {
        self.hide_module || self.stealth_inject
    }
    pub fn effective_hide_thread(&self) -> bool {
        self.hide_from_debugger || self.stealth_inject
    }
}

pub type Logger<'a> = &'a dyn Fn(&str);

// ================= remote memory helpers =================

pub fn remote_alloc(h: HANDLE, size: usize, protect: u32) -> Result<usize, String> {
    unsafe {
        let p = kernel32::VirtualAllocEx(h, std::ptr::null_mut(), size, MEM_COMMIT | MEM_RESERVE, protect);
        if p.is_null() {
            return Err(crate::util::last_error("VirtualAllocEx"));
        }
        Ok(p as usize)
    }
}

pub fn remote_free(h: HANDLE, addr: usize) -> Result<(), String> {
    unsafe {
        if kernel32::VirtualFreeEx(h, addr as _, 0, MEM_RELEASE) == 0 {
            return Err(crate::util::last_error("VirtualFreeEx"));
        }
        Ok(())
    }
}

pub fn remote_write(h: HANDLE, addr: usize, data: &[u8]) -> Result<(), String> {
    unsafe {
        if kernel32::WriteProcessMemory(h, addr as _, data.as_ptr() as _, data.len(), std::ptr::null_mut()) == 0 {
            return Err(crate::util::last_error("WriteProcessMemory"));
        }
        Ok(())
    }
}

pub fn remote_read(h: HANDLE, addr: usize, len: usize) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; len];
    unsafe {
        if kernel32::ReadProcessMemory(h, addr as _, buf.as_mut_ptr() as _, len, std::ptr::null_mut()) == 0 {
            return Err(crate::util::last_error("ReadProcessMemory"));
        }
    }
    Ok(buf)
}

pub fn remote_read_u32(h: HANDLE, addr: usize) -> Result<u32, String> {
    let b = remote_read(h, addr, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

pub fn remote_read_u64(h: HANDLE, addr: usize) -> Result<u64, String> {
    let b = remote_read(h, addr, 8)?;
    Ok(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
}

pub fn remote_read_ptr(h: HANDLE, addr: usize, arch: Arch) -> Result<usize, String> {
    match arch {
        Arch::X64 => Ok(remote_read_u64(h, addr)? as usize),
        Arch::X86 => Ok(remote_read_u32(h, addr)? as usize),
    }
}

#[allow(dead_code)]
pub fn remote_write_u32(h: HANDLE, addr: usize, v: u32) -> Result<(), String> {
    remote_write(h, addr, &v.to_le_bytes())
}

// ================= remote module walking =================

#[derive(Clone, Debug)]
pub struct RemoteModule {
    pub name: String,
    pub base: usize,
    pub size: u32,
}

/// Walk the target's PEB module list (x64 PEB or PEB32 for WOW64 targets).
pub fn remote_modules(h: HANDLE, arch: Arch) -> Result<Vec<RemoteModule>, String> {
    let mut out = Vec::new();
    unsafe {
        match arch {
            Arch::X64 => {
                let mut pbi = PROCESS_BASIC_INFORMATION::default();
                let st = (ntdll::get().NtQueryInformationProcess)(
                    h,
                    ProcessBasicInformation,
                    &mut pbi as *mut _ as _,
                    std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
                    std::ptr::null_mut(),
                );
                if st != 0 {
                    return Err(nt_error(st, "NtQueryInformationProcess"));
                }
                let peb = pbi.PebBaseAddress as usize;
                let ldr = remote_read_ptr(h, peb + 0x18, arch)?;
                if ldr == 0 {
                    return Err("remote PEB->Ldr is null".into());
                }
                let mut cur = remote_read_ptr(h, ldr + 0x10, arch)?; // InLoadOrderModuleList.Flink
                let head = ldr + 0x10;
                let mut guard = 0;
                while cur != 0 && cur != head && guard < 4096 {
                    guard += 1;
                    let dll_base = remote_read_ptr(h, cur + 0x30, arch)?;
                    let size = remote_read_u32(h, cur + 0x40)?;
                    let name = read_remote_unicode64(h, cur + 0x58)?; // BaseDllName
                    out.push(RemoteModule { name, base: dll_base, size });
                    cur = remote_read_ptr(h, cur, arch)?; // Flink
                }
            }
            Arch::X86 => {
                let mut peb32: usize = 0;
                let st = (ntdll::get().NtQueryInformationProcess)(
                    h,
                    ProcessWow64Information,
                    &mut peb32 as *mut _ as _,
                    std::mem::size_of::<usize>() as u32,
                    std::ptr::null_mut(),
                );
                if st != 0 {
                    return Err(nt_error(st, "NtQueryInformationProcess(Wow64)"));
                }
                if peb32 == 0 {
                    return Err("ProcessWow64Information returned null PEB32".into());
                }
                let ldr = remote_read_ptr(h, peb32 + 0x0C, arch)?;
                let mut cur = remote_read_ptr(h, ldr + 0x0C, arch)?;
                let head = ldr + 0x0C;
                let mut guard = 0;
                while cur != 0 && cur != head && guard < 4096 {
                    guard += 1;
                    let dll_base = remote_read_u32(h, cur + 0x18)? as usize;
                    let size = remote_read_u32(h, cur + 0x20)?;
                    let name = read_remote_unicode32(h, cur + 0x2C)?; // BaseDllName
                    out.push(RemoteModule { name, base: dll_base, size });
                    cur = remote_read_ptr(h, cur, arch)?;
                }
            }
        }
    }
    Ok(out)
}

unsafe fn read_remote_unicode64(h: HANDLE, addr: usize) -> Result<String, String> {
    let raw = remote_read(h, addr, 16)?;
    let len = u16::from_le_bytes([raw[0], raw[1]]) as usize;
    let buf = u64::from_le_bytes(raw[8..16].try_into().unwrap()) as usize;
    if len == 0 || buf == 0 {
        return Ok(String::new());
    }
    let data = remote_read(h, buf, len)?;
    Ok(String::from_utf16_lossy(data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>().as_slice()))
}

unsafe fn read_remote_unicode32(h: HANDLE, addr: usize) -> Result<String, String> {
    let raw = remote_read(h, addr, 8)?;
    let len = u16::from_le_bytes([raw[0], raw[1]]) as usize;
    let buf = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]) as usize;
    if len == 0 || buf == 0 {
        return Ok(String::new());
    }
    let data = remote_read(h, buf, len)?;
    Ok(String::from_utf16_lossy(data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>().as_slice()))
}

pub fn find_module<'a>(mods: &'a [RemoteModule], name: &str) -> Option<&'a RemoteModule> {
    let want = name.to_lowercase();
    mods.iter().find(|m| {
        let n = m.name.to_lowercase();
        n == want || n.ends_with(&format!("\\{want}"))
    })
}

// Remote export resolution for manual import handling

/// Resolve an export by name directly from the remote module's export directory.
/// Handles forwarded exports ("ntdll.RtlXxx" style strings) by recursing into the
/// forwarding module.
pub fn remote_export(h: HANDLE, arch: Arch, module_base: usize, name: &str, depth: u32) -> Result<usize, String> {
    if depth > 4 {
        return Err("export forwarder chain too deep".into());
    }
    let pe32 = arch == Arch::X86;

    // e_lfanew
    let dos = remote_read(h, module_base, 0x40)?;
    if dos.len() < 0x40 || dos[0] != b'M' || dos[1] != b'Z' {
        return Err("remote module is not a PE image".into());
    }
    let pe_off = u32::from_le_bytes(dos[0x3C..0x40].try_into().unwrap()) as usize;
    let nt = remote_read(h, module_base + pe_off, 0x18 + 0xE0)?;
    if &nt[0..4] != b"PE\0\0" {
        return Err("bad NT signature in remote module".into());
    }
    let opt_off = 0x18usize; // within nt
    let data_dir_off = opt_off + if pe32 { 96 } else { 112 };
    let exp_rva = u32::from_le_bytes(nt[data_dir_off..data_dir_off + 4].try_into().unwrap()) as usize;
    let exp_size = u32::from_le_bytes(nt[data_dir_off + 4..data_dir_off + 8].try_into().unwrap()) as usize;
    if exp_rva == 0 {
        return Err("module has no export directory".into());
    }

    let ed = remote_read(h, module_base + exp_rva, 40)?;
    let num_funcs = u32::from_le_bytes(ed[20..24].try_into().unwrap()) as usize;
    let num_names = u32::from_le_bytes(ed[24..28].try_into().unwrap()) as usize;
    let addr_funcs = u32::from_le_bytes(ed[28..32].try_into().unwrap()) as usize;
    let addr_names = u32::from_le_bytes(ed[32..36].try_into().unwrap()) as usize;
    let addr_ords = u32::from_le_bytes(ed[36..40].try_into().unwrap()) as usize;

    // A corrupt export directory must not make us allocate an absurd buffer.
    if num_names > 65_536 || num_funcs > 65_536 {
        return Err(format!(
            "remote export table is implausible ({num_funcs} functions, {num_names} names)"
        ));
    }
    if num_names == 0 || num_funcs == 0 {
        return Err(format!("remote module exports no names (looking for '{name}')"));
    }

    // Read the name table in one go (bounded).
    let names_bytes = num_names * 4;
    let names_raw = remote_read(h, module_base + addr_names, names_bytes)?;
    let mut found_ord: Option<usize> = None;
    let want = name.as_bytes();
    let read_len = (want.len() + 1).max(64);

    // PE spec requires AddressOfNames to be sorted in ascending ASCII order.
    // Binary search resolves in ~10 remote reads instead of thousands.
    let mut low = 0usize;
    let mut high = num_names;
    while low < high {
        let mid = low + (high - low) / 2;
        let name_rva = u32::from_le_bytes(names_raw[mid * 4..mid * 4 + 4].try_into().unwrap()) as usize;
        let raw = remote_read(h, module_base + name_rva, read_len)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let found_name = &raw[..end];
        match found_name.cmp(want) {
            std::cmp::Ordering::Equal => {
                found_ord = Some(mid);
                break;
            }
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid,
        }
    }

    if found_ord.is_none() {
        for i in 0..num_names {
            let name_rva = u32::from_le_bytes(names_raw[i * 4..i * 4 + 4].try_into().unwrap()) as usize;
            if let Ok(raw) = remote_read(h, module_base + name_rva, read_len) {
                let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
                if &raw[..end] == want {
                    found_ord = Some(i);
                    break;
                }
            }
        }
    }

    let idx = found_ord.ok_or_else(|| format!("export '{name}' not found in remote module"))?;

    let ords_raw = remote_read(h, module_base + addr_ords, num_names * 2)?;
    let fn_idx = u16::from_le_bytes(ords_raw[idx * 2..idx * 2 + 2].try_into().unwrap()) as usize;
    if fn_idx >= num_funcs {
        return Err("export ordinal out of range".into());
    }
    let funcs_raw = remote_read(h, module_base + addr_funcs, num_funcs * 4)?;
    let rva = u32::from_le_bytes(funcs_raw[fn_idx * 4..fn_idx * 4 + 4].try_into().unwrap()) as usize;
    if rva == 0 {
        return Err(format!("export '{name}' resolved to null"));
    }

    // Forwarded export: RVA points inside the export directory itself, holding an
    // ASCII string like "ucrtbase.memcpy". Recurse into the forwarding module.
    if rva >= exp_rva && rva < exp_rva + exp_size.max(1) {
        let raw = remote_read(h, module_base + rva, 128)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let fwd = String::from_utf8_lossy(&raw[..end]).to_string();
        let (fwd_mod, fwd_fn) = fwd.split_once('.').ok_or_else(|| format!("bad forwarder string '{fwd}'"))?;
        if depth > 16 {
            return Err(format!("forwarder chain too deep at '{fwd}'"));
        }
        let fresh = remote_modules(h, arch)?;
        let mut target = find_module(&fresh, &format!("{fwd_mod}.dll")).cloned();

        // API-set contract names (api-ms-win-*, ext-ms-*) are never listed in the PEB; the
        // loader resolves them to a backing module (kernelbase.dll, ...). Do the same, else a
        // plain kernel32 forwarder such as InitOnceExecuteOnce fails to resolve.
        if target.is_none() && (fwd_mod.starts_with("api-ms-") || fwd_mod.starts_with("ext-ms-")) {
            let want = format!("{fwd_mod}.dll");
            if let Some(backing) = api_set_backing_remote(h, arch, &want).or_else(|| api_set_backing_disk(&want)) {
                target = find_module(&fresh, &backing).cloned();
            }
            // Contract names are not real modules; the function is exported by whichever
            // loaded module hosts the contract. Ask every loaded module rather than keeping a
            // hardcoded host list (the API-set schema above is the fast path).
            if target.is_none() {
                let mut candidates: Vec<&str> = Vec::new();
                for host in ["kernelbase.dll", "kernel32.dll", "ntdll.dll", "ucrtbase.dll", "combase.dll", "ole32.dll", "user32.dll", "advapi32.dll"] {
                    candidates.push(host);
                }
                for m in &fresh {
                    if !candidates.iter().any(|c| c.eq_ignore_ascii_case(&m.name)) {
                        candidates.push(&m.name);
                    }
                }
                for host in candidates {
                    if let Some(m) = find_module(&fresh, host) {
                        if m.base == module_base {
                            continue;
                        }
                        if remote_export(h, arch, m.base, fwd_fn, depth + 1).is_ok() {
                            target = Some(m.clone());
                            break;
                        }
                    }
                }
            }
        }
        let target = target.ok_or_else(|| format!("forwarder target module {fwd_mod}.dll not loaded"))?;
        return remote_export(h, arch, target.base, fwd_fn, depth + 1);
    }
    Ok(module_base + rva)
}

/// Ensure a dependency is loaded in the target (remote LoadLibraryW), return the
/// module that owns its exports. API-set forwarders (api-ms-win-crt-*) resolve to
/// their backing module (e.g. ucrtbase) since the PEB never lists the api-set name.
pub fn ensure_remote_module(h: HANDLE, arch: Arch, mods: &[RemoteModule], name: &str, log: Logger) -> Result<RemoteModule, String> {
    if let Some(m) = find_module(mods, name) {
        return Ok(m.clone());
    }
    let kernel = find_module(mods, "kernel32.dll").ok_or("kernel32 not found in target")?;
    let loadlib = remote_export(h, arch, kernel.base, "LoadLibraryW", 0)?;
    let path = wstr(name);
    let path_addr = remote_alloc(h, path.len() * 2, PAGE_READWRITE)?;
    let r = (|| -> Result<(), String> {
        remote_write(h, path_addr, &path_as_bytes(&path))?;
        let th = create_remote_thread(h, arch, loadlib, path_addr, false)?;
        let status = wait_thread_status(th, 15_000);
        let _ = close_handle(th);
        match status {
            ThreadWait::Done(_) => Ok(()),
            ThreadWait::TimedOut => Err(format!(
                "remote LoadLibraryW({name}) did not finish within 15 s"
            )),
            ThreadWait::Failed(e) => Err(e),
        }
    })();
    match &r {
        // Only free the path buffer when the remote call has provably finished; a timed-out
        // loader may still be reading it.
        Ok(()) => {
            let _ = remote_free(h, path_addr);
        }
        Err(_) => log(&format!("[!] Leaving the path buffer for {name} allocated (remote call did not finish)")),
    }
    r?;
    // refresh list
    let fresh = remote_modules(h, arch)?;
    if let Some(m) = find_module(&fresh, name) {
        log(&format!("[+] Loaded dependency {} @ {:#x}", name, m.base));
        return Ok(m.clone());
    }
    // API-set forwarder: resolve the backing module via the target's API Set schema.
    if let Some(backing) = api_set_backing_remote(h, arch, name).or_else(|| api_set_backing_disk(name)) {
        if let Some(m) = find_module(&fresh, &backing) {
            log(&format!("[+] Dependency {} -> backing {} @ {:#x}", name, backing, m.base));
            return Ok(m.clone());
        }
        log(&format!(
            "[!] API-set '{name}' resolved to '{backing}' but that module is not loaded in the target"
        ));
    }
    Err(format!("remote LoadLibraryW({name}) did not register the module"))
}

/// Resolve an api-ms-* forwarder name to its backing DLL by walking the target's
/// PEB->ApiSetMap (the API Set namespace schema the loader itself uses).
fn api_set_backing_remote(h: HANDLE, arch: Arch, dep: &str) -> Option<String> {
    unsafe {
        let peb: usize = match arch {
            Arch::X64 => {
                let mut pbi = PROCESS_BASIC_INFORMATION::default();
                let st = (ntdll::get().NtQueryInformationProcess)(
                    h,
                    ProcessBasicInformation,
                    &mut pbi as *mut _ as _,
                    std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
                    std::ptr::null_mut(),
                );
                if st != 0 {
                    return None;
                }
                pbi.PebBaseAddress as usize
            }
            Arch::X86 => {
                let mut peb32: usize = 0;
                let st = (ntdll::get().NtQueryInformationProcess)(
                    h,
                    ProcessWow64Information,
                    &mut peb32 as *mut _ as _,
                    std::mem::size_of::<usize>() as u32,
                    std::ptr::null_mut(),
                );
                if st != 0 || peb32 == 0 {
                    return None;
                }
                peb32
            }
        };
        // PEB->ApiSetMap: x64 +0x68, x86 +0x38.
        let schema_base = remote_read_ptr(h, peb + if arch == Arch::X64 { 0x68 } else { 0x38 }, arch).ok()?;

        if schema_base == 0 {
            return None;
        }
        // API_SET_NAMESPACE (v6, Windows 10 19H1+ / 11): Version, Size, Flags, Count,
        // EntryOffset, HashOffset, HashFactor.
        let hdr = remote_read(h, schema_base, 32).ok()?;
        let at = |off: usize, buf: &[u8]| u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as usize;
        let version = at(0, &hdr);
        let size = at(4, &hdr);
        let count = at(12, &hdr);
        let entry_off = at(16, &hdr);

        if version != 6
            || size < 0x100
            || size > 0x100_0000
            || count == 0
            || count > 100_000
            || entry_off < 0x1C
            || entry_off + count.saturating_mul(24) > size
        {
            return None;
        }

        // One read for the whole schema; all lookups are then local.
        let schema = remote_read(h, schema_base, size).ok()?;

        let u32_at = |off: usize| -> usize {
            u32::from_le_bytes(schema[off..off + 4].try_into().unwrap()) as usize
        };
        let utf16_at = |off: usize, len: usize| -> Option<String> {
            if len == 0 || len > 2 * 512 || off.checked_add(len)? > schema.len() {
                return None;
            }
            let units: Vec<u16> = schema[off..off + len]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            Some(
                String::from_utf16_lossy(&units)
                    .trim_matches('\0')
                    .to_ascii_lowercase(),
            )
        };
        let want = dep.trim_end_matches(".dll").to_ascii_lowercase();
        // The loader ignores the revision suffix: compare up to the last hyphen
        // (api-ms-win-core-synch-l1-2-0 -> api-ms-win-core-synch-l1-2).
        let want_cut = match want.rfind('-') {
            Some(p) if p > 0 => &want[..p],
            _ => want.as_str(),
        };
        for i in 0..count {
            let e = entry_off + i * 24;
            // API_SET_NAMESPACE_ENTRY_V6: Flags, NameOffset, NameLength, HashedLength,
            // ValueOffset, ValueCount.
            let name_off = u32_at(e + 4);
            let name_len = u32_at(e + 8);
            let hashed_len = u32_at(e + 12);
            let val_off = u32_at(e + 16);
            let val_count = u32_at(e + 20);
            if name_len == 0 || hashed_len == 0 || val_count == 0 || val_off + 20 > schema.len() {
                continue;
            }
            let Some(name) = utf16_at(name_off, name_len) else { continue };
            let hashed: String = name.chars().take(hashed_len / 2).collect();

            if hashed != want_cut && name != want {
                continue;
            }
            // API_SET_VALUE_ENTRY_V6: Flags, NameOffset, NameLength, ValueOffset, ValueLength.
            let host_off = u32_at(val_off + 12);
            let host_len = u32_at(val_off + 16);
            let Some(host) = utf16_at(host_off, host_len) else { continue };
            if host.is_empty() {
                continue;
            }
            return Some(if host.ends_with(".dll") {
                host
            } else {
                format!("{host}.dll")
            });
        }
        None
    }
}

/// Secondary fallback: read the forwarder file from disk (older Windows builds
/// ship api-ms-* as physical forwarder DLLs in System32/SysWOW64).
fn api_set_backing_disk(dep: &str) -> Option<String> {
    let dirs = ["C:\\Windows\\System32", "C:\\Windows\\SysWOW64"];
    for dir in dirs {
        let path = format!("{dir}\\{dep}");
        let Ok(data) = kernel32::read_file(&path) else { continue };
        let Ok(pe) = crate::pe::parse::PeImage::parse(&data) else { continue };
        let Some((rva, _size)) = crate::pe::parse::data_dir(&data, &pe, 0) else { continue };
        if rva == 0 {
            continue;
        }
        let exp_off = local_rva_to_off(&data, &pe, rva)?;
        if exp_off + 40 > data.len() {
            continue;
        }
        let num_names = u32::from_le_bytes(data[exp_off + 24..exp_off + 28].try_into().unwrap()) as usize;
        let addr_names = u32::from_le_bytes(data[exp_off + 32..exp_off + 36].try_into().unwrap()) as usize;
        let addr_funcs = u32::from_le_bytes(data[exp_off + 28..exp_off + 32].try_into().unwrap()) as usize;
        let exp_size = u32::from_le_bytes(data[exp_off + 4..exp_off + 8].try_into().unwrap()) as usize;
        if num_names == 0 {
            continue;
        }
        let _names_off = local_rva_to_off(&data, &pe, addr_names as u32)?;
        let funcs_off = local_rva_to_off(&data, &pe, addr_funcs as u32)?;
        let fn_rva = u32::from_le_bytes(data[funcs_off..funcs_off + 4].try_into().unwrap()) as usize;
        // Forwarder = RVA inside the export directory holding "dllname.function".
        if fn_rva >= rva as usize && fn_rva < rva as usize + exp_size.max(1) {
            let foff = local_rva_to_off(&data, &pe, fn_rva as u32)?;
            let end = data[foff..].iter().position(|&b| b == 0)? + foff;
            let fwd = String::from_utf8_lossy(&data[foff..end]).to_string();
            let (backing, _func) = fwd.split_once('.')?;
            return Some(format!("{backing}.dll"));
        }
    }
    None
}

fn local_rva_to_off(_data: &[u8], pe: &crate::pe::parse::PeImage, rva: u32) -> Option<usize> {
    if rva < pe.size_of_headers {
        return Some(rva as usize);
    }
    for s in &pe.sections {
        if rva >= s.virtual_address && rva < s.virtual_address + s.size_of_raw_data.max(1) {
            return Some(s.pointer_to_raw_data as usize + (rva - s.virtual_address) as usize);
        }
    }
    None
}

pub fn path_as_bytes(path: &Vec<u16>) -> Vec<u8> {
    path.iter().flat_map(|c| c.to_le_bytes()).collect()
}

// ================= threads =================

/// Result of waiting on a remote thread.
pub enum ThreadWait {
    /// The thread signalled; the value is its exit code.
    Done(u32),
    /// The timeout elapsed while the thread is still running.
    TimedOut,
    /// WaitForSingleObject / GetExitCodeThread failed.
    Failed(String),
}

pub fn wait_thread_status(th: HANDLE, timeout_ms: u32) -> ThreadWait {
    unsafe {
        match kernel32::WaitForSingleObject(th, timeout_ms) {
            WAIT_OBJECT_0 => {
                let mut code = 0u32;
                if kernel32::GetExitCodeThread(th, &mut code) == 0 {
                    ThreadWait::Failed(crate::util::last_error("GetExitCodeThread"))
                } else {
                    ThreadWait::Done(code)
                }
            }
            WAIT_TIMEOUT => ThreadWait::TimedOut,
            WAIT_FAILED => ThreadWait::Failed(crate::util::last_error("WaitForSingleObject")),
            other => ThreadWait::Failed(format!("WaitForSingleObject returned {other:#x}")),
        }
    }
}

/// Wait for a remote thread and return its exit code. A timeout is an error: callers must
/// not free remote memory while the thread may still be executing it.
#[allow(dead_code)]
pub fn wait_thread(th: HANDLE, timeout_ms: u32) -> Result<u32, String> {
    match wait_thread_status(th, timeout_ms) {
        ThreadWait::Done(c) => Ok(c),
        ThreadWait::TimedOut => Err(format!(
            "remote thread is still running after {timeout_ms} ms (buffers left allocated)"
        )),
        ThreadWait::Failed(e) => Err(e),
    }
}

/// Create a remote thread; optionally suspended (so the caller can prepare per-thread
/// state - e.g. a TLS slot - before it runs) and optionally hidden from debuggers.
pub fn create_remote_thread_ext(
    h: HANDLE,
    _arch: Arch,
    start: usize,
    param: usize,
    hide: bool,
    suspended: bool,
) -> Result<HANDLE, String> {
    unsafe {
        let mut th: HANDLE = std::ptr::null_mut();
        let mut flags = 0u32;
        if hide {
            flags |= THREAD_CREATE_FLAGS_HIDE_FROM_DEBUGGER;
        }
        if suspended {
            flags |= THREAD_CREATE_FLAGS_CREATE_SUSPENDED;
        }
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
            // Fall back to CreateRemoteThread (older syntax / restricted flags).
            // Win32 CREATE_SUSPENDED is 0x4 (different from the NT flag).
            let cflags = if suspended { 0x0000_0004u32 } else { 0 };
            th = kernel32::CreateRemoteThread(
                h,
                std::ptr::null_mut(),
                0,
                Some(std::mem::transmute::<usize, unsafe extern "system" fn(*mut c_void_raw) -> u32>(start)),
                param as _,
                cflags,
                std::ptr::null_mut(),
            );
            if th.is_null() {
                return Err(format_win_err(crate::win32::kernel32::GetLastError(), "NtCreateThreadEx/CreateRemoteThread"));
            }
        }
        Ok(th)
    }
}

pub fn create_remote_thread(h: HANDLE, arch: Arch, start: usize, param: usize, hide: bool) -> Result<HANDLE, String> {
    create_remote_thread_ext(h, arch, start, param, hide, false)
}

/// Offset of `TEB->ThreadLocalStoragePointer`, the per-module TLS array that
/// `__declspec(thread)` code indexes through (`mov rax, gs:[0x58]`).
pub fn tls_array_offset(arch: Arch) -> usize {
    match arch {
        Arch::X64 => 0x58,
        Arch::X86 => 0x2C,
    }
}

/// Entries a manual map is willing to use in a thread's static TLS array.
pub const MAX_STATIC_TLS: usize = 1024;

pub fn read_tls_array(h: HANDLE, arch: Arch, teb: usize) -> Result<usize, String> {
    match arch {
        Arch::X64 => Ok(remote_read_u64(h, teb + tls_array_offset(arch))? as usize),
        Arch::X86 => Ok(remote_read_u32(h, teb + tls_array_offset(arch))? as usize),
    }
}

pub fn write_tls_array(h: HANDLE, arch: Arch, teb: usize, value: usize) -> Result<(), String> {
    match arch {
        Arch::X64 => remote_write(h, teb + tls_array_offset(arch), &(value as u64).to_le_bytes()),
        Arch::X86 => remote_write(h, teb + tls_array_offset(arch), &(value as u32).to_le_bytes()),
    }
}

pub fn read_tls_array_entry(h: HANDLE, arch: Arch, array: usize, index: usize) -> Result<usize, String> {
    let ptr = std::mem::size_of::<usize>();
    match arch {
        Arch::X64 => Ok(remote_read_u64(h, array + index * ptr)? as usize),
        Arch::X86 => Ok(remote_read_u32(h, array + index * ptr)? as usize),
    }
}

pub fn write_tls_array_entry(h: HANDLE, arch: Arch, array: usize, index: usize, value: usize) -> Result<(), String> {
    let ptr = std::mem::size_of::<usize>();
    match arch {
        Arch::X64 => remote_write(h, array + index * ptr, &(value as u64).to_le_bytes()),
        Arch::X86 => remote_write(h, array + index * ptr, &(value as u32).to_le_bytes()),
    }
}

/// Highest used entry + 1 in a thread's static TLS array (scanning at most `limit` slots -
/// callers pass the loader's module count, so this does not read past the loader's array).
pub fn tls_array_used(h: HANDLE, arch: Arch, array: usize, limit: usize) -> usize {
    let mut last = 0usize;
    for i in 0..limit.min(MAX_STATIC_TLS) {
        if let Ok(v) = read_tls_array_entry(h, arch, array, i) {
            if v != 0 {
                last = i + 1;
            }
        }
    }
    last
}

/// Give a thread a static TLS block for `index`, growing (or creating) its
/// `ThreadLocalStoragePointer` array. Existing entries are copied, so the CRT's own TLS
/// blocks keep working after the array is replaced. `new_array` must come from the target's
/// own heap: the loader frees the array when the thread exits. `capacity` is the number of
/// entries in `new_array` (must be > index).
pub fn set_static_tls_slot(
    h: HANDLE,
    arch: Arch,
    teb: usize,
    index: usize,
    block: usize,
    new_array: usize,
    capacity: usize,
) -> Result<(), String> {
    if index >= capacity || capacity > MAX_STATIC_TLS {
        return Err(format!(
            "static TLS index {index} does not fit the {capacity}-entry array"
        ));
    }
    let old = read_tls_array(h, arch, teb)?;
    // The loader's own entries are exactly the modules that have a TLS index; that count is
    // the index we were given, so scanning that far covers everything that exists.
    let used = if old != 0 { tls_array_used(h, arch, old, index.min(capacity)) } else { 0 };
    if old != 0 {
        for i in 0..used.min(capacity) {
            if let Ok(v) = read_tls_array_entry(h, arch, old, i) {
                if v != 0 {
                    write_tls_array_entry(h, arch, new_array, i, v)?;
                }
            }
        }
    }
    write_tls_array_entry(h, arch, new_array, index, block)?;
    write_tls_array(h, arch, teb, new_array)?;
    Ok(())
}

/// Address of the thread's TEB as the *target* sees it. For WOW64 threads the 64-bit TEB
/// stores the 32-bit TEB in its first field; the 32-bit TLS slots live there.
pub fn thread_teb(h: HANDLE, tid: u32, arch: Arch) -> Result<usize, String> {
    unsafe {
        let th = kernel32::OpenThread(THREAD_QUERY_INFORMATION, 0, tid);
        if th.is_null() {
            return Err(crate::util::last_error("OpenThread(THREAD_QUERY_INFORMATION)"));
        }
        let _g = HandleGuard(th);
        let mut buf = [0u8; 64];
        let st = (ntdll::get().NtQueryInformationThread)(
            th,
            0, // ThreadBasicInformation
            buf.as_mut_ptr() as _,
            48,
            std::ptr::null_mut(),
        );
        if st != 0 {
            return Err(nt_error(st, "NtQueryInformationThread(ThreadBasicInformation)"));
        }
        let teb64 = u64::from_le_bytes(buf[8..16].try_into().unwrap()) as usize;
        if teb64 == 0 {
            return Err("thread has no TEB".into());
        }
        match arch {
            Arch::X64 => Ok(teb64),
            Arch::X86 => {
                // WOW64 keeps the 32-bit TEB pointer in the first qword of the 64-bit TEB.
                let p = remote_read_u64(h, teb64)?;
                let p32 = (p & 0xFFFF_FFFF) as usize;
                if p32 < 0x10000 || p32 >= 0x8000_0000 {
                    return Err(format!("cannot resolve the 32-bit TEB of thread {tid}"));
                }
                Ok(p32)
            }
        }
    }
}

pub fn close_handle(h: HANDLE) -> Result<(), String> {
    unsafe {
        if kernel32::CloseHandle(h) == 0 {
            return Err(crate::util::last_error("CloseHandle"));
        }
        Ok(())
    }
}

// ================= dispatcher =================

pub const METHOD_NAMES: [&str; 6] = [
    "Standard (CreateRemoteThread + LoadLibrary)",
    "LdrLoadDll",
    "Manual Map",
    "Thread Hijacking",
    "NtCreateThreadEx",
    "APC (threadless)",
];

#[allow(dead_code)]
pub struct InjectJob {
    pub pid: u32,
    pub dll_path: String,
    pub opts: InjectOptions,
}

/// Validate + dispatch. Returns injected remote base (manual map) or module base estimate.
pub fn inject(pid: u32, dll_path: &str, opts: &InjectOptions, log: Logger) -> Result<usize, String> {
    let image = kernel32::read_file(dll_path)?;
    if image.len() < 0x40 || &image[0..2] != b"MZ" {
        return Err(format!("{dll_path} is not a PE file"));
    }
    let machine = crate::process::dll_machine(dll_path)?;

    let h = crate::process::open_target(pid)?;
    let _g = HandleGuard(h);
    let arch = crate::process::process_arch(h)?;

    // architecture compatibility
    let target_is_x64 = arch == Arch::X64;
    let dll_is_x64 = machine == 0x8664;
    if target_is_x64 != dll_is_x64 {
        return Err(format!(
            "architecture mismatch: target is {}, DLL is {}",
            crate::process::arch_name(arch),
            if dll_is_x64 { "x64" } else { "x86" }
        ));
    }

    // Scramble the image locally before transfer if options are active.
    let mut work = image.clone();
    if opts.scramble.any() {
        log("[*] Scrambling PE image...");
        crate::pe::scrambler::scramble(&mut work, &opts.scramble, log)?;
    }

    let dll_name = std::path::Path::new(dll_path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(dll_path);

    log(&format!(
        "[*] Injecting '{}' into pid {} ({}) using {}",
        dll_name,
        pid,
        crate::process::arch_name(arch),
        METHOD_NAMES[opts.method.min(5) as usize]
    ));

    // Loader-based methods (0, 1, 4) need the DLL on disk; a scrambled image is
    // materialized as a temp file, otherwise the original path is used as-is.
    let mut temp_dll: Option<String> = None;
    let load_path: String = if opts.method == 0 || opts.method == 1 || opts.method == 3 || opts.method == 4 || opts.method == 5 {
        if opts.scramble.any() {
            let tmp = crate::pe::scrambler::write_temp_dll(dll_path, &work)?;
            let tmp_name = std::path::Path::new(&tmp)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(&tmp);
            log(&format!("[*] Scrambled copy: {}", tmp_name));
            temp_dll = Some(tmp.clone());
            tmp
        } else {
            dll_path.to_string()
        }
    } else {
        String::new()
    };

    // APC can fire after this process exits, so its temp file must outlive the run.
    let keep_temp = opts.method == 5;

    let inject_result = (|| -> Result<usize, String> {
        match opts.method {
            0 => standard::inject(h, arch, &load_path, opts, log),
            1 => ldrload::inject(h, arch, &load_path, opts, log),
            2 => manual_map::inject(h, arch, dll_path, &work, opts, log),
            3 => hijack::inject(h, arch, &load_path, opts, log),
            4 => ntthread::inject(h, arch, &load_path, opts, log),
            5 => apc::inject(pid, h, arch, &load_path, opts, log),
            m => Err(format!("unknown method {m}")),
        }
    })();

    let base = match inject_result {
        Ok(b) => b,
        Err(e) => {
            if let (Some(tmp), false) = (&temp_dll, keep_temp) {
                let _ = std::fs::remove_file(tmp);
            }
            return Err(e);
        }
    };

    // Post-injection cloaking. HideModule only applies to loader-registered modules;
    // every cloaking failure is reported but does not undo the injection.
    let mut cloak_warnings = Vec::new();
    if opts.method != 2 && opts.effective_hide_module() {
        log("[*] Cloaking: hiding module from PEB...");
        if let Err(e) = stealth::hide_module(h, arch, base) {
            log(&format!("[!] HideModule: {e}"));
            cloak_warnings.push(format!("HideModule: {e}"));
        }
    }
    if opts.effective_erase_pe() {
        log("[*] Cloaking: erasing PE headers...");
        if let Err(e) = stealth::erase_pe(h, arch, base) {
            log(&format!("[!] ErasePE: {e}"));
            cloak_warnings.push(format!("ErasePE: {e}"));
        }
    }

    if base == 0 {
        return Err(format!(
            "{} reported success but the module base is unknown (module not found in the target)",
            dll_name
        ));
    }

    if cloak_warnings.is_empty() {
        log(&format!("[+] Injection succeeded (base {:#x})", base));
    } else {
        log(&format!(
            "[+] Module loaded (base {:#x}) but cloaking failed: {}",
            base,
            cloak_warnings.join("; ")
        ));
    }

    if let (Some(tmp), false) = (&temp_dll, keep_temp) {
        let _ = std::fs::remove_file(tmp);
    }
    Ok(base)
}

pub struct HandleGuard(pub HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = kernel32::CloseHandle(self.0);
        }
    }
}



