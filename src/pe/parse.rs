//! Bounds-checked PE32 / PE32+ parser.

#[derive(Clone, Copy, Debug)]
pub struct Section {
    #[allow(dead_code)]
    pub name: [u8; 8],
    pub virtual_size: u32,
    pub virtual_address: u32,
    pub size_of_raw_data: u32,
    pub pointer_to_raw_data: u32,
    #[allow(dead_code)]
    pub characteristics: u32,
}

#[allow(dead_code)]
impl Section {
    pub fn name_str(&self) -> String {
        let end = self.name.iter().position(|&b| b == 0).unwrap_or(8);
        String::from_utf8_lossy(&self.name[..end]).to_string()
    }
    pub fn set_name(&mut self, s: &str) {
        let bytes = s.as_bytes();
        for i in 0..8 {
            self.name[i] = if i < bytes.len() { bytes[i] } else { 0 };
        }
    }
    /// Span this section occupies in memory (without the alignment round-up).
    pub fn memory_span(&self) -> u64 {
        self.virtual_size.max(self.size_of_raw_data) as u64
    }
}

#[derive(Clone, Debug)]
pub struct PeImage {
    pub pe_off: usize,
    pub opt_off: usize,
    pub pe32: bool, // true = PE32 (x86), false = PE32+ (x64)
    pub machine: u16,
    pub number_of_sections: u16,
    #[allow(dead_code)]
    pub size_of_optional_header: u16,
    pub size_of_headers: u32,
    pub size_of_image: u32,
    pub image_base: u64,
    pub address_of_entry_point: u32,
    pub section_off: usize,
    pub sections: Vec<Section>,
}

impl PeImage {
    pub fn parse(data: &[u8]) -> Result<PeImage, String> {
        if data.len() < 0x40 || &data[0..2] != b"MZ" {
            return Err("missing MZ signature".into());
        }
        let pe_off = u32::from_le_bytes(data[0x3C..0x40].try_into().unwrap()) as usize;
        if pe_off.checked_add(0x18).map_or(true, |e| e > data.len()) || &data[pe_off..pe_off + 4] != b"PE\0\0" {
            return Err("missing PE signature".into());
        }
        let machine = u16::from_le_bytes(data[pe_off + 4..pe_off + 6].try_into().unwrap());
        let number_of_sections = u16::from_le_bytes(data[pe_off + 6..pe_off + 8].try_into().unwrap());
        let size_of_optional_header = u16::from_le_bytes(data[pe_off + 20..pe_off + 22].try_into().unwrap());
        let opt_off = pe_off + 24;
        // The optional-header fields we read must be inside the file.
        if opt_off.checked_add(0x70).map_or(true, |e| e > data.len()) {
            return Err("truncated optional header".into());
        }
        let magic = u16::from_le_bytes(data[opt_off..opt_off + 2].try_into().unwrap());
        let pe32 = match magic {
            0x10B => true,
            0x20B => false,
            m => return Err(format!("unknown optional header magic {m:#x}")),
        };
        let size_of_headers = u32::from_le_bytes(data[opt_off + 60..opt_off + 64].try_into().unwrap());
        let address_of_entry_point = u32::from_le_bytes(data[opt_off + 16..opt_off + 20].try_into().unwrap());
        let image_base = if pe32 {
            u32::from_le_bytes(data[opt_off + 28..opt_off + 32].try_into().unwrap()) as u64
        } else {
            u64::from_le_bytes(data[opt_off + 24..opt_off + 32].try_into().unwrap())
        };
        let size_of_image = u32::from_le_bytes(data[opt_off + 56..opt_off + 60].try_into().unwrap());

        // SizeOfOptionalHeader decides where the section table is. Fall back to the
        // canonical 224/240 only if the field is absent or implausible.
        let canonical = if pe32 { 224usize } else { 240 };
        let declared = size_of_optional_header as usize;
        let mut section_off = opt_off + declared;
        if declared < 0x60 || section_off > data.len() {
            section_off = opt_off + canonical;
        }
        let sect_bytes = number_of_sections as usize * 40;
        if section_off.checked_add(sect_bytes).map_or(true, |e| e > data.len()) {
            return Err("section table out of bounds".into());
        }
        let mut sections = Vec::with_capacity(number_of_sections as usize);
        for i in 0..number_of_sections as usize {
            let o = section_off + i * 40;
            sections.push(Section {
                name: data[o..o + 8].try_into().unwrap(),
                virtual_size: u32::from_le_bytes(data[o + 8..o + 12].try_into().unwrap()),
                virtual_address: u32::from_le_bytes(data[o + 12..o + 16].try_into().unwrap()),
                size_of_raw_data: u32::from_le_bytes(data[o + 16..o + 20].try_into().unwrap()),
                pointer_to_raw_data: u32::from_le_bytes(data[o + 20..o + 24].try_into().unwrap()),
                characteristics: u32::from_le_bytes(data[o + 36..o + 40].try_into().unwrap()),
            });
        }

        Ok(PeImage {
            pe_off,
            opt_off,
            pe32,
            machine,
            number_of_sections,
            size_of_optional_header,
            size_of_headers,
            size_of_image,
            image_base,
            address_of_entry_point,
            section_off,
            sections,
        })
    }
}

