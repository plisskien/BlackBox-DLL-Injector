//! Method 2: Manual Map - in-memory PE loader and relocator.
//!
//! The image is mapped, relocated and import-resolved locally, then written to the target
//! in one shot. Initialisation then follows the loader's order:
//!   1. per-section protections are applied (code is never left writable while it runs)
//!   2. x64 unwind data is registered so SEH / C++ exceptions survive
//!   3. the TLS directory is materialised (index + per-thread block) and TLS callbacks run
//!   4. DllMain runs; a FALSE return is treated as a failed load
//!
//! All PE reads are bounds-checked: a malformed file must yield an error, never a panic.

use super::*;
use crate::pe::parse::{self, PeImage};

/// How long a single remote initialisation call may take. DLLs with slow DllMain code
/// (licence checks, unpacking) are legitimate; the old 5 s limit caused use-after-free.
const INIT_TIMEOUT_MS: u32 = 60_000;

struct RemoteTls {
    index: usize,
    /// True when the loader itself owns this module's TLS block (helper-module path): every
    /// current and future thread gets a block, so no manual array replacement is done.
    loader_managed: bool,
    /// Initialised bytes of one thread block (the rest is zero-filled by VirtualAllocEx).
    template: Vec<u8>,
    size: usize,
    callbacks: Vec<usize>,
    /// True when the target's threads already have a loader TLS array (so the call thread's
    /// must be allowed to appear before we grow it).
    expect_array: bool,
    /// GetProcessHeap() handle and RtlAllocateHeap address in the target. Arrays are
    /// allocated on the target's heap because the loader frees them with the thread.
    heap: usize,
    rtl_alloc: usize,
    /// TerminateThread, used to end a TLS-initialised call thread without letting the
    /// loader free TLS state it does not own.
    terminate: usize,
}

pub fn inject(h: HANDLE, arch: Arch, dll_path: &str, image: &[u8], opts: &InjectOptions, log: Logger) -> Result<usize, String> {
    let pe = PeImage::parse(image).map_err(|e| format!("{dll_path}: {e}"))?;
    parse::validate_image(image, &pe).map_err(|e| format!("{dll_path}: {e}"))?;
    // 32-bit manual map cannot provide exception support: the WOW64 dispatcher validates
    // handlers against loader-registered images and a mapped image is not one. Refuse such
    // an image up front instead of letting its first throw terminate the target.
    if arch == Arch::X86 {
        if let Some((rva, size)) = parse::data_dir(image, &pe, 3) {
            if rva != 0 && size != 0 {
                return Err(format!(
                    "{dll_path}: this 32-bit DLL carries exception data (.pdata); manual map on WOW64 \
                     cannot unwind exceptions inside it - use Standard, LdrLoadDll or NtCreateThreadEx"
                ));
            }
        }
    }
    let size_of_image = pe.size_of_image as usize;

    // --- Step A: allocate remote space ---
    let base = remote_alloc(h, size_of_image, PAGE_EXECUTE_READWRITE)?;
    log(&format!("[*] Allocated {:#x} bytes at {:#x}", size_of_image, base));

    // Set when a remote call timed out: a thread may still be executing the image, so it
    // must not be freed in that case.
    let mut unsafe_to_free = false;

    let result = (|| -> Result<usize, String> {
        // --- Local image assembly: headers + sections at their virtual offsets ---
        let delta = base as i64 - pe.image_base as i64;
        let mut mapped = vec![0u8; size_of_image];
        let soh = pe.size_of_headers as usize;
        if soh > image.len() || soh > mapped.len() {
            return Err("SizeOfHeaders does not fit the image".into());
        }
        mapped[..soh].copy_from_slice(&image[..soh]);
        for s in &pe.sections {
            if s.size_of_raw_data == 0 {
                continue;
            }
            let take = (s.virtual_size as usize).min(s.size_of_raw_data as usize);
            if take == 0 {
                continue;
            }
            let src = s.pointer_to_raw_data as usize;
            let dst = s.virtual_address as usize;
            if src + take > image.len() {
                return Err(format!(
                    "section '{}' raw data is outside the file",
                    s.name_str()
                ));
            }
            if dst + take > mapped.len() {
                return Err(format!(
                    "section '{}' does not fit in SizeOfImage",
                    s.name_str()
                ));
            }
            mapped[dst..dst + take].copy_from_slice(&image[src..src + take]);
        }

        // SEH/CFG validation suppression: zero the LOAD_CONFIG directory
        if opts.disable_seh_validation {
            if let Some((rva, size)) = parse::data_dir(image, &pe, 10) {
                if rva > 0 && size > 0 {
                    let start = rva as usize;
                    let end = (start + size as usize).min(mapped.len());
                    if start <= end {
                        for b in &mut mapped[start..end] {
                            *b = 0;
                        }
                        log("[*] SEH/CFG validation metadata zeroed");
                    }
                }
            }
        }

        // Disable exception support: drop the exception directory (x64 .pdata / x86 SEH)
        if opts.disable_exception_support {
            if let Some((rva, size)) = parse::data_dir(image, &pe, 3) {
                if rva > 0 && size > 0 {
                    let start = rva as usize;
                    let end = (start + size as usize).min(mapped.len());
                    if start <= end {
                        for b in &mut mapped[start..end] {
                            *b = 0;
                        }
                        log("[*] Exception directory zeroed (exception support disabled)");
                    }
                }
            }
        }

        // --- Step C: base relocations ---
        if delta != 0 {
            let applied = parse::apply_relocations(&mut mapped, &pe, delta)?;
            log(&format!("[*] Applied {} relocations (delta {:#x})", applied, delta));
        }

        // --- Step D: import resolution ---
        let mods = remote_modules(h, arch)?;
        let resolved = resolve_imports(h, arch, image, &pe, &mods, &mut mapped, opts, log)?;
        log(&format!("[*] Resolved {} imports", resolved));

        // --- Step B (batched): write the assembled image ---
        remote_write(h, base, &mapped)?;

        // --- Step E: section protections (before any of the image runs) ---
        apply_protections(h, &pe, base, log);

        // --- Step F: register unwind data so SEH/C++ exceptions can unwind ---
        if arch == Arch::X64 && !opts.disable_exception_support {
            register_unwind_info(h, arch, image, &pe, base, log, &mut unsafe_to_free)?;
        }

        // --- Step G: TLS directory + TLS callbacks ---
        let tls = setup_tls(h, arch, base, &mapped, &pe, opts.effective_hide_thread(), log, &mut unsafe_to_free)?;

        // --- Step H: DllMain (skipped for images without an entry point) ---
        if pe.address_of_entry_point == 0 {
            log("[*] Image has no entry point - DllMain not called (resource-only DLL)");
        } else {
            let entry = base + pe.address_of_entry_point as usize;
            log(&format!("[*] Calling entry point @ {:#x}", entry));
            let code = call_remote(
                h,
                arch,
                entry,
                base,
                1, // DLL_PROCESS_ATTACH
                0,
                tls.as_ref(),
                opts.effective_hide_thread(),
                log,
                &mut unsafe_to_free,
            )?;
            if code == 0 {
                return Err("DllMain returned FALSE (the loader would reject this module)".into());
            }
            log("[*] DllMain returned TRUE");
        }

        Ok(base)
    })();

    if let Err(e) = &result {
        if unsafe_to_free {
            log("[!] A remote call timed out while running the image - keeping it mapped");
        } else {
            let _ = remote_free(h, base);
            log("[!] Remote allocation freed after failure");
        }
        return Err(e.clone());
    }
    Ok(base)
}

