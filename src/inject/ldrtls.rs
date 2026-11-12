//! Loader-managed static TLS for a manually mapped image, without loading a helper module.
//!
//! The Windows loader owns the static TLS index space and the per-thread TLS vectors. A
//! manually mapped image is not in the loader's module lists, so it has no index and no thread
//! blocks. Instead of loading a real seed DLL (which opens an image file and is rejected by
//! image-load filters such as CS2 Trusted Mode), this module calls the loader's own
//! `LdrpHandleTlsData` on a synthetic `LDR_DATA_TABLE_ENTRY` that points at the mapped image.
//!
//! `LdrpHandleTlsData` is called under the loader lock. It:
//!   - reads the image's TLS directory from `DllBase`,
//!   - allocates a real index with `LdrpAllocateTlsEntry` and writes it through `AddressOfIndex`,
//!   - inserts an `LDRP_TLS_ENTRY` into `LdrpTlsList`, so future TLS module loads preserve the
//!     index and future threads get a block through `LdrpInitializeThread` / `LdrpAllocateTls`,
//!   - builds and installs the TLS vector for every existing thread through
//!     `NtSetInformationProcess(ProcessTlsInformation)`.
//!
//! No file is opened and no `LoadLibraryW` / `LdrLoadDll` call is made. `LdrpHandleTlsData` is not
//! exported; it is resolved by name from the matching ntdll PDB, downloaded once from the
//! Microsoft symbol server and cached, so the path works across Windows 10/11 ntdll builds. If
//! the PDB is neither cached nor downloadable, registration fails with a normal error; there is
//! no seed fallback.

use super::*;
use crate::pe::parse::{self, PeImage};
use crate::win32::{dbghelp, kernel32, urlmon};
use std::path::{Path, PathBuf};

/// Synthetic `LDR_DATA_TABLE_ENTRY` allocation. The Windows 11 structure is 0x138 bytes; a
/// 0x200-byte zero-filled allocation covers the Windows 10 layouts too. Only `DllBase` and
/// `SizeOfImage` need stable offsets; the other fields are read as zero or written by the loader.
const LDR_ENTRY_SIZE: usize = 0x200;
const LDR_ENTRY_DLL_BASE: usize = 0x30;
const LDR_ENTRY_SIZE_OF_IMAGE: usize = 0x40;
const LDR_ENTRY_FLAGS: usize = 0x68;
const LDR_ENTRY_LOAD_COUNT: usize = 0x6C;
const LDR_ENTRY_TLS_INDEX: usize = 0x6E;
const LDR_ENTRY_LOAD_REASON: usize = 0x10C;

/// `IMAGE_FILE_DLL` entry flag (`LDR_DATA_TABLE_ENTRY.Flags.ImageDll`).
const LDR_IMAGE_DLL: u32 = 0x4;

/// `ProcessTlsInformation` timeout: the loader call can briefly wait on the loader lock.
const TLS_REGISTER_TIMEOUT_MS: u32 = 60_000;