/// Data directory read against the original file buffer: returns (RVA, Size).
pub fn data_dir(data: &[u8], pe: &PeImage, idx: usize) -> Option<(u32, u32)> {
    let dd_off = pe.opt_off + if pe.pe32 { 96 } else { 112 } + idx * 8;
    if dd_off.checked_add(8)? > data.len() {
        return None;
    }
    let rva = u32::from_le_bytes(data[dd_off..dd_off + 4].try_into().unwrap());
    let size = u32::from_le_bytes(data[dd_off + 4..dd_off + 8].try_into().unwrap());
    Some((rva, size))
}

/// Map a relative virtual address to a file offset using the section table.
pub fn rva_to_offset(pe: &PeImage, rva: u32) -> Option<usize> {
    for s in &pe.sections {
        let start = s.virtual_address;
        let span = s.virtual_size.max(s.size_of_raw_data);
        if rva >= start && rva < start.saturating_add(span) {
            let delta = rva - start;
            if delta >= s.size_of_raw_data {
                return None; // inside the zero-filled tail of the section
            }
            return Some(s.pointer_to_raw_data as usize + delta as usize);
        }
    }
    if rva < pe.size_of_headers {
        return Some(rva as usize);
    }
    None
}

/// RVA -> file offset, bounded against the actual buffer length.
pub fn rva_to_off(data: &[u8], pe: &PeImage, rva: u32) -> Option<usize> {
    let off = rva_to_offset(pe, rva)?;
    if off < data.len() {
        Some(off)
    } else {
        None
    }
}

/// Sanity checks a mapped image must pass before it touches the target. A malformed file
/// must produce an error, never a panic (the release build aborts on panic).
pub fn validate_image(data: &[u8], pe: &PeImage) -> Result<(), String> {
    let len = data.len() as u64;
    let soh = pe.size_of_headers as u64;
    if soh == 0 {
        return Err("SizeOfHeaders is zero".into());
    }
    if soh > len {
        return Err(format!("SizeOfHeaders {soh:#x} is larger than the file ({len:#x})"));
    }
    let soi = pe.size_of_image as u64;
    if soi == 0 {
        return Err("SizeOfImage is zero".into());
    }
    if soi > 0x4000_0000 {
        return Err(format!("SizeOfImage {soi:#x} is implausibly large"));
    }
    if soi < soh {
        return Err("SizeOfImage is smaller than SizeOfHeaders".into());
    }
    let table_end = pe.section_off as u64 + pe.number_of_sections as u64 * 40;
    if table_end > soh.min(len) {
        return Err("section table is outside the headers".into());
    }
    for (i, s) in pe.sections.iter().enumerate() {
        if s.size_of_raw_data == 0 && s.virtual_size == 0 {
            continue;
        }
        let va = s.virtual_address as u64;
        let vs = s.virtual_size as u64;
        let raw = s.size_of_raw_data as u64;
        let ptr = s.pointer_to_raw_data as u64;
        if va < soh {
            return Err(format!("section {i} starts inside the headers"));
        }
        if va.saturating_add(vs.max(raw)) > soi {
            return Err(format!(
                "section {i} (VA {va:#x}, size {:#x}) does not fit in SizeOfImage {soi:#x}",
                vs.max(raw)
            ));
        }
        if raw > 0 {
            if ptr != 0 && ptr < soh {
                return Err(format!("section {i} raw data overlaps the headers"));
            }
            if ptr.checked_add(raw).map_or(true, |e| e > len) {
                return Err(format!(
                    "section {i} raw data {ptr:#x}+{raw:#x} is outside the file ({len:#x})"
                ));
            }
        }
    }
    if pe.address_of_entry_point != 0 {
        let ep = pe.address_of_entry_point as u64;
        let inside = pe.sections.iter().any(|s| {
            let va = s.virtual_address as u64;
            ep >= va && ep < va + s.memory_span()
        });
        if !inside {
            return Err(format!("entry point {ep:#x} is outside every section"));
        }
    }
    Ok(())
}

