//! Method 0: Standard - CreateRemoteThread with remote LoadLibraryW.

use super::*;

const LOAD_TIMEOUT_MS: u32 = 60_000;

/// Stub that calls `LoadLibraryW(path)` and stores the returned HMODULE at `out`.
fn load_stub(arch: Arch, loadlib: usize, path: usize, out: usize) -> Vec<u8> {
    match arch {
        Arch::X64 => {
            let mut b = Vec::with_capacity(40);
            b.extend_from_slice(&[0x48, 0xB8]); // mov rax, loadlib
            b.extend_from_slice(&(loadlib as u64).to_le_bytes());
            b.extend_from_slice(&[0x48, 0xB9]); // mov rcx, path
            b.extend_from_slice(&(path as u64).to_le_bytes());
            b.extend_from_slice(&[0x48, 0x83, 0xEC, 0x28]); // sub rsp, 0x28
            b.extend_from_slice(&[0xFF, 0xD0]); // call rax
            b.extend_from_slice(&[0x48, 0x83, 0xC4, 0x28]); // add rsp, 0x28
            b.extend_from_slice(&[0x48, 0xBA]); // mov rdx, out
            b.extend_from_slice(&(out as u64).to_le_bytes());
            b.extend_from_slice(&[0x48, 0x89, 0x02]); // mov [rdx], rax
            b.extend_from_slice(&[0x31, 0xC0, 0xC3]); // xor eax, eax; ret
            b
        }
        Arch::X86 => {
            let mut b = Vec::with_capacity(20);
            b.push(0xB8); // mov eax, loadlib
            b.extend_from_slice(&(loadlib as u32).to_le_bytes());
            b.push(0x68); // push path
            b.extend_from_slice(&(path as u32).to_le_bytes());
            b.extend_from_slice(&[0xFF, 0xD0]); // call eax
            b.push(0xA3); // mov [out], eax
            b.extend_from_slice(&(out as u32).to_le_bytes());
            b.extend_from_slice(&[0x31, 0xC0, 0xC3]); // xor eax, eax; ret
            b
        }
    }
}

/// Load `dll_path` with `LoadLibraryW` through `create_thread`, capture the full pointer-sized
/// HMODULE from the stub and verify that it points at a PE image. The PEB module list is only used
/// for a log line: a DLL that unlinks itself is still a successful load and gets its real base.
pub(crate) fn load_library_capture<F>(
    h: HANDLE,
    arch: Arch,
    dll_path: &str,
    log: Logger,
    create_thread: F,
) -> Result<usize, String>
where
    F: FnOnce(usize) -> Result<HANDLE, String>,
{
    let mods = remote_modules(h, arch)?;
    let kernel = find_module(&mods, "kernel32.dll").ok_or("kernel32.dll not found in target")?;
    let loadlib = remote_export(h, arch, kernel.base, "LoadLibraryW", 0)?;
    log(&format!("[*] Remote kernel32!LoadLibraryW @ {loadlib:#x}"));

    let path = wstr(dll_path);
    let path_addr = remote_alloc(h, path.len() * 2, PAGE_READWRITE)?;
    remote_write(h, path_addr, &path_as_bytes(&path))?;

    let out_addr = remote_alloc(h, 8, PAGE_READWRITE)?;
    remote_write(h, out_addr, &[0u8; 8])?;
    let stub = load_stub(arch, loadlib, path_addr, out_addr);
    let stub_addr = remote_alloc(h, stub.len(), PAGE_EXECUTE_READWRITE)?;
    remote_write(h, stub_addr, &stub)?;

    let result = (|| -> Result<usize, String> {
        let th = create_thread(stub_addr)?;
        let _g = HandleGuard(th);
        match wait_thread_status(th, LOAD_TIMEOUT_MS) {
            ThreadWait::Done(_) => {}
            ThreadWait::TimedOut => {
                return Err(format!(
                    "LoadLibraryW did not return within {} s (the DLL is blocking inside DllMain)",
                    LOAD_TIMEOUT_MS / 1000
                ))
            }
            ThreadWait::Failed(e) => return Err(e),
        }
        let base = match arch {
            Arch::X64 => {
                let b = remote_read(h, out_addr, 8)?;
                u64::from_le_bytes(b[0..8].try_into().unwrap()) as usize
            }
            Arch::X86 => {
                let b = remote_read(h, out_addr, 4)?;
                u32::from_le_bytes(b[0..4].try_into().unwrap()) as usize
            }
        };
        if base == 0 {
            return Err("LoadLibraryW returned NULL (module rejected or dependency missing)".into());
        }
        let dos = remote_read(h, base, 0x40)?;
        if dos.len() < 0x40 || &dos[0..2] != b"MZ" {
            return Err(format!("LoadLibraryW returned {base:#x}, which is not a PE image"));
        }
        let pe = u32::from_le_bytes(dos[0x3C..0x40].try_into().unwrap()) as usize;
        let sig = remote_read(h, base + pe, 4)?;
        if sig != b"PE\0\0" {
            return Err(format!("LoadLibraryW returned {base:#x}, which has no PE signature"));
        }
        log(&format!("[+] LoadLibraryW returned base {base:#x}"));
        Ok(base)
    })();

    let _ = remote_free(h, stub_addr);
    let _ = remote_free(h, out_addr);
    match &result {
        Ok(_) => {
            let _ = remote_free(h, path_addr);
        }
        Err(_) => log("[!] Leaving the path buffer allocated (the loader may still be using it)"),
    }
    let base = result?;

    // Verification only: a self-unlinking DLL is still a successful load.
    let name = std::path::Path::new(dll_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if let Ok(fresh) = remote_modules(h, arch) {
        if find_module(&fresh, &name).is_none() {
            log(&format!(
                "[!] '{name}' unlinked itself from the PEB; verified by its base address instead"
            ));
        }
    }
    Ok(base)
}

pub fn inject(h: HANDLE, arch: Arch, dll_path: &str, opts: &InjectOptions, log: Logger) -> Result<usize, String> {
    load_library_capture(h, arch, dll_path, log, |start| {
        create_remote_thread(h, arch, start, 0, opts.effective_hide_thread())
    })
}