/// Apply each section's final memory protection. Without this the whole image stays RWX,
/// which is both a detection signal and weaker than what the loader would do.
fn apply_protections(h: HANDLE, pe: &PeImage, base: usize, log: Logger) {
    for s in &pe.sections {
        let exec = s.characteristics & 0x2000_0000 != 0; // IMAGE_SCN_MEM_EXECUTE
        let write = s.characteristics & 0x8000_0000 != 0; // IMAGE_SCN_MEM_WRITE
        let read = s.characteristics & 0x4000_0000 != 0; // IMAGE_SCN_MEM_READ
        let prot = if exec {
            if write { PAGE_EXECUTE_READWRITE } else { PAGE_EXECUTE_READ }
        } else if write {
            PAGE_READWRITE
        } else if read {
            PAGE_READONLY
        } else {
            PAGE_NOACCESS
        };
        let va = base + s.virtual_address as usize;
        let sz = (s.virtual_size as usize).max(1);
        unsafe {
            let mut old = 0u32;
            if kernel32::VirtualProtectEx(h, va as _, sz, prot, &mut old) == 0 {
                log(&format!(
                    "[!] VirtualProtectEx({:#x}, {:#x}) failed: {}",
                    va,
                    sz,
                    crate::util::last_error("")
                ));
            }
        }
    }
    // Headers end up read-only like a normal image.
    let soh = pe.size_of_headers as usize;
    if soh > 0 {
        unsafe {
            let mut old = 0u32;
            let _ = kernel32::VirtualProtectEx(h, base as _, soh, PAGE_READONLY, &mut old);
        }
    }
}

/// x64: add the image's .pdata to the process' dynamic function table so the unwinder can
/// find handlers in a manually mapped module (SEH and C++ exceptions).
fn register_unwind_info(
    h: HANDLE,
    arch: Arch,
    image: &[u8],
    pe: &PeImage,
    base: usize,
    log: Logger,
    unsafe_to_free: &mut bool,
) -> Result<(), String> {
    let Some((rva, size)) = parse::data_dir(image, pe, 3) else {
        return Ok(());
    };
    if rva == 0 || size < 12 {
        return Ok(());
    }
    let count = size / 12; // RUNTIME_FUNCTION (x64) is 12 bytes
    if count > 1_000_000 {
        return Ok(()); // implausible; validation already rejected bad images
    }
    let mods = remote_modules(h, arch)?;
    let Some(ntdll_mod) = find_module(&mods, "ntdll.dll") else {
        log("[!] ntdll not found - exception support not registered");
        return Ok(());
    };
    let add = match remote_export(h, arch, ntdll_mod.base, "RtlAddFunctionTable", 0) {
        Ok(a) => a,
        Err(e) => {
            log(&format!("[!] RtlAddFunctionTable unavailable: {e}"));
            return Ok(());
        }
    };
    let ok = call_remote(
        h,
        arch,
        add,
        base + rva as usize,
        count as usize,
        base,
        None,
        false,
        log,
        unsafe_to_free,
    )?;
    if ok & 0xFF == 0 {
        log("[!] RtlAddFunctionTable failed - SEH inside this module will not unwind");
    } else {
        log(&format!(
            "[*] Registered {} unwind entries (SEH enabled)",
            count
        ));
    }
    // The MSVC C++ runtime resolves FuncInfo RVAs with RtlPcToFileHeader, which only reads
    // ntdll's *inverted function table* - and RtlAddFunctionTable does not touch that table.
    // Add an entry for the mapped image (x64 layout: count@+0, capacity@+4, entries@+0x10,
    // 24 bytes each {functionTable, imageBase, sizeOfImage, reserved}; entries 1.. are sorted
    // by base, entry 0 is the loader's hot-path cache).
    if let Err(e) = register_inverted_table(h, arch, base, base + rva as usize, size, pe.size_of_image, ntdll_mod.base, log) {
        log(&format!("[!] Inverted function table: {e}"));
    }
    Ok(())
}