/// Number of exported entries (functions + data), if the image exports anything.
pub fn export_count(data: &[u8], pe: &PeImage) -> Option<u32> {
    let (rva, size) = data_dir(data, pe, 0)?;
    if rva == 0 || size < 40 {
        return None;
    }
    let off = rva_to_off(data, pe, rva)?;
    if off + 24 > data.len() {
        return None;
    }
    let number_of_functions = u32::from_le_bytes(data[off + 20..off + 24].try_into().ok()?);
    if number_of_functions == 0 { None } else { Some(number_of_functions) }
}

/// DLL names from the import directory, as written in the file. Used to flag dependencies
/// (for example the VC++ runtime) before injecting.
pub fn import_dlls(data: &[u8], pe: &PeImage) -> Vec<String> {
    let mut out = Vec::new();
    let Some((rva, size)) = data_dir(data, pe, 1) else {
        return out;
    };
    if rva == 0 || size == 0 {
        return out;
    }
    let Some(start) = rva_to_off(data, pe, rva) else {
        return out;
    };
    let mut off = start;
    for _ in 0..256 {
        if off + 20 > data.len() {
            break;
        }
        let name_rva = u32::from_le_bytes(data[off + 12..off + 16].try_into().unwrap());
        let oft = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        let first_thunk = u32::from_le_bytes(data[off + 16..off + 20].try_into().unwrap());
        if name_rva == 0 && oft == 0 && first_thunk == 0 {
            break;
        }
        if let Some(name_off) = rva_to_off(data, pe, name_rva) {
            let end = data[name_off..]
                .iter()
                .position(|&b| b == 0)
                .map(|n| name_off + n)
                .unwrap_or(data.len());
            let name = String::from_utf8_lossy(&data[name_off..end]).to_string();
            if !name.is_empty() {
                out.push(name);
            }
        }
        off += 20;
    }
    out
}

/// Apply base relocations to a locally-mapped image (delta = new_base - image_base).
pub fn apply_relocations(mapped: &mut [u8], pe: &PeImage, delta: i64) -> Result<usize, String> {
    let (rva, size) = match data_dir_by_index(mapped, pe, 5) {
        Some(v) if v.0 != 0 && v.1 != 0 => v,
        _ => return Ok(0),
    };
    let mut off = rva as usize;
    let end = (rva as usize + size as usize).min(mapped.len());
    let mut applied = 0usize;
    while off + 8 <= end {
        let page_rva = u32::from_le_bytes(mapped[off..off + 4].try_into().unwrap());
        let block_size = u32::from_le_bytes(mapped[off + 4..off + 8].try_into().unwrap()) as usize;
        if block_size < 8 || off + block_size > end {
            break;
        }
        let entries = (block_size - 8) / 2;
        let mut i = 0usize;
        while i < entries {
            let e_off = off + 8 + i * 2;
            let entry = u16::from_le_bytes(mapped[e_off..e_off + 2].try_into().unwrap());
            let kind = entry >> 12;
            let offs = (entry & 0x0FFF) as usize;
            let target = page_rva as usize + offs;
            match kind {
                10 => {
                    // IMAGE_REL_BASED_DIR64
                    if target + 8 <= mapped.len() {
                        let v = i64::from_le_bytes(mapped[target..target + 8].try_into().unwrap());
                        let v = v.wrapping_add(delta);
                        mapped[target..target + 8].copy_from_slice(&v.to_le_bytes());
                        applied += 1;
                    }
                }
                3 => {
                    // IMAGE_REL_BASED_HIGHLOW
                    if target + 4 <= mapped.len() {
                        let v = i32::from_le_bytes(mapped[target..target + 4].try_into().unwrap());
                        let v = (v as i64).wrapping_add(delta) as i32;
                        mapped[target..target + 4].copy_from_slice(&v.to_le_bytes());
                        applied += 1;
                    }
                }
                4 => {
                    // IMAGE_REL_BASED_HIGHADJ consumes the following entry.
                    i += 1;
                }
                0 => {} // IMAGE_REL_BASED_ABSOLUTE (padding)
                _ => {}
            }
            i += 1;
        }
        off += block_size;
    }
    Ok(applied)
}