/// Register `base`'s TLS directory with the loader and return the loader-assigned index.
///
/// `address_of_index` is the payload's `AddressOfIndex` VA (inside the mapped image). The TLS
/// entry allocated by the loader remains in `LdrpTlsList`; its synthetic `LDR_DATA_TABLE_ENTRY`
/// is deliberately kept in the target, because the loader stores a pointer to it and may use it
/// for later thread initialisation or callback delivery.
pub fn register(
    h: HANDLE,
    arch: Arch,
    base: usize,
    size_of_image: u32,
    address_of_index: usize,
    hide_threads: bool,
    log: Logger,
    unsafe_to_free: &mut bool,
) -> Result<u32, String> {
    if arch != Arch::X64 {
        return Err("loader TLS registration is x64-only".into());
    }
    if address_of_index == 0 {
        return Err("TLS directory has no AddressOfIndex".into());
    }

    let mods = remote_modules(h, arch)?;
    let ntdll = find_module(&mods, "ntdll.dll").ok_or("ntdll.dll not found in target")?;
    let handle = resolve_from_pdb(h, ntdll.base, log).map_err(|e| {
        format!(
            "TLS-PDB-01: static TLS registration requires ntdll symbols: {e}. Connect to the \
             internet (Microsoft symbol server) or place the matching ntdll.pdb in the BlackBox \
             symbol cache, then retry"
        )
    })?;
    let lock = remote_export(h, arch, ntdll.base, "LdrLockLoaderLock", 0)?;
    let unlock = remote_export(h, arch, ntdll.base, "LdrUnlockLoaderLock", 0)?;

    let entry = build_fake_entry(h, base, size_of_image)?;
    log(&format!(
        "[*] TLS: registering mapped image with the loader (LDR entry {entry:#x}, LdrpHandleTlsData {handle:#x})"
    ));

    // A success value from LdrpHandleTlsData is 0, so pre-fill AddressOfIndex with an impossible
    // index. If the call did not actually install TLS (for example no active threads), the
    // sentinel remains and is rejected below.
    remote_write(h, address_of_index, &0xFFFF_FFFFu32.to_le_bytes())?;

    let stub_len = shellcode::tls_register_stub_x64(lock, handle, unlock, entry, 0).len();
    let buf = remote_alloc(h, 8 + stub_len, PAGE_EXECUTE_READWRITE)?;
    let cookie_addr = buf;
    let code_addr = buf + 8;
    let stub = shellcode::tls_register_stub_x64(lock, handle, unlock, entry, cookie_addr);
    remote_write(h, cookie_addr, &0u64.to_le_bytes())?;
    remote_write(h, code_addr, &stub)?;

    let result = (|| -> Result<(), String> {
        let th = create_remote_thread_ext(h, arch, code_addr, 0, hide_threads, false)?;
        let _g = HandleGuard(th);
        match wait_thread_status(th, TLS_REGISTER_TIMEOUT_MS) {
            ThreadWait::Done(status) => {
                if (status as i32) < 0 {
                    return Err(crate::util::nt_error(
                        status as i32,
                        "LdrpHandleTlsData",
                    ));
                }
                Ok(())
            }
            ThreadWait::TimedOut => {
                *unsafe_to_free = true;
                Err(format!(
                    "LdrpHandleTlsData did not return within {} s",
                    TLS_REGISTER_TIMEOUT_MS / 1000
                ))
            }
            ThreadWait::Failed(e) => {
                *unsafe_to_free = true;
                Err(e)
            }
        }
    })();

    if !*unsafe_to_free {
        let _ = remote_free(h, buf);
    }
    if let Err(e) = &result {
        // The loader may have inserted the TLS entry before failing; keep the payload and the
        // synthetic entry so no loader state points at freed memory.
        *unsafe_to_free = true;
        log(&format!("[!] Loader TLS registration failed: {e}"));
        return Err(e.clone());
    }

    let index = match remote_read_u32(h, address_of_index) {
        Ok(v) => v,
        Err(e) => {
            *unsafe_to_free = true;
            return Err(format!("cannot read AddressOfIndex after TLS registration: {e}"));
        }
    };
    if index >= MAX_STATIC_TLS as u32 {
        *unsafe_to_free = true;
        return Err(format!(
            "loader assigned TLS index {index:#x}, outside the {MAX_STATIC_TLS}-slot limit"
        ));
    }
    log(&format!("[+] TLS: loader-assigned index {index}"));
    Ok(index)
}

/// Build the synthetic `LDR_DATA_TABLE_ENTRY` used by `LdrpHandleTlsData`. Only the fields the
/// loader reads are populated; the rest stays zero. The allocation is leaked into the target on
/// purpose: `LDRP_TLS_ENTRY.Module` stores this pointer.
fn build_fake_entry(h: HANDLE, base: usize, size_of_image: u32) -> Result<usize, String> {
    let entry = remote_alloc(h, LDR_ENTRY_SIZE, PAGE_READWRITE)?;
    remote_write(h, entry + LDR_ENTRY_DLL_BASE, &(base as u64).to_le_bytes())?;
    remote_write(h, entry + LDR_ENTRY_SIZE_OF_IMAGE, &size_of_image.to_le_bytes())?;
    remote_write(h, entry + LDR_ENTRY_FLAGS, &LDR_IMAGE_DLL.to_le_bytes())?;
    remote_write(h, entry + LDR_ENTRY_LOAD_COUNT, &1u16.to_le_bytes())?;
    remote_write(h, entry + LDR_ENTRY_TLS_INDEX, &0u16.to_le_bytes())?;
    remote_write(h, entry + LDR_ENTRY_LOAD_REASON, &0u32.to_le_bytes())?;
    Ok(entry)
}

/// Local ntdll identity and PDB coordinates.
struct LocalNtdll {
    path: String,
    base: usize,
    size: u32,
    timestamp: u32,
    pdb_name: String,
    pdb_key: String,
}