/// Insert the mapped image into `ntdll!KiUserInvertedFunctionTable` (exported data symbol).
fn register_inverted_table(
    h: HANDLE,
    arch: Arch,
    base: usize,
    pdata_va: usize,
    pdata_size: u32,
    size_of_image: u32,
    ntdll_base: usize,
    log: Logger,
) -> Result<(), String> {
    if arch != Arch::X64 {
        // The C++ EH runtime on x86 uses the FS:[0] chain; leave the table alone.
        return Ok(());
    }
    let table = match remote_export(h, arch, ntdll_base, "KiUserInvertedFunctionTable", 0) {
        Ok(p) => p,
        Err(e) => return Err(format!("KiUserInvertedFunctionTable not found: {e}")),
    };
    let hdr = remote_read(h, table, 16)?;
    let count = u32::from_le_bytes(hdr[0..4].try_into().unwrap()) as usize;
    let max = u32::from_le_bytes(hdr[4..8].try_into().unwrap()) as usize;
    if count == 0 || count >= max || max > 4096 {
        return Err(format!("unexpected table state (count {count}, max {max})"));
    }
    let entries = table + 0x10;
    // Read the sorted part (indices 1..count-1).
    let tail_len = count.saturating_sub(1) * 24;
    let tail = remote_read(h, entries + 24, tail_len)?;
    let mut pos = count;
    for i in 1..count {
        let off = (i - 1) * 24;
        let b = u64::from_le_bytes(tail[off + 8..off + 16].try_into().unwrap()) as usize;
        if base < b {
            pos = i;
            break;
        }
    }
    let mut buf: Vec<u8> = Vec::with_capacity((count - pos + 1) * 24);
    buf.extend_from_slice(&(pdata_va as u64).to_le_bytes());
    buf.extend_from_slice(&(base as u64).to_le_bytes());
    buf.extend_from_slice(&size_of_image.to_le_bytes());
    // The loader stores the exception directory size here; the lookup fast path uses it.
    buf.extend_from_slice(&pdata_size.to_le_bytes());
    if pos < count {
        buf.extend_from_slice(&tail[(pos - 1) * 24..]);
    }
    let region = 0x10 + max * 24;
    unsafe {
        let mut old = 0u32;
        // The table lives on a read-only ntdll page; the loader flips it when it inserts.
        if kernel32::VirtualProtectEx(h, table as _, region, PAGE_READWRITE, &mut old) == 0 {
            return Err(crate::util::last_error("VirtualProtectEx(inverted table)"));
        }
        let result = (|| -> Result<(), String> {
            remote_write(h, entries + pos * 24, &buf)?;
            remote_write(h, table, &((count + 1) as u32).to_le_bytes())
        })();
        let mut restore = 0u32;
        let _ = kernel32::VirtualProtectEx(h, table as _, region, old, &mut restore);
        result?;
    }
    log(&format!(
        "[*] Image registered for RtlPcToFileHeader (inverted table index {pos}, {count} -> {})",
        count + 1
    ));
    Ok(())
}

