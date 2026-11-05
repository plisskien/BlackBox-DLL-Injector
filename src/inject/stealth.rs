//! Module cloaking operations: ErasePE, PEB unlink (HideModule), ThreadHideFromDebugger.

use super::*;

/// Zeroes the PE headers of the module in the target process.
///
/// Loader-mapped modules keep their headers on a read-only image page, so the page has to be
/// made writable first (a plain WriteProcessMemory fails with ERROR_NOACCESS there). The
/// write is verified afterwards so a silent failure cannot be mistaken for success.
pub fn erase_pe(h: HANDLE, arch: Arch, base: usize) -> Result<(), String> {
    if base == 0 {
        return Err("module base is unknown".into());
    }
    let _ = arch;
    // SizeOfHeaders from the remote image's optional header.
    let pe_off = {
        let dos = remote_read(h, base, 0x40)?;
        if dos.len() < 0x40 || dos[0] != b'M' || dos[1] != b'Z' {
            return Err("cannot erase PE: module signature missing".into());
        }
        u32::from_le_bytes(dos[0x3C..0x40].try_into().unwrap()) as usize
    };
    let nt = remote_read(h, base + pe_off, 0x18 + 2)?;
    let opt_off = pe_off + 0x18;
    let magic = u16::from_le_bytes([nt[0x18], nt[0x19]]);
    let _ = magic;
    const SIZE_OF_HEADERS_IN_OPT: usize = 60; // same offset in PE32 and PE32+
    let opt = remote_read(h, base + opt_off, SIZE_OF_HEADERS_IN_OPT + 4)?;
    let size_of_headers =
        u32::from_le_bytes(opt[SIZE_OF_HEADERS_IN_OPT..SIZE_OF_HEADERS_IN_OPT + 4].try_into().unwrap()) as usize;
    if size_of_headers == 0 {
        return Err("SizeOfHeaders is zero".into());
    }
    if size_of_headers > 0x0100_0000 {
        return Err(format!("SizeOfHeaders {size_of_headers:#x} is implausible"));
    }
    unsafe {
        let mut old = 0u32;
        if kernel32::VirtualProtectEx(h, base as _, size_of_headers, PAGE_READWRITE, &mut old) == 0 {
            return Err(crate::util::last_error("VirtualProtectEx(headers)"));
        }
        let zeros = vec![0u8; size_of_headers];
        let write = remote_write(h, base, &zeros);
        let mut restored = 0u32;
        let _ = kernel32::VirtualProtectEx(h, base as _, size_of_headers, old, &mut restored);
        write?;
    }
    // Verify: the first bytes must read back as zero.
    let check = remote_read(h, base, 2)?;
    if check != [0u8, 0u8] {
        return Err("header write did not take effect".into());
    }
    Ok(())
}

/// Unlinks the module from all three PEB Ldr lists and blanks its name so name-based loader
/// lookups (`GetModuleHandle`, `LdrGetDllHandle`) no longer find it.
///
/// Note: the loader's hash table still owns the entry, so an anti-cheat that walks that table
/// directly can still see the module. This is a PEB-level cloak, not a loader re-write.
pub fn hide_module(h: HANDLE, arch: Arch, base: usize) -> Result<(), String> {
    if base == 0 {
        return Err("module base is unknown".into());
    }
    match arch {
        Arch::X64 => hide_module_x64(h, base),
        Arch::X86 => hide_module_x86(h, base),
    }
}

fn hide_module_x64(h: HANDLE, base: usize) -> Result<(), String> {
    unsafe {
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
        let ldr = remote_read_ptr(h, peb + 0x18, Arch::X64)?;
        if ldr == 0 {
            return Err("remote PEB->Ldr is null".into());
        }
        // (list-head offset in PEB_LDR_DATA, link-field offset within the entry)
        let lists = [(0x10usize, 0x00usize), (0x20, 0x10), (0x30, 0x20)];
        let mut unlinked = 0usize;
        for (head_off, field_off) in lists {
            let head = ldr + head_off;
            let mut cur = remote_read_u64(h, head)? as usize;
            let mut guard = 0;
            while cur != 0 && cur != head && guard < 4096 {
                guard += 1;
                let next = remote_read_u64(h, cur)? as usize;
                // LDR_DATA_TABLE_ENTRY64: links at +0/+0x10/+0x20, DllBase at +0x30,
                // FullDllName at +0x48, BaseDllName at +0x58.
                let entry = cur - field_off;
                let dll_base = remote_read_u64(h, entry + 0x30)? as usize;
                if dll_base == base {
                    unlink_x64(h, cur)?;
                    // Blank the name lengths so LdrpFindLoadedDllByName misses it.
                    let _ = remote_write(h, entry + 0x48, &[0u8; 4]);
                    let _ = remote_write(h, entry + 0x58, &[0u8; 4]);
                    unlinked += 1;
                }
                if next == cur {
                    break;
                }
                cur = next;
            }
        }
        if unlinked == 0 {
            return Err(format!("module {base:#x} not found in any PEB list"));
        }
    }
    Ok(())
}

