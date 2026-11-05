//! Method 0: Standard - CreateRemoteThread with remote LoadLibraryW.

use super::*;

/// The loader may legitimately spend a long time inside DllMain (unpacking, licence checks).
const LOAD_TIMEOUT_MS: u32 = 60_000;

pub fn inject(h: HANDLE, arch: Arch, dll_path: &str, opts: &InjectOptions, log: Logger) -> Result<usize, String> {
    let mods = remote_modules(h, arch)?;
    let kernel = find_module(&mods, "kernel32.dll").ok_or("kernel32.dll not found in target")?;
    let loadlib = remote_export(h, arch, kernel.base, "LoadLibraryW", 0)?;
    log(&format!("[*] Remote kernel32!LoadLibraryW @ {:#x}", loadlib));

    let path = wstr(dll_path);
    let path_addr = remote_alloc(h, path.len() * 2, PAGE_READWRITE)?;
    remote_write(h, path_addr, &path_as_bytes(&path))?;

    let result = (|| -> Result<(), String> {
        let th = create_remote_thread(h, arch, loadlib, path_addr, opts.effective_hide_thread())?;
        let _g = HandleGuard(th);
        let code = match wait_thread_status(th, LOAD_TIMEOUT_MS) {
            ThreadWait::Done(c) => c,
            ThreadWait::TimedOut => {
                return Err(format!(
                    "LoadLibraryW did not return within {} s (the DLL is blocking inside DllMain) - \
                     the target was left untouched",
                    LOAD_TIMEOUT_MS / 1000
                ))
            }
            ThreadWait::Failed(e) => return Err(e),
        };
        if code == 0 {
            return Err("LoadLibraryW returned NULL (module rejected or dependency missing)".into());
        }
        Ok(())
    })();

    match &result {
        // Only release the path once the loader call has provably returned.
        Ok(()) => {
            let _ = remote_free(h, path_addr);
        }
        Err(e) => {
            log("[!] Leaving the path buffer allocated (the loader may still be using it)");
            return Err(e.clone());
        }
    }

    // Find the newly registered module: the exit code is only a 32-bit status here, so the
    // 64-bit base has to come from the PEB list.
    let fresh = remote_modules(h, arch)?;
    let name = std::path::Path::new(dll_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    match find_module(&fresh, &name) {
        Some(m) => Ok(m.base),
        None => Err(format!(
            "LoadLibraryW succeeded but '{name}' is not in the target's module list"
        )),
    }
}