/// Resolve `LdrpHandleTlsData` from the matching ntdll PDB. This is the cross-version path:
/// public Microsoft symbols contain the internal name, so one resolver works for every Windows
/// 10/11 ntdll build. The matching PDB is cached locally; if it is not present it is downloaded
/// once from the Microsoft symbol server.
fn resolve_from_pdb(h: HANDLE, ntdll_base: usize, log: Logger) -> Result<usize, String> {
    let local = local_ntdll_info()?;
    let (target_ts, target_size) = target_ntdll_identity(h, ntdll_base)?;
    if target_ts != local.timestamp || target_size != local.size {
        return Err(format!(
            "target ntdll (ts {target_ts:#x}, size {target_size:#x}) differs from the local ntdll \
             (ts {:#x}, size {:#x})",
            local.timestamp, local.size
        ));
    }
    let cache = symbol_cache_root()?;
    ensure_pdb(&local, &cache, log)?;

    let proc = unsafe {
        kernel32::OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            0,
            kernel32::GetCurrentProcessId(),
        )
    };
    if proc.is_null() {
        return Err(crate::util::last_error("OpenProcess(self)"));
    }
    let cache_w = crate::util::wstr(&cache.to_string_lossy());
    let path_w = crate::util::wstr(&local.path);
    let module_w = crate::util::wstr("ntdll");
    let symbol_w = crate::util::wstr("ntdll!LdrpHandleTlsData");
    let mut resolved = None;
    unsafe {
        dbghelp::SymSetOptions(dbghelp::SYMOPT_UNDNAME | dbghelp::SYMOPT_DEFERRED_LOADS);
        if dbghelp::SymInitializeW(proc, cache_w.as_ptr(), 0) != 0 {
            let _ = dbghelp::SymSetSearchPathW(proc, cache_w.as_ptr());
            let _ = dbghelp::SymLoadModuleExW(
                proc,
                std::ptr::null_mut(),
                path_w.as_ptr(),
                module_w.as_ptr(),
                local.base as u64,
                local.size,
                std::ptr::null_mut(),
                0,
            );
            let mut sym = dbghelp::SymbolInfoW::default();
            if dbghelp::SymFromNameW(proc, symbol_w.as_ptr(), &mut sym) != 0
                && sym.address >= local.base as u64
            {
                resolved = Some(sym.address as usize);
            }
            let _ = dbghelp::SymCleanup(proc);
        }
        let _ = kernel32::CloseHandle(proc);
    }
    let local_va = resolved.ok_or_else(|| {
        format!(
            "dbghelp could not resolve ntdll!LdrpHandleTlsData (PDB missing or mismatched; cache {})",
            cache.display()
        )
    })?;
    let rva = local_va - local.base;
    let target = ntdll_base + rva;
    log(&format!(
        "[*] TLS: LdrpHandleTlsData resolved from ntdll PDB (RVA {rva:#x}) at {target:#x}"
    ));
    Ok(target)
}

fn local_ntdll_info() -> Result<LocalNtdll, String> {
    let mut buf = [0u16; 260];
    let n = unsafe { kernel32::GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 || n >= buf.len() {
        return Err("GetSystemDirectoryW failed".into());
    }
    let path = format!("{}\\ntdll.dll", String::from_utf16_lossy(&buf[..n]));
    let data = std::fs::read(&path).map_err(|e| format!("read {path}: {e}"))?;
    let pe = PeImage::parse(&data).map_err(|e| format!("{path}: {e}"))?;
    let pe_off = u32::from_le_bytes(data[0x3C..0x40].try_into().unwrap()) as usize;
    let timestamp = u32::from_le_bytes(data[pe_off + 8..pe_off + 12].try_into().unwrap());
    let (pdb_name, pdb_key) = pdb_identity(&data, &pe)?;
    let name = crate::util::wstr("ntdll.dll");
    let base = unsafe { kernel32::GetModuleHandleW(name.as_ptr()) } as usize;
    if base == 0 {
        return Err("local ntdll base is null".into());
    }
    Ok(LocalNtdll {
        path,
        base,
        size: pe.size_of_image,
        timestamp,
        pdb_name,
        pdb_key,
    })
}

fn target_ntdll_identity(h: HANDLE, ntdll_base: usize) -> Result<(u32, u32), String> {
    let dos = remote_read(h, ntdll_base, 0x40)?;
    if dos.len() < 0x40 || dos[0] != b'M' || dos[1] != b'Z' {
        return Err("target ntdll is not a PE image".into());
    }
    let pe_off = u32::from_le_bytes(dos[0x3C..0x40].try_into().unwrap()) as usize;
    let nt = remote_read(h, ntdll_base + pe_off, 0x18 + 0x40)?;
    let timestamp = u32::from_le_bytes(nt[8..12].try_into().unwrap());
    let size = u32::from_le_bytes(nt[0x18 + 56..0x18 + 60].try_into().unwrap());
    Ok((timestamp, size))
}

fn symbol_cache_root() -> Result<PathBuf, String> {
    // The standard BlackBox temporary directory already used by reports and the VC++ runtime
    // helper. The PDB is cached here so later injections never download it again.
    let mut candidates = vec![std::env::temp_dir()
        .join("BlackBoxInjector")
        .join("symbols")];
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local)
                .join("BlackBoxInjector")
                .join("symbols"),
        );
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        candidates.push(dir.join("BlackBoxSymbols"));
    }
    let mut last = String::new();
    for dir in candidates {
        match std::fs::create_dir_all(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) => last = format!("{}: {e}", dir.display()),
        }
    }
    Err(format!("no writable symbol cache ({last})"))
}