fn data_dir_by_index(mapped: &[u8], pe: &PeImage, idx: usize) -> Option<(u32, u32)> {
    data_dir(mapped, pe, idx)
}

// ===== TLS =====

/// IMAGE_TLS_DIRECTORY as stored in a buffer where the fields are image-base-relative VAs
/// (raw file) or already-relocated absolute VAs (a mapped image).
#[derive(Clone, Copy, Debug, Default)]
pub struct TlsDirectory {
    pub start_address: u64,
    pub end_address: u64,
    pub address_of_index: u64,
    pub address_of_callbacks: u64,
    pub size_of_zero_fill: u32,
    #[allow(dead_code)]
    pub characteristics: u32,
}

/// Parse an IMAGE_TLS_DIRECTORY at a byte offset inside `buf`.
pub fn tls_dir_at(buf: &[u8], off: usize, pe32: bool) -> Option<TlsDirectory> {
    let ptr = if pe32 { 4 } else { 8 };
    let need = if pe32 { 24 } else { 40 };
    if off.checked_add(need)? > buf.len() {
        return None;
    }
    let rd = |o: usize| -> u64 {
        if pe32 {
            u32::from_le_bytes(buf[off + o..off + o + 4].try_into().unwrap()) as u64
        } else {
            u64::from_le_bytes(buf[off + o..off + o + 8].try_into().unwrap())
        }
    };
    Some(TlsDirectory {
        start_address: rd(0),
        end_address: rd(ptr),
        address_of_index: rd(ptr * 2),
        address_of_callbacks: rd(ptr * 3),
        size_of_zero_fill: u32::from_le_bytes(buf[off + ptr * 4..off + ptr * 4 + 4].try_into().unwrap()),
        characteristics: u32::from_le_bytes(
            buf[off + ptr * 4 + 4..off + ptr * 4 + 8].try_into().unwrap(),
        ),
    })
}

/// TLS directory of a *file* image (fields are image-base-relative VAs).
pub fn tls_directory(data: &[u8], pe: &PeImage) -> Option<(u32, TlsDirectory)> {
    let (rva, _size) = data_dir(data, pe, 9)?;
    if rva == 0 {
        return None;
    }
    let off = rva_to_off(data, pe, rva)?;
    Some((rva, tls_dir_at(data, off, pe.pe32)?))
}

/// TLS callback RVAs from the original file buffer.
pub fn tls_callbacks(data: &[u8], pe: &PeImage) -> Option<Vec<u32>> {
    let (_dir_rva, tls) = tls_directory(data, pe)?;
    if tls.address_of_callbacks == 0 {
        return Some(Vec::new());
    }
    let cb_rva = tls.address_of_callbacks.wrapping_sub(pe.image_base) as u32;
    let mut off = rva_to_off(data, pe, cb_rva)?;
    let ptr = if pe.pe32 { 4 } else { 8 };
    let mut out = Vec::new();
    for _ in 0..64 {
        if off + ptr > data.len() {
            break;
        }
        let v: u64 = if pe.pe32 {
            u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as u64
        } else {
            u64::from_le_bytes(data[off..off + 8].try_into().unwrap())
        };
        if v == 0 {
            break;
        }
        let rva = v.wrapping_sub(pe.image_base);
        if rva == 0 || rva > u32::MAX as u64 {
            break;
        }
        out.push(rva as u32);
        off += ptr;
    }
    Some(out)
}

