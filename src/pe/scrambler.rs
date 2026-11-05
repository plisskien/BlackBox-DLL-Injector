//! PE Header and Section Mutation Engine.
//! Applies non-destructive binary transformations to PE images before delivery.

use crate::config::ScrambleOptions;
use crate::pe::parse::{self, PeImage};
use crate::util::wstr;
use crate::win32::kernel32;

struct Rng(u64);
impl Rng {
    fn new() -> Rng {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15);
        Rng(t | 1)
    }
    fn next(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn byte(&mut self) -> u8 {
        (self.next() >> 24) as u8
    }
    fn range(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

/// Apply the enabled mutations in-place. `image` is a private copy â€” the on-disk
/// DLL is never touched (original behavior).
pub fn scramble(image: &mut Vec<u8>, opts: &ScrambleOptions, log: &dyn Fn(&str)) -> Result<(), String> {
    let mut rng = Rng::new();
    let pe = PeImage::parse(image)?;
    // A malformed input must be rejected here, before any mutation relies on its layout.
    parse::validate_image(image, &pe)?;

    // 1) RemoveUselessData â€” DOS stub, e_res/e_res2, rich header.
    if opts.remove_useless_data {
        // Zero the DOS stub area but keep the header fields intact:
        // e_lfanew lives at 0x3C..0x40 and must survive.
        image[0x02..0x3C].fill(0);
        // e_res/e_res2 live at 0x08..0x18 â€” already covered above.
        // Rich header: starts at 0x40 (after e_lfanew field begins at 0x3C). The
        // DOS stub region 0x40..pe_off contains the stub text + Rich/XPAD.
        if pe.pe_off > 0x40 && pe.pe_off <= image.len() {
            for b in &mut image[0x40..pe.pe_off] {
                *b = rng.byte();
            }
        }
        log("[*] Scramble: cleared DOS/rich headers");
    }

    // 2) RemoveDebugData / 3) CreateFakeDebugDirectory (mutually exclusive in effect).
    if opts.remove_debug_data {
        if let Some((rva, size)) = parse::data_dir(image, &pe, 6) {
            if rva > 0 && size > 0 {
                let off = rva_to_file(image, &pe, rva).unwrap_or(rva as usize);
                let end = off.saturating_add(size as usize).min(image.len());
                if off < end {
                    image[off..end].fill(0);
                }
            }
        }
        zero_data_dir(image, &pe, 6);
        log("[*] Scramble: stripped debug directory");
    } else if opts.create_fake_debug_directory {
        // Rewrite the name inside the *existing* CodeView record. Writing a fake entry at
        // entry+28 (the old behaviour) overwrote unrelated .rdata and crashed the target.
        if let Some((rva, size)) = parse::data_dir(image, &pe, 6) {
            if rva > 0 && size as usize >= 28 {
                let off = rva_to_file(image, &pe, rva).unwrap_or(rva as usize);
                if off.saturating_add(28) <= image.len() {
                    let size_of_data =
                        u32::from_le_bytes(image[off + 16..off + 20].try_into().unwrap()) as usize;
                    let data_ptr =
                        u32::from_le_bytes(image[off + 24..off + 28].try_into().unwrap()) as usize;
                    const FAKE: &[u8] =
                        b"C:\\Users\\Dev\\source\\repos\\GameEngine\\Release\\GameEngine.pdb\0";
                    let need = 24 + FAKE.len();
                    let usable = data_ptr != 0
                        && data_ptr.saturating_add(need) <= image.len()
                        && size_of_data >= need
                        && &image[data_ptr..data_ptr + 4] == b"RSDS";
                    if usable {
                        // Keep the GUID and age; only swap the PDB path in place.
                        image[data_ptr + 24..data_ptr + 24 + FAKE.len()].copy_from_slice(FAKE);
                        log("[*] Scramble: fake debug PDB name created");
                    } else {
                        log("[!] Scramble: debug directory has no writable CodeView record - skipped");
                    }
                }
            }
        }
    }

    // 4) ScrambleHeaderFields â€” TimeDateStamp, linker version, checksum.
    //    FileHeader: Machine@+0, NumberOfSections@+2, TimeDateStamp@+8.
    //    OptionalHeader: MajorLinkerVersion/MinorLinkerVersion are its first two bytes.
    if opts.scramble_header_fields {
        // OptionalHeader layout: Magic@0(2), MajorLinkerVersion@2, MinorLinkerVersion@3.
        image[pe.opt_off + 2] = rng.byte(); // MajorLinkerVersion
        image[pe.opt_off + 3] = rng.byte(); // MinorLinkerVersion
        let ts = pe.pe_off + 8;
        image[ts..ts + 4].copy_from_slice(&(rng.next() as u32).to_le_bytes()); // TimeDateStamp
        let cs = pe.opt_off + 64;
        image[cs..cs + 4].copy_from_slice(&(rng.next() as u32).to_le_bytes()); // CheckSum
        log("[*] Scramble: randomized PE header fields");
    }

    // 5) RenameSections.
    if opts.rename_sections {
        let names = [
            ".xdta", ".xrsrc", ".codex", ".bssq", ".rdatx", ".data9", ".idata2", ".reloc2",
            ".edata1", ".tlsx", ".gfids", ".didat", ".text9",
        ];
        for (i, _s) in pe.sections.iter().enumerate() {
            let n = names[i % names.len()];
            let o = pe.section_off + i * 40;
            image[o..o + 8].copy_from_slice(&pad_name(n));
        }
        log("[*] Scramble: renamed sections");
    }

    // 6) StripSectionCharacteristics â€” drop the code/data type hints, keep memory perms sane.
    if opts.strip_section_characteristics {
        for i in 0..pe.number_of_sections as usize {
            let o = pe.section_off + i * 40;
            let mut chars = u32::from_le_bytes(image[o + 36..o + 40].try_into().unwrap());
            chars &= !(0x0000_0020 | 0x0000_0040 | 0x0000_0080); // CNT_CODE / CNT_IDATA / CNT_UDATA
            chars &= !(0x0200_0000 | 0x0400_0000); // DISCARDABLE / NOT_CACHED
            image[o + 36..o + 40].copy_from_slice(&chars.to_le_bytes());
        }
        log("[*] Scramble: stripped section flags");
    }

    // 7) ShiftSectionData â€” insert variable file padding before each section's raw data.
    if opts.shift_section_data {
        shift_section_data(image, &pe, &mut rng)?;
        log("[*] Scramble: shifted raw section offsets");
    }

    // 8) ShiftSectionMemory â€” move sections at virtual alignment granularity.
    //    Relocation blocks referencing shifted pages are fixed up accordingly.
    if opts.shift_section_memory {
        // Moving sections without rebasing every RVA in the image corrupts it (the entry
        // point, data directories, import/export/TLS tables all keep pointing at the old
        // addresses). The option is kept for UI compatibility but skipped with a warning
        // instead of producing a broken DLL.
        log("[!] Scramble: ShiftSectionMemory needs a full PE rebase and was skipped (unsupported)");
    }

    // 9) MoveRelocationTable â€” the reloc directory stays functional but its owning
    //    section is renamed and the directory entry is offset to a copy at a new file spot.
    if opts.move_relocation_table {
        if let Some((rva, size)) = parse::data_dir(image, &pe, 5) {
            if rva > 0 && size > 0 {
                let file_off = rva_to_file(image, &pe, rva).unwrap_or(rva as usize);
                if file_off.saturating_add(size as usize) <= image.len() {
                    let reloc = image[file_off..file_off + size as usize].to_vec();
                    // Append at the end of the file.
                    let new_off = image.len();
                    image.extend_from_slice(&reloc);
                    // New raw offset -> new RVA (same as file offset tail: use offset from first section
                    // alignment base). For loader-based methods the dir is consumed via RVA; we keep the
                    // RVA valid by mapping the tail into the last section's raw overflow (common layout).
                    let new_rva = file_off_to_rva(image, &pe, new_off as u32).unwrap_or(rva);
                    set_data_dir(image, &pe, 5, new_rva, size);
                    log(&format!("[*] Scramble: reloc table @ {:#x}", new_rva));
                }
            }
        }
    }

    // 10) InsertExtraSections â€” append decoy section headers with junk bodies.
    if opts.insert_extra_sections {
        insert_extra_sections(image, &pe, &mut rng)?;
        log("[*] Scramble: inserted decoy sections");
    }

    // 11) ModifyImportTable â€” shuffle import descriptor order (decoy imports skipped:
    //     adding real deps would change load behavior; order shuffling is the safe core).
    if opts.modify_import_table {
        if let Some((rva, _size)) = parse::data_dir(image, &pe, 1) {
            if rva > 0 {
                let off = rva_to_file(image, &pe, rva).unwrap_or(rva as usize);
                // Count descriptors.
                let mut n = 0usize;
                while off + (n + 1) * 20 <= image.len() {
                    let d = &image[off + n * 20..off + n * 20 + 20];
                    if d[12..20] == [0; 8] {
                        break;
                    }
                    n += 1;
                }
                if n > 1 {
                    // In-place rotation shuffle of the descriptor records.
                    for i in (1..n).rev() {
                        let j = 1 + rng.range(i); // keep first in place
                        let a = off + i * 20;
                        let b = off + j * 20;
                        for k in 0..20 {
                            image.swap(a + k, b + k);
                        }
                    }
                    log(&format!("[*] Scramble: shuffled {} imports", n));
                }
            }
        }
    }

    // 12) ModifyAssemblyCode â€” rewrite NOP/int3 padding runs in executable sections
    //     with equivalent encodings (safe subset; no live-instruction mutation).
    if opts.modify_assembly_code {
        let mut patched = 0usize;
        for i in 0..pe.number_of_sections as usize {
            let o = pe.section_off + i * 40;
            let chars = u32::from_le_bytes(image[o + 36..o + 40].try_into().unwrap());
            if chars & 0x2000_0000 == 0 {
                continue; // not executable
            }
            let raw = u32::from_le_bytes(image[o + 16..o + 20].try_into().unwrap()) as usize;
            let ptr = u32::from_le_bytes(image[o + 20..o + 24].try_into().unwrap()) as usize;
            let vsize = u32::from_le_bytes(image[o + 8..o + 12].try_into().unwrap()) as usize;
            if ptr + raw > image.len() {
                continue;
            }
            // Padding tail: between virtual_size and size_of_raw_data.
            let tail = raw.saturating_sub(vsize.min(raw));
            if tail >= 2 {
                for b in &mut image[ptr + vsize..ptr + vsize + tail] {
                    if *b == 0x90 || *b == 0x00 || *b == 0xCC {
                        *b = [0x90, 0x66, 0x0F][rng.range(3)];
                        patched += 1;
                    }
                }
            }
        }
        log(&format!("[*] Scramble: mutated {} padding bytes", patched));
    }

    // 13) CreateNewEntryPoint â€” trampoline section that jumps to the real entry.
    if opts.create_new_entry_point {
        create_new_entry_point(image, &pe, &mut rng)?;
        log("[*] Scramble: created entry trampoline");
    }

    // Re-check parse-ability; a failure here means we corrupted the image â€” abort injection.
    PeImage::parse(image).map(|_| ()).map_err(|e| format!("scrambler produced an invalid PE ({e}); injection aborted"))
}

fn pad_name(n: &str) -> [u8; 8] {
    let mut out = [0u8; 8];
    let b = n.as_bytes();
    for i in 0..8.min(b.len()) {
        out[i] = b[i];
    }
    out
}

fn rva_to_file(data: &[u8], pe: &PeImage, rva: u32) -> Option<usize> {
    if rva < pe.size_of_headers {
        return Some(rva as usize);
    }
    for i in 0..pe.number_of_sections as usize {
        let o = pe.section_off + i * 40;
        let va = u32::from_le_bytes(data[o + 12..o + 16].try_into().unwrap());
        let raw = u32::from_le_bytes(data[o + 16..o + 20].try_into().unwrap());
        let ptr = u32::from_le_bytes(data[o + 20..o + 24].try_into().unwrap());
        if rva >= va && rva < va + raw.max(1) {
            return Some(ptr as usize + (rva - va) as usize);
        }
    }
    None
}

fn file_off_to_rva(data: &[u8], pe: &PeImage, off: u32) -> Option<u32> {
    for i in 0..pe.number_of_sections as usize {
        let o = pe.section_off + i * 40;
        let va = u32::from_le_bytes(data[o + 12..o + 16].try_into().unwrap());
        let raw = u32::from_le_bytes(data[o + 16..o + 20].try_into().unwrap());
        let ptr = u32::from_le_bytes(data[o + 20..o + 24].try_into().unwrap());
        if off >= ptr && off < ptr + raw.max(1) {
            return Some(va + (off - ptr));
        }
    }
    None
}

fn zero_data_dir(data: &mut [u8], pe: &PeImage, idx: usize) {
    let dd_off = pe.opt_off + if pe.pe32 { 96 } else { 112 } + idx * 8;
    if dd_off + 8 <= data.len() {
        for b in &mut data[dd_off..dd_off + 8] {
            *b = 0;
        }
    }
}

fn set_data_dir(data: &mut [u8], pe: &PeImage, idx: usize, rva: u32, size: u32) {
    let dd_off = pe.opt_off + if pe.pe32 { 96 } else { 112 } + idx * 8;
    if dd_off + 8 <= data.len() {
        data[dd_off..dd_off + 4].copy_from_slice(&rva.to_le_bytes());
        data[dd_off + 4..dd_off + 8].copy_from_slice(&size.to_le_bytes());
    }
}

/// Shift raw data pointers by inserting junk padding between sections (file grows).
/// The padding is a whole number of FileAlignment units: the Windows loader rejects images
/// whose PointerToRawData is not FileAlignment aligned (the old 0x40..0x100 padding made
/// every loader-based injection fail with ERROR_BAD_EXE_FORMAT).
fn shift_section_data(data: &mut Vec<u8>, pe: &PeImage, rng: &mut Rng) -> Result<(), String> {
    let file_align = {
        let a = u32::from_le_bytes(data[pe.opt_off + 36..pe.opt_off + 40].try_into().unwrap());
        if a == 0 || !a.is_power_of_two() || a > 0x10000 {
            0x200
        } else {
            a
        }
    } as usize;
    // Work from the last section backwards so shifts don't invalidate earlier offsets.
    let mut shifts: Vec<(usize, usize)> = Vec::new(); // (old_ptr, shift)
    for i in 0..pe.number_of_sections as usize {
        let o = pe.section_off + i * 40;
        let raw = u32::from_le_bytes(data[o + 16..o + 20].try_into().unwrap());
        let ptr = u32::from_le_bytes(data[o + 20..o + 24].try_into().unwrap()) as usize;
        if raw == 0 || ptr == 0 || ptr >= data.len() {
            continue;
        }
        let pad = file_align * (1 + rng.range(2));
        shifts.push((ptr, pad));
    }
    // Apply from highest offset down.
    shifts.sort_by(|a, b| b.0.cmp(&a.0));
    for (ptr, pad) in shifts {
        if ptr > data.len() {
            continue;
        }
        let junk = vec![0u8; pad];
        data.splice(ptr..ptr, junk);
        // Adjust every PointerToRawData >= ptr.
        for i in 0..pe.number_of_sections as usize {
            let o = pe.section_off + i * 40;
            let p = u32::from_le_bytes(data[o + 20..o + 24].try_into().unwrap()) as usize;
            if p >= ptr && p != 0 {
                data[o + 20..o + 24].copy_from_slice(&((p + pad) as u32).to_le_bytes());
            }
        }
    }
    Ok(())
}

fn insert_extra_sections(data: &mut Vec<u8>, _pe: &PeImage, rng: &mut Rng) -> Result<(), String> {
    let fresh = PeImage::parse(data).map_err(|e| e.to_string())?;
    let names = [".rsrc2", ".idata3", ".data4", ".ctx1", ".meta9"];
    let extra = 2usize;
    // Need header room: section table must not overflow the first page or the file.
    let new_count = fresh.number_of_sections as usize + extra;
    let table_end = fresh.section_off + new_count * 40;
    if table_end > fresh.size_of_headers as usize || table_end > data.len() {
        return Err("no room in headers for decoy sections".into());
    }
    let align = u32::from_le_bytes(data[fresh.opt_off + 32..fresh.opt_off + 36].try_into().unwrap());
    let file_align = {
        let a = u32::from_le_bytes(data[fresh.opt_off + 36..fresh.opt_off + 40].try_into().unwrap());
        if a == 0 || !a.is_power_of_two() {
            0x200
        } else {
            a
        }
    };
    // Highest current VA and raw end.
    let mut max_va = fresh.size_of_headers;
    let mut max_raw = fresh.size_of_headers;
    for s in &fresh.sections {
        max_va = max_va.max(s.virtual_address + align_up(s.virtual_size.max(s.size_of_raw_data), align));
        max_raw = max_raw.max(s.pointer_to_raw_data + s.size_of_raw_data);
    }
    let mut va = align_up(max_va, align);
    let mut raw = align_up(data.len() as u32, file_align);
    for i in 0..extra {
        let body: Vec<u8> = (0..0x200).map(|_| (rng.next() & 0xFF) as u8).collect();
        let o = fresh.section_off + (fresh.number_of_sections as usize + i) * 40;
        data[o..o + 8].copy_from_slice(&pad_name(names[i % names.len()]));
        data[o + 8..o + 12].copy_from_slice(&(body.len() as u32).to_le_bytes()); // VirtualSize
        data[o + 12..o + 16].copy_from_slice(&va.to_le_bytes()); // VirtualAddress
        data[o + 16..o + 20].copy_from_slice(&(body.len() as u32).to_le_bytes()); // SizeOfRawData
        data[o + 20..o + 24].copy_from_slice(&raw.to_le_bytes()); // PointerToRawData
        data[o + 36..o + 40].copy_from_slice(&0x4000_0040u32.to_le_bytes()); // CNT_IDATA | READ | WRITE
        // Pad the file to `raw`, then place the body exactly there.
        if (data.len() as u64) < raw as u64 {
            data.resize(raw as usize, 0);
        }
        data.extend_from_slice(&body);
        va += align_up(body.len() as u32, align);
        raw += align_up(body.len() as u32, file_align);
    }
    data[fresh.pe_off + 6..fresh.pe_off + 8].copy_from_slice(&(new_count as u16).to_le_bytes());
    let soi = u32::from_le_bytes(data[fresh.opt_off + 56..fresh.opt_off + 60].try_into().unwrap());
    data[fresh.opt_off + 56..fresh.opt_off + 60].copy_from_slice(&(soi + align_up(0x400, align) * extra as u32).to_le_bytes());
    Ok(())
}

/// Point the entry point at a 5-byte `jmp rel32` trampoline in a code cave (the raw tail of
/// an executable section past its VirtualSize).
///
/// * `rel32` is position independent, so it survives ASLR and the manual mapper (the old
///   x64 `mov rax, imm64` used the preferred image base and crashed every relocated load).
/// * the section's VirtualSize is grown to cover the trampoline, so both the loader and the
///   manual mapper copy the bytes into memory.
fn create_new_entry_point(data: &mut Vec<u8>, _pe: &PeImage, _rng: &mut Rng) -> Result<(), String> {
    let fresh = PeImage::parse(data).map_err(|e| e.to_string())?;
    let orig_ep = fresh.address_of_entry_point;
    if orig_ep == 0 {
        return Err("image has no entry point to trampoline".into());
    }
    const TRAMP_LEN: usize = 5;
    let mut chosen: Option<(usize, u32, usize)> = None; // (section index, cave VA, file offset)
    for (i, s) in fresh.sections.iter().enumerate() {
        if s.characteristics & 0x2000_0000 == 0 {
            continue; // not executable
        }
        let raw = s.size_of_raw_data as usize;
        let vsize = s.virtual_size as usize;
        let ptr = s.pointer_to_raw_data as usize;
        if raw < vsize + TRAMP_LEN {
            continue;
        }
        if ptr.saturating_add(raw) > data.len() || ptr.saturating_add(vsize + TRAMP_LEN) > data.len() {
            continue;
        }
        chosen = Some((i, s.virtual_address + s.virtual_size, ptr + vsize));
        break;
    }
    let Some((idx, cave_va, off)) = chosen else {
        return Err("no code cave available for the entry trampoline".into());
    };
    let rel = (orig_ep as i64) - (cave_va as i64 + TRAMP_LEN as i64);
    if rel < i32::MIN as i64 || rel > i32::MAX as i64 {
        return Err("entry trampoline is out of rel32 range".into());
    }
    data[off] = 0xE9;
    data[off + 1..off + 5].copy_from_slice(&(rel as i32).to_le_bytes());
    // Grow VirtualSize so the trampoline is part of the section for loaders and mappers.
    let s = &fresh.sections[idx];
    let new_vsize = s.virtual_size.saturating_add(TRAMP_LEN as u32);
    let sec_off = fresh.section_off + idx * 40;
    data[sec_off + 8..sec_off + 12].copy_from_slice(&new_vsize.to_le_bytes());
    // SizeOfImage must still cover max(VirtualSize, SizeOfRawData) of every section.
    let end = (s.virtual_address as u64) + (new_vsize as u64).max(s.size_of_raw_data as u64);
    let section_align = u32::from_le_bytes(data[fresh.opt_off + 32..fresh.opt_off + 36].try_into().unwrap());
    let needed = align_up(end.min(u32::MAX as u64) as u32, section_align.max(0x1000));
    if needed > fresh.size_of_image {
        data[fresh.opt_off + 56..fresh.opt_off + 60].copy_from_slice(&needed.to_le_bytes());
    }
    data[fresh.opt_off + 16..fresh.opt_off + 20].copy_from_slice(&cave_va.to_le_bytes());
    Ok(())
}

fn align_up(v: u32, a: u32) -> u32 {
    if a == 0 {
        return v;
    }
    (v + a - 1) & !(a - 1)
}

/// Materialize a scrambled image as a temp file for loader-based methods.
pub fn write_temp_dll(original_path: &str, image: &[u8]) -> Result<String, String> {
    let _ = original_path;
    unsafe {
        let mut buf = [0u16; 512];
        let n = kernel32::GetTempPathW(buf.len() as u32, buf.as_mut_ptr());
        if n == 0 {
            return Err(crate::util::last_error("GetTempPathW"));
        }
        let tmp: String = crate::util::from_wsz(buf.as_ptr());
        let dir = format!("{}BlackBoxInjector", tmp);
        let wdir = wstr(&dir);
        kernel32::CreateDirectoryW(wdir.as_ptr(), std::ptr::null_mut()); // ok if exists
        let name = format!(
            "{}\\scrambled_{}.dll",
            dir,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
        );
        let wname = wstr(&name);
        let mut h = kernel32::CreateFileW(
            wname.as_ptr(),
            crate::win32::GENERIC_WRITE,
            0,
            std::ptr::null_mut(),
            2, // CREATE_ALWAYS
            0,
            crate::win32::NULL_HANDLE,
        );
        let mut final_name = name;
        if h == crate::win32::INVALID_HANDLE_VALUE || h.is_null() {
            // Fallback to local directory if temp directory write is denied by sandbox ACLs
            if let Ok(cur) = std::env::current_dir() {
                let local_dir = cur.join("reports");
                let _ = std::fs::create_dir_all(&local_dir);
                let fallback = local_dir.join(format!(
                    "scrambled_{}.dll",
                    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
                ));
                let fallback_str = fallback.to_string_lossy().into_owned();
                let wfallback = wstr(&fallback_str);
                h = kernel32::CreateFileW(
                    wfallback.as_ptr(),
                    crate::win32::GENERIC_WRITE,
                    0,
                    std::ptr::null_mut(),
                    2,
                    0,
                    crate::win32::NULL_HANDLE,
                );
                if h != crate::win32::INVALID_HANDLE_VALUE && !h.is_null() {
                    final_name = fallback_str;
                }
            }
        }
        if h == crate::win32::INVALID_HANDLE_VALUE || h.is_null() {
            return Err(crate::util::last_error("CreateFileW(temp)"));
        }
        let mut written = 0u32;
        let ok = kernel32::WriteFile(h, image.as_ptr() as _, image.len() as u32, &mut written, std::ptr::null_mut()) != 0;
        let _ = kernel32::CloseHandle(h);
        if !ok {
            return Err(crate::util::last_error("WriteFile(temp)"));
        }
        Ok(final_name)
    }
}