fn ensure_pdb(local: &LocalNtdll, cache: &Path, log: Logger) -> Result<PathBuf, String> {
    let dir = cache.join(&local.pdb_name).join(&local.pdb_key);
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let path = dir.join(&local.pdb_name);
    if valid_pdb(&path) {
        return Ok(path);
    }
    let url = format!(
        "https://msdl.microsoft.com/download/symbols/{}/{}/{}",
        local.pdb_name, local.pdb_key, local.pdb_name
    );
    log(&format!(
        "[*] TLS: downloading ntdll symbols ({} bytes expected, cached afterwards)",
        "~2 MB"
    ));
    let url_w = crate::util::wstr(&url);
    let path_w = crate::util::wstr(&path.to_string_lossy());
    let hr = unsafe {
        urlmon::URLDownloadToFileW(
            std::ptr::null_mut(),
            url_w.as_ptr(),
            path_w.as_ptr(),
            0,
            std::ptr::null_mut(),
        )
    };
    if hr != 0 || !valid_pdb(&path) {
        let _ = std::fs::remove_file(&path);
        return Err(format!(
            "ntdll PDB download failed (HRESULT {hr:#x}); expected at {}",
            path.display()
        ));
    }
    Ok(path)
}

fn valid_pdb(path: &Path) -> bool {
    std::fs::read(path)
        .map(|d| d.starts_with(b"Microsoft C/C++ MSF 7.00"))
        .unwrap_or(false)
}

fn pdb_identity(data: &[u8], pe: &PeImage) -> Result<(String, String), String> {
    let (rva, size) = parse::data_dir(data, pe, 6).ok_or("ntdll has no debug directory")?;
    if rva == 0 || size < 28 {
        return Err("ntdll debug directory is empty".into());
    }
    let off = rva_to_off(pe, rva).ok_or("ntdll debug directory RVA is unmapped")?;
    let count = (size as usize) / 28;
    for i in 0..count {
        let e = off + i * 28;
        if e + 28 > data.len() {
            break;
        }
        let kind = u32::from_le_bytes(data[e + 12..e + 16].try_into().unwrap());
        if kind != 2 {
            continue;
        }
        let raw = u32::from_le_bytes(data[e + 24..e + 28].try_into().unwrap()) as usize;
        if raw + 24 > data.len() || &data[raw..raw + 4] != b"RSDS" {
            continue;
        }
        let mut guid = [0u8; 16];
        guid.copy_from_slice(&data[raw + 4..raw + 20]);
        let age = u32::from_le_bytes(data[raw + 20..raw + 24].try_into().unwrap());
        let end = data[raw + 24..]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(0);
        let name = String::from_utf8_lossy(&data[raw + 24..raw + 24 + end]).to_string();
        if name.is_empty() {
            continue;
        }
        return Ok((name, guid_age_key(&guid, age)));
    }
    Err("ntdll PDB CodeView entry not found".into())
}

fn rva_to_off(pe: &PeImage, rva: u32) -> Option<usize> {
    for s in &pe.sections {
        if rva >= s.virtual_address
            && rva < s.virtual_address + s.size_of_raw_data.max(1)
        {
            return Some(s.pointer_to_raw_data as usize + (rva - s.virtual_address) as usize);
        }
    }
    None
}

fn guid_age_key(guid: &[u8; 16], age: u32) -> String {
    let d1 = u32::from_le_bytes(guid[0..4].try_into().unwrap());
    let d2 = u16::from_le_bytes(guid[4..6].try_into().unwrap());
    let d3 = u16::from_le_bytes(guid[6..8].try_into().unwrap());
    let mut tail = String::with_capacity(16);
    for b in &guid[8..16] {
        tail.push_str(&format!("{b:02X}"));
    }
    format!("{d1:08X}{d2:04X}{d3:04X}{tail}{age:X}")
}