/// Materialise the image's TLS directory: allocate a TLS index, give every live thread a
/// block copied from the template, patch AddressOfIndex and collect the callbacks.
fn setup_tls(
    h: HANDLE,
    arch: Arch,
    base: usize,
    mapped: &[u8],
    pe: &PeImage,
    hide_threads: bool,
    log: Logger,
    unsafe_to_free: &mut bool,
) -> Result<Option<RemoteTls>, String> {
    let (tls_rva, tls_size) = match parse::data_dir(mapped, pe, 9) {
        Some(v) if v.0 != 0 && v.1 >= 24 => v,
        _ => return Ok(None),
    };
    let Some(dir) = parse::tls_dir_at(mapped, tls_rva as usize, pe.pe32) else {
        return Ok(None);
    };
    let _ = tls_size;
    if arch == Arch::X86 {
        // The WOW64 TLS array cannot be resized safely from outside the process: replacing
        // it breaks the 32-bit loader's per-thread startup (verified: the target faults).
        // Refuse instead of loading a module whose TLS would read another module's slot.
        return Err(
            "manual map on a 32-bit target cannot initialise static TLS - use Standard, \
             LdrLoadDll or NtCreateThreadEx for a DLL that uses thread-local storage"
                .into(),
        );
    }
    if dir.end_address < dir.start_address {
        return Err("TLS directory has EndAddressOfRawData < StartAddressOfRawData".into());
    }
    let raw_len = (dir.end_address - dir.start_address) as usize;
    let total = raw_len
        .checked_add(dir.size_of_zero_fill as usize)
        .ok_or("TLS size overflow")?;
    if total > 0x0100_0000 {
        return Err(format!("TLS block of {total:#x} bytes is implausible"));
    }
    let start_rva = dir.start_address.wrapping_sub(base as u64) as usize;
    let mut template = Vec::new();
    if raw_len > 0 {
        if start_rva + raw_len > mapped.len() {
            return Err("TLS template is outside the image".into());
        }
        template.extend_from_slice(&mapped[start_rva..start_rva + raw_len]);
    }
    let size = ((total + 15) & !15).max(16);

    // Callbacks (absolute VAs after relocation -> RVAs into `mapped`).
    let mut callbacks = Vec::new();
    if dir.address_of_callbacks != 0 {
        let cb_rva = dir.address_of_callbacks.wrapping_sub(base as u64) as usize;
        let ptr = if pe.pe32 { 4 } else { 8 };
        let mut off = cb_rva;
        for _ in 0..64 {
            if off + ptr > mapped.len() {
                break;
            }
            let v = if pe.pe32 {
                u32::from_le_bytes(mapped[off..off + 4].try_into().unwrap()) as u64
            } else {
                u64::from_le_bytes(mapped[off..off + 8].try_into().unwrap())
            };
            if v == 0 {
                break;
            }
            callbacks.push(v as usize);
            off += ptr;
        }
    }

    // Nothing to materialise when the image has neither TLS data nor callbacks.
    if total == 0 && callbacks.is_empty() {
        return Ok(None);
    }

    // Preferred path (x64): let the loader own the index and the per-thread blocks. A real,
    // minimal MSVC-built DLL (embedded as `assets/tls_seed.dll`) carries a 0x2000-byte static
    // TLS template; we patch that template with the payload's bytes and LoadLibrary it in the
    // target. The loader assigns a genuine `_tls_index` and fills a block for every thread,
    // including threads that already exist (verified with a probe) and every future one. That
    // removes all manual thread-array surgery - which raced with the game's own thread churn
    // and corrupted its heap. A synthetic PE does not work here (the loader ignores its TLS
    // directory), which is why a real DLL is embedded.
    if arch == Arch::X64 {
        match install_tls_seed(h, arch, &template, log) {
            Ok((idx, seed_base)) => {
                if dir.address_of_index != 0 {
                    remote_write(h, dir.address_of_index as usize, &idx.to_le_bytes())?;
                }
                log(&format!(
                    "[*] TLS: index {idx} (loader-managed via seed module at {seed_base:#x}, {raw_len:#x} template + {:#x} zero-fill)",
                    dir.size_of_zero_fill
                ));
                // Blocks are loader-owned for existing and future threads; no array surgery.
                let t = RemoteTls {
                    index: idx as usize,
                    loader_managed: true,
                    template: Vec::new(),
                    size: 0,
                    callbacks,
                    expect_array: false,
                    heap: 0,
                    rtl_alloc: 0,
                    terminate: 0,
                };
                for (i, cb) in t.callbacks.iter().enumerate() {
                    log(&format!("[*] Running TLS callback {} @ {:#x}", i + 1, cb));
                    call_remote(h, arch, *cb, base, 1, 0, Some(&t), hide_threads, log, unsafe_to_free)?;
                }
                return Ok(Some(t));
            }
            Err(e) => log(&format!(
                "[!] TLS seed module unavailable ({e}); falling back to manual TLS seeding"
            )),
        }
    }

    // Pick a free *static* TLS index. The loader assigns one per module that has a TLS
    // directory, and `__declspec(thread)` indexes its ThreadLocalStoragePointer array with
    // it. The Win32 TlsAlloc slots are a different array and cannot be used here.
    let pid = crate::process::pid_of(h)?;
    let mods = remote_modules(h, arch)?;
    // The index the loader would hand to the next TLS module is the number of loaded
    // modules that already have one.
    let counted = count_tls_modules(h, arch, &mods);
    let mut threads: Vec<(u32, usize, usize)> = Vec::new(); // (tid, teb, array)
    for tid in target_thread_ids(pid) {
        if let Ok(teb) = thread_teb(h, tid, arch) {
            threads.push((tid, teb, read_tls_array(h, arch, teb).unwrap_or(0)));
        }
    }
    if threads.is_empty() {
        return Err("cannot enumerate target threads for TLS setup".into());
    }
    // `count_tls_modules` mirrors the loader's own module-index assignment. As a safety net
    // against an under-count, never pick an index that an existing thread's array already
    // uses (read only up to the counted index - past the loader's array is heap garbage).
    let observed = threads
        .iter()
        .filter(|(_, _, a)| *a != 0)
        .map(|(_, _, a)| tls_array_used(h, arch, *a, counted.min(MAX_STATIC_TLS)))
        .max()
        .unwrap_or(0);
    let index = counted.max(observed);
    if index >= MAX_STATIC_TLS {
        return Err(format!(
            "static TLS index {index} exceeds the {MAX_STATIC_TLS}-slot limit"
        ));
    }
    let array_entries = index + 1;
    let expect_array = threads.iter().any(|(_, _, a)| *a != 0);
    if dir.address_of_index != 0 {
        remote_write(h, dir.address_of_index as usize, &(index as u32).to_le_bytes())?;
    }
    // The TLS array is owned by the loader and freed with the thread, so replacements have
    // to come from the target's own heap (RtlAllocateHeap), never from VirtualAllocEx.
    let kernel = find_module(&mods, "kernel32.dll").ok_or("kernel32.dll not found in target")?;
    let get_heap = remote_export(h, arch, kernel.base, "GetProcessHeap", 0)?;
    let heap = call_remote_ptr(h, arch, get_heap, 0, 0, 0, log)?;
    if heap == 0 {
        return Err("GetProcessHeap failed in the target".into());
    }
    let ntdll_mod = find_module(&mods, "ntdll.dll").ok_or("ntdll.dll not found in target")?;
    let rtl_alloc = remote_export(h, arch, ntdll_mod.base, "RtlAllocateHeap", 0)?;
    let terminate = remote_export(h, arch, kernel.base, "TerminateThread", 0)?;
    let array_size = array_entries * std::mem::size_of::<usize>();
    let mut threaded = 0usize;
    // Seed every live thread; retry transient failures once, then fail the injection instead of
    // leaving a thread without the slot (the payload would corrupt memory on that thread).
    let mut done: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut failures: Vec<(u32, String)> = Vec::new();
    for pass in 0..2 {
        failures.clear();
        for (tid, teb, _) in &threads {
            if done.contains(tid) {
                continue;
            }
            let outcome = (|| -> Result<(), String> {
                let block = alloc_tls_block(h, arch, heap, rtl_alloc, &template, size, log)?;
                let array = call_remote_ptr(h, arch, rtl_alloc, heap, 0, array_size, log)?;
                if array == 0 {
                    return Err("RtlAllocateHeap returned 0 for a TLS array".into());
                }
                set_static_tls_slot(h, arch, *teb, index, block, array, array_entries)
            })();
            match outcome {
                Ok(()) => {
                    done.insert(*tid);
                    threaded += 1;
                }
                Err(e) => failures.push((*tid, e)),
            }
        }
        if failures.is_empty() {
            break;
        }
        if pass == 0 {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    if !failures.is_empty() {
        let sample: Vec<String> = failures
            .iter()
            .take(3)
            .map(|(t, e)| format!("tid {t}: {e}"))
            .collect();
        return Err(format!(
            "could not initialise static TLS for {}/{} target threads ({})",
            failures.len(),
            threads.len(),
            sample.join("; ")
        ));
    }
    log(&format!(
        "[*] TLS: index {index}, {raw_len:#x} template bytes + {:#x} zero-fill, {} thread block(s)",
        dir.size_of_zero_fill, threaded
    ));

    // Run the TLS callbacks before DllMain, like the loader.
    let tls = RemoteTls { index, loader_managed: false, template, size, callbacks, expect_array, heap, rtl_alloc, terminate };
    for (i, cb) in tls.callbacks.iter().enumerate() {
        log(&format!("[*] Running TLS callback {} @ {:#x}", i + 1, cb));
        call_remote(h, arch, *cb, base, 1, 0, Some(&tls), hide_threads, log, unsafe_to_free)?;
    }
    Ok(Some(tls))
}

fn alloc_tls_block(
    h: HANDLE,
    arch: Arch,
    heap: usize,
    rtl_alloc: usize,
    template: &[u8],
    size: usize,
    log: Logger,
) -> Result<usize, String> {
    // The loader frees TLS blocks/arrays with the thread, so they must live on the heap.
    let p = call_remote_ptr(h, arch, rtl_alloc, heap, 0, size, log)?;
    if p == 0 {
        return Err("RtlAllocateHeap failed for a TLS block".into());
    }
    if !template.is_empty() {
        remote_write(h, p, template)?;
    }
    Ok(p)
}
// ===== loader-managed static TLS via a real embedded seed DLL =====
//
// A synthetic PE does not work: the loader accepts the module but never writes its
// `_tls_index` nor copies its template into per-thread blocks (verified with a probe). A real
// MSVC-built DLL does get both. `assets/tls_seed.dll` is such a DLL with a 0x2000-byte static
// TLS template; its DllMain only reads the template, so patched bytes stay intact.

const TLS_SEED_DLL: &[u8] = include_bytes!("../../assets/tls_seed.dll");

fn seed_u16(d: &[u8], o: usize) -> usize {
    u16::from_le_bytes(d[o..o + 2].try_into().unwrap()) as usize
}
fn seed_u32(d: &[u8], o: usize) -> usize {
    u32::from_le_bytes(d[o..o + 4].try_into().unwrap()) as usize
}
fn seed_u64(d: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(d[o..o + 8].try_into().unwrap())
}

fn pe_rva_to_off(data: &[u8], rva: usize) -> Option<usize> {
    let pe = seed_u32(data, 0x3C);
    if pe + 0x18 > data.len() {
        return None;
    }
    let opt = pe + 24;
    let opt_size = seed_u16(data, pe + 20);
    let num = seed_u16(data, pe + 6);
    let sec = opt + opt_size;
    for i in 0..num {
        let s = sec + i * 40;
        if s + 40 > data.len() {
            return None;
        }
        let vsize = seed_u32(data, s + 8);
        let va = seed_u32(data, s + 12);
        let raw_size = seed_u32(data, s + 16);
        let raw_ptr = seed_u32(data, s + 20);
        if rva >= va && rva < va + vsize.max(raw_size) {
            let d = rva - va;
            if d >= raw_size {
                return None;
            }
            return Some(raw_ptr + d);
        }
    }
    None
}

/// (file offset, size) of the seed DLL's TLS template inside the embedded file.
fn seed_template_span() -> Result<(usize, usize), String> {
    let d = TLS_SEED_DLL;
    if d.len() < 0x100 || &d[0..2] != b"MZ" {
        return Err("embedded TLS seed is not a PE file".into());
    }
    let pe = seed_u32(d, 0x3C);
    let opt = pe + 24;
    let dd = opt + 112;
    if dd + 9 * 8 + 8 > d.len() {
        return Err("embedded TLS seed has no data directories".into());
    }
    let tls_rva = seed_u32(d, dd + 9 * 8);
    if tls_rva == 0 {
        return Err("embedded TLS seed has no TLS directory".into());
    }
    let tls_off = pe_rva_to_off(d, tls_rva).ok_or("TLS directory RVA is unmapped")?;
    if tls_off + 0x28 > d.len() {
        return Err("TLS directory is truncated".into());
    }
    let image_base = seed_u64(d, opt + 24) as usize;
    let start = seed_u64(d, tls_off) as usize;
    let end = seed_u64(d, tls_off + 8) as usize;
    if end <= start || image_base == 0 || start < image_base || end < image_base {
        return Err("embedded TLS seed has an invalid template range".into());
    }
    let off = pe_rva_to_off(d, start - image_base).ok_or("TLS template RVA is unmapped")?;
    Ok((off, end - start))
}

/// Patch the embedded seed with the payload's TLS template, load it in the target and return
/// (loader-assigned `_tls_index`, module base).
fn install_tls_seed(
    h: HANDLE,
    arch: Arch,
    template: &[u8],
    log: Logger,
) -> Result<(u32, usize), String> {
    if arch != Arch::X64 {
        return Err("only x64 has a loader-managed seed".into());
    }
    let (off, size) = seed_template_span()?;
    if template.len() > size {
        return Err(format!(
            "payload TLS template {:#x} exceeds the seed's {size:#x} bytes",
            template.len()
        ));
    }
    let mut bytes = TLS_SEED_DLL.to_vec();
    bytes[off..off + template.len()].copy_from_slice(template);
    for b in &mut bytes[off + template.len()..off + size] {
        *b = 0;
    }
    let mods = remote_modules(h, arch)?;
    let kernel = find_module(&mods, "kernel32.dll").ok_or("kernel32.dll not found in target")?;
    let loadlib = remote_export(h, arch, kernel.base, "LoadLibraryW", 0)?;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = format!(
        "{}\\bb_tlsseed_{}_{}.dll",
        std::env::temp_dir().display(),
        std::process::id(),
        unique
    );
    std::fs::write(&path, &bytes).map_err(|e| format!("cannot write TLS seed: {e}"))?;
    let name = std::path::Path::new(&path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let wpath = wstr(&path);
    let addr = remote_alloc(h, wpath.len() * 2, PAGE_READWRITE)?;
    remote_write(h, addr, &path_as_bytes(&wpath))?;
    let load = (|| -> Result<(), String> {
        let th = create_remote_thread(h, arch, loadlib, addr, false)?;
        let st = wait_thread_status(th, 15_000);
        let _ = close_handle(th);
        match st {
            ThreadWait::Done(_) => Ok(()),
            ThreadWait::TimedOut => Err("LoadLibraryW(TLS seed) timed out".into()),
            ThreadWait::Failed(e) => Err(e),
        }
    })();
    let _ = remote_free(h, addr);
    let _ = std::fs::remove_file(&path);
    load?;
    let fresh = remote_modules(h, arch)?;
    let m = find_module(&fresh, &name)
        .cloned()
        .ok_or("TLS seed did not appear in the module list")?;
    let index = (|| -> Result<u32, String> {
        let dos = remote_read(h, m.base, 0x40)?;
        let pe = u32::from_le_bytes(dos[0x3C..0x40].try_into().unwrap()) as usize;
        let nt = remote_read(h, m.base + pe, 0x18 + 0xF0)?;
        let dd = 0x18 + 112;
        let tls_rva = u32::from_le_bytes(nt[dd + 9 * 8..dd + 9 * 8 + 4].try_into().unwrap()) as usize;
        if tls_rva == 0 {
            return Err("seed has no TLS directory in the target".into());
        }
        let td = remote_read(h, m.base + tls_rva, 0x28)?;
        let aoi = u64::from_le_bytes(td[0x10..0x18].try_into().unwrap()) as usize;
        if aoi == 0 {
            return Err("seed AddressOfIndex is null".into());
        }
        remote_read_u32(h, aoi)
    })();
    match index {
        Ok(v) if (v as usize) < MAX_STATIC_TLS => {
            log(&format!("[+] Loaded TLS seed {} @ {:#x} (index {v})", name, m.base));
            Ok((v, m.base))
        }
        Ok(v) => Err(format!("seed TLS index {v} is out of range")),
        Err(e) => Err(format!("seed TLS index unreadable: {e}")),
    }
}

/// The index the loader would hand to the next TLS module: `max(_tls_index) + 1`, read from
/// each loaded module's own TLS directory (`AddressOfIndex`). Counting modules that merely have
/// a TLS directory can lag the loader (a directory may exist before its index is assigned, or an
/// index may be assigned before the module is visible in the PEB), which would pick a slot
/// already owned by another module and corrupt that module's thread-local data.
fn count_tls_modules(h: HANDLE, arch: Arch, mods: &[RemoteModule]) -> usize {
    let pe32 = arch == Arch::X86;
    let mut max_index: Option<usize> = None;
    for m in mods {
        let idx = (|| -> Result<Option<usize>, String> {
            let dos = remote_read(h, m.base, 0x40)?;
            if dos[0] != b'M' || dos[1] != b'Z' {
                return Ok(None);
            }
            let pe_off = u32::from_le_bytes(dos[0x3C..0x40].try_into().unwrap()) as usize;
            let nt = remote_read(h, m.base + pe_off, 0x18 + if pe32 { 0xE0 } else { 0xF0 })?;
            let data_dir_off = 0x18 + if pe32 { 96 } else { 112 };
            let tls_rva = u32::from_le_bytes(
                nt[data_dir_off + 9 * 8..data_dir_off + 9 * 8 + 4]
                    .try_into()
                    .unwrap(),
            ) as usize;
            if tls_rva == 0 {
                return Ok(None);
            }
            // IMAGE_TLS_DIRECTORY: AddressOfIndex at +0x0C (PE32) / +0x10 (PE32+).
            let td = remote_read(h, m.base + tls_rva, 0x28)?;
            let aoi = if pe32 {
                u32::from_le_bytes(td[0x0C..0x10].try_into().unwrap()) as usize
            } else {
                u64::from_le_bytes(td[0x10..0x18].try_into().unwrap()) as usize
            };
            if aoi == 0 {
                return Ok(None);
            }
            let v = if pe32 {
                remote_read_u32(h, aoi)? as usize
            } else {
                remote_read_u64(h, aoi)? as usize
            };
            // Ignore implausible values (uninitialised index storage) rather than failing the map.
            Ok(if v < MAX_STATIC_TLS { Some(v) } else { None })
        })()
        .unwrap_or(None);
        if let Some(v) = idx {
            max_index = Some(max_index.map_or(v, |m| m.max(v)));
        }
    }
    max_index.map(|v| v + 1).unwrap_or(0)
}

/// Run a call in a remote thread and return the full-width pointer result. The thread exit
/// code is only 32 bit, so the stub stores the value in a remote slot as well.
fn call_remote_ptr(
    h: HANDLE,
    arch: Arch,
    fn_addr: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    log: Logger,
) -> Result<usize, String> {
    let slot = remote_alloc(h, std::mem::size_of::<usize>(), PAGE_READWRITE)?;
    let stub = match arch {
        Arch::X64 => shellcode::call3_stub_result_x64(fn_addr, a1, a2, a3, slot),
        Arch::X86 => shellcode::call3_stub_result_x86(fn_addr, a1, a2, a3, slot),
    };
    let addr = remote_alloc(h, stub.len(), PAGE_EXECUTE_READWRITE)?;
    let r = (|| -> Result<usize, String> {
        remote_write(h, slot, &vec![0u8; std::mem::size_of::<usize>()])?;
        remote_write(h, addr, &stub)?;
        let th = create_remote_thread_ext(h, arch, addr, 0, false, false)?;
        let _g = HandleGuard(th);
        match wait_thread_status(th, INIT_TIMEOUT_MS) {
            ThreadWait::Done(_) => {}
            ThreadWait::TimedOut => {
                return Err(format!("remote call at {fn_addr:#x} did not finish"));
            }
            ThreadWait::Failed(e) => return Err(e),
        }
        match arch {
            Arch::X64 => Ok(remote_read_u64(h, slot)? as usize),
            Arch::X86 => Ok(remote_read_u32(h, slot)? as usize),
        }
    })();
    let _ = remote_free(h, slot);
    let _ = remote_free(h, addr);
    let _ = log;
    r
}

/// Run `fn(a1, a2, a3)` in a remote thread and return its exit code (the callee's return
/// value). When a TLS block is needed the thread is started with a gated stub, so the TLS
/// slot can be installed after the loader's thread-start code but before the call.
fn call_remote(
    h: HANDLE,
    arch: Arch,
    fn_addr: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    tls: Option<&RemoteTls>,
    hide: bool,
    log: Logger,
    unsafe_to_free: &mut bool,
) -> Result<u32, String> {
    // Loader-managed TLS needs no gate/result-slot choreography: the call thread is created
    // after the helper module is loaded, so the loader itself gives it a TLS block.
    let tls = tls.filter(|t| !t.loader_managed);
    // The gate needs its own address at build time, so it gets its own one-byte allocation.
    let gate = if tls.is_some() {
        Some(remote_alloc(h, 1, PAGE_READWRITE)?)
    } else {
        None
    };
    let slot = if tls.is_some() {
        Some(remote_alloc(h, std::mem::size_of::<usize>(), PAGE_READWRITE)?)
    } else {
        None
    };
    let stub = match (arch, gate, slot) {
        (Arch::X64, Some(g), Some(s)) => {
            let t = tls.unwrap();
            shellcode::call3_gated_result_term_x64(fn_addr, a1, a2, a3, g, s, t.terminate)
        }
        (Arch::X86, Some(g), Some(s)) => {
            let t = tls.unwrap();
            shellcode::call3_gated_result_term_x86(fn_addr, a1, a2, a3, g, s, t.terminate)
        }
        (Arch::X64, None, None) => shellcode::call3_stub_x64(fn_addr, a1, a2, a3),
        (Arch::X86, None, None) => shellcode::call3_stub_x86(fn_addr, a1, a2, a3),
        _ => return Err("internal: gate/result slot mismatch".into()),
    };
    let addr = remote_alloc(h, stub.len(), PAGE_EXECUTE_READWRITE)?;
    let r = (|| -> Result<u32, String> {
        remote_write(h, addr, &stub)?;
        if let Some(g) = gate {
            remote_write(h, g, &[0u8])?;
        }
        if let Some(s) = slot {
            remote_write(h, s, &vec![0u8; std::mem::size_of::<usize>()])?;
        }
        let th = create_remote_thread_ext(h, arch, addr, 0, hide, false)?;
        let _g = HandleGuard(th);
        if let (Some(t), Some(g)) = (tls, gate) {
            let tid = unsafe { kernel32::GetThreadId(th) };
            if tid == 0 {
                return Err(crate::util::last_error("GetThreadId"));
            }
            // The loader allocates the thread's TLS array while the thread starts; wait for
            // it (when the process has any) so the array we grow is the loader's own.
            if t.expect_array {
                let deadline =
                    std::time::Instant::now() + std::time::Duration::from_millis(1500);
                loop {
                    if let Ok(teb) = thread_teb(h, tid, arch) {
                        if let Ok(array) = read_tls_array(h, arch, teb) {
                            if array != 0 {
                                break;
                            }
                        }
                    }
                    if std::time::Instant::now() >= deadline {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            let teb = thread_teb(h, tid, arch)?;
            let block = alloc_tls_block(h, arch, t.heap, t.rtl_alloc, &t.template, t.size, log)?;
            let array_entries = t.index + 1;
            let array_size = array_entries * std::mem::size_of::<usize>();
            let array = call_remote_ptr(h, arch, t.rtl_alloc, t.heap, 0, array_size, log)?;
            if array == 0 {
                return Err("RtlAllocateHeap failed in the target".into());
            }
            set_static_tls_slot(h, arch, teb, t.index, block, array, array_entries)?;
            remote_write(h, g, &[1u8])?;
        }
        match wait_thread_status(th, INIT_TIMEOUT_MS) {
            ThreadWait::Done(code) => {
                if let Some(s) = slot {
                    // The thread terminated itself to skip the loader's TLS teardown, so the
                    // real return value comes from the result slot.
                    match arch {
                        Arch::X64 => Ok(remote_read_u64(h, s)? as u32),
                        Arch::X86 => Ok(remote_read_u32(h, s)?),
                    }
                } else {
                    Ok(code)
                }
            }
            ThreadWait::TimedOut => {
                *unsafe_to_free = true;
                Err(format!(
                    "remote call at {fn_addr:#x} is still running after {} s - the target may be stuck",
                    INIT_TIMEOUT_MS / 1000
                ))
            }
            ThreadWait::Failed(e) => Err(e),
        }
    })();
    if !*unsafe_to_free {
        let _ = remote_free(h, addr);
        if let Some(g) = gate {
            let _ = remote_free(h, g);
        }
        if let Some(s) = slot {
            let _ = remote_free(h, s);
        }
    } else {
        log("[!] Leaving a call stub allocated (its thread did not finish)");
    }
    r
}

/// Thread ids of a process (Toolhelp snapshot).
fn target_thread_ids(pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    unsafe {
        let snap = kernel32::CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut te = THREADENTRY32::default();
        te.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        if kernel32::Thread32First(snap, &mut te) != 0 {
            loop {
                if te.th32OwnerProcessID == pid {
                    out.push(te.th32ThreadID);
                }
                if kernel32::Thread32Next(snap, &mut te) == 0 {
                    break;
                }
            }
        }
        let _ = kernel32::CloseHandle(snap);
    }
    out
}

/// Populate IAT entries in the locally-mapped image.
fn resolve_imports(
    h: HANDLE,
    arch: Arch,
    image: &[u8],
    pe: &PeImage,
    mods: &[RemoteModule],
    mapped: &mut [u8],
    _opts: &InjectOptions,
    log: Logger,
) -> Result<usize, String> {
    let pe32 = arch == Arch::X86;
    let ptr_size: usize = if pe32 { 4 } else { 8 };
    let mut count = 0usize;
    let (imp_rva, imp_size) = match parse::data_dir(image, pe, 1) {
        Some(v) if v.0 != 0 => v,
        _ => return Ok(0),
    };
    let _ = imp_size;

    let mut desc_rva = imp_rva as usize;
    let max_descriptors = 1024usize;
    for _ in 0..max_descriptors {
        if desc_rva + 20 > mapped.len() {
            return Err("import descriptor table is outside the image".into());
        }
        let (original_first_thunk, name_rva, first_thunk) = {
            let d = &mapped[desc_rva..desc_rva + 20];
            (
                u32::from_le_bytes(d[0..4].try_into().unwrap()) as usize,
                u32::from_le_bytes(d[12..16].try_into().unwrap()) as usize,
                u32::from_le_bytes(d[16..20].try_into().unwrap()) as usize,
            )
        };
        if name_rva == 0 && first_thunk == 0 {
            return Ok(count); // terminator
        }
        if name_rva >= mapped.len() {
            return Err(format!("import descriptor name RVA {name_rva:#x} is out of range"));
        }
        let name_end = mapped[name_rva..]
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| format!("import descriptor name at {name_rva:#x} is not terminated"))?;
        let dep = String::from_utf8_lossy(&mapped[name_rva..name_rva + name_end]).to_string();
        if dep.is_empty() {
            return Err("import descriptor with an empty DLL name".into());
        }

        let dep_mod = ensure_remote_module(h, arch, mods, &dep, log)?;

        let mut i = 0usize;
        loop {
            let int_rva = if original_first_thunk != 0 { original_first_thunk } else { first_thunk }
                + i.saturating_mul(ptr_size);
            if int_rva + ptr_size > mapped.len() {
                return Err(format!("import thunk table of '{dep}' runs outside the image"));
            }
            let thunk_raw: u64 = if pe32 {
                u32::from_le_bytes(mapped[int_rva..int_rva + 4].try_into().unwrap()) as u64
            } else {
                u64::from_le_bytes(mapped[int_rva..int_rva + 8].try_into().unwrap())
            };
            if thunk_raw == 0 {
                break;
            }
            let ordinal = (thunk_raw & (if pe32 { 0x8000_0000u64 } else { 0x8000_0000_0000_0000 })) != 0;
            let func: usize = if ordinal {
                let ord = (thunk_raw & 0xFFFF) as u32;
                remote_export_ordinal(h, arch, dep_mod.base, ord)?
            } else {
                let hint_rva = (thunk_raw & 0xFFFF_FFFF) as usize;
                if hint_rva + 2 >= mapped.len() {
                    return Err(format!("import name RVA {hint_rva:#x} is out of range"));
                }
                let name_start = hint_rva + 2;
                let name_end = mapped[name_start..]
                    .iter()
                    .position(|&b| b == 0)
                    .ok_or_else(|| format!("import name at {name_start:#x} is not terminated"))?;
                let fname = String::from_utf8_lossy(&mapped[name_start..name_start + name_end]).to_string();
                // Always resolve against the *target's* export directory: the injector's
                // own process may hold the module at a different base (or not at all).
                remote_export(h, arch, dep_mod.base, &fname, 0)?
            };
            let iat_rva = first_thunk + i.saturating_mul(ptr_size);
            if iat_rva + ptr_size > mapped.len() {
                return Err("IAT entry is outside the image".into());
            }
            let bytes = if pe32 {
                (func as u32).to_le_bytes().to_vec()
            } else {
                (func as u64).to_le_bytes().to_vec()
            };
            mapped[iat_rva..iat_rva + ptr_size].copy_from_slice(&bytes);
            count += 1;
            i += 1;
            if i > 1_000_000 {
                return Err("import thunk table does not terminate".into());
            }
        }
        desc_rva += 20;
    }
    Err("import descriptor table does not terminate".into())
}

fn remote_export_ordinal(h: HANDLE, arch: Arch, module_base: usize, ordinal: u32) -> Result<usize, String> {
    // Export-by-ordinal: functions array indexed by (ordinal - Base).
    let dos = remote_read(h, module_base, 0x40)?;
    if dos[0] != b'M' || dos[1] != b'Z' {
        return Err("module is not a PE image".into());
    }
    let pe_off = u32::from_le_bytes(dos[0x3C..0x40].try_into().unwrap()) as usize;
    let nt = remote_read(h, module_base + pe_off, 0x18 + 0xE0)?;
    if &nt[0..4] != b"PE\0\0" {
        return Err("bad NT signature".into());
    }
    let pe32 = arch == Arch::X86;
    let data_dir_off = 0x18 + if pe32 { 96 } else { 112 };
    let exp_rva = u32::from_le_bytes(nt[data_dir_off..data_dir_off + 4].try_into().unwrap()) as usize;
    if exp_rva == 0 {
        return Err("module has no exports".into());
    }
    let ed = remote_read(h, module_base + exp_rva, 40)?;
    let base_ord = u32::from_le_bytes(ed[16..20].try_into().unwrap());
    let num_funcs = u32::from_le_bytes(ed[20..24].try_into().unwrap()) as usize;
    let addr_funcs = u32::from_le_bytes(ed[28..32].try_into().unwrap()) as usize;
    if num_funcs == 0 || num_funcs > 65_536 {
        return Err("export function table is implausible".into());
    }
    if ordinal < base_ord {
        return Err(format!("ordinal {ordinal} is below the export base {base_ord}"));
    }
    let idx = (ordinal - base_ord) as usize;
    if idx >= num_funcs {
        return Err(format!("ordinal {ordinal} out of range"));
    }
    let funcs_raw = remote_read(h, module_base + addr_funcs, num_funcs * 4)?;
    let rva = u32::from_le_bytes(funcs_raw[idx * 4..idx * 4 + 4].try_into().unwrap()) as usize;
    if rva == 0 {
        return Err(format!("ordinal {ordinal} resolves to null"));
    }
    Ok(module_base + rva)
}