fn unlink_x64(h: HANDLE, link: usize) -> Result<(), String> {
    let flink = remote_read_u64(h, link)? as usize;
    let blink = remote_read_u64(h, link + 8)? as usize;
    if flink == 0 || blink == 0 || flink == link || blink == link {
        return Ok(()); // already unlinked / self-linked
    }
    // prev->Flink = next
    remote_write(h, blink + 0, &(flink as u64).to_le_bytes())?;
    // next->Blink = prev
    remote_write(h, flink + 8, &(blink as u64).to_le_bytes())?;
    // Self-link the removed node.
    remote_write(h, link, &(link as u64).to_le_bytes())?;
    remote_write(h, link + 8, &(link as u64).to_le_bytes())?;
    Ok(())
}

fn hide_module_x86(h: HANDLE, base: usize) -> Result<(), String> {
    unsafe {
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
            return Err("ProcessWow64Information returned null".into());
        }
        let ldr = remote_read_ptr(h, peb32 + 0x0C, Arch::X86)?;
        if ldr == 0 {
            return Err("remote PEB32->Ldr is null".into());
        }
        // (list-head offset in PEB_LDR_DATA32, link-field offset within the entry)
        let lists = [(0x0Cusize, 0x00usize), (0x14, 0x08), (0x1C, 0x10)];
        let mut unlinked = 0usize;
        for (head_off, field_off) in lists {
            let head = ldr + head_off;
            let mut cur = remote_read_u32(h, head)? as usize;
            let mut guard = 0;
            while cur != 0 && cur != head && guard < 4096 {
                guard += 1;
                let next = remote_read_u32(h, cur)? as usize;
                // LDR_DATA_TABLE_ENTRY32: links at +0/+8/+0x10, DllBase at +0x18,
                // FullDllName at +0x24, BaseDllName at +0x2C.
                let entry = cur - field_off;
                let dll_base = remote_read_u32(h, entry + 0x18)? as usize;
                if dll_base == base {
                    unlink_x86(h, cur)?;
                    let _ = remote_write(h, entry + 0x24, &[0u8; 4]);
                    let _ = remote_write(h, entry + 0x2C, &[0u8; 4]);
                    unlinked += 1;
                }
                if next == cur {
                    break;
                }
                cur = next;
            }
        }
        if unlinked == 0 {
            return Err(format!("module {base:#x} not found in any PEB list"));
        }
    }
    Ok(())
}

fn unlink_x86(h: HANDLE, link: usize) -> Result<(), String> {
    let flink = remote_read_u32(h, link)? as usize;
    let blink = remote_read_u32(h, link + 4)? as usize;
    if flink == 0 || blink == 0 || flink == link || blink == link {
        return Ok(());
    }
    remote_write(h, blink, &(flink as u32).to_le_bytes())?;
    remote_write(h, flink + 4, &(blink as u32).to_le_bytes())?;
    remote_write(h, link, &(link as u32).to_le_bytes())?;
    remote_write(h, link + 4, &(link as u32).to_le_bytes())?;
    Ok(())
}

/// Hides the thread from debuggers via NtSetInformationThread.
pub fn hide_thread_from_debugger(tid: u32) -> Result<(), String> {
    unsafe {
        let th = kernel32::OpenThread(THREAD_SET_INFORMATION | THREAD_QUERY_INFORMATION, 0, tid);
        if th.is_null() {
            return Err(crate::util::last_error("OpenThread"));
        }
        let _g = HandleGuard(th);
        let st = (ntdll::get().NtSetInformationThread)(
            th,
            ThreadHideFromDebugger,
            std::ptr::null_mut(),
            0,
        );
        if st != 0 {
            return Err(nt_error(st, "NtSetInformationThread(ThreadHideFromDebugger)"));
        }
        Ok(())
    }
}
