//! Method 1: LdrLoadDll - direct invocation of the native NT module loader.

use super::*;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Us64 {
    length: u16,
    max: u16,
    _pad: u32,
    buffer: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Us32 {
    length: u16,
    max: u16,
    buffer: u32,
}

pub fn inject(h: HANDLE, arch: Arch, load_path: &str, opts: &InjectOptions, log: Logger) -> Result<usize, String> {
    let dll_path = load_path;

    let mods = remote_modules(h, arch)?;
    let ntdll_mod = find_module(&mods, "ntdll.dll").ok_or("ntdll.dll not found in target")?;
    let ldrload = match remote_export(h, arch, ntdll_mod.base, "LdrLoadDll", 0) {
        Ok(a) => a,
        Err(_) => {
            log("[!] LdrLoadDll not exported, trying LdrpLoadDll (undocumented)");
            remote_export(h, arch, ntdll_mod.base, "LdrpLoadDll", 0)?
        }
    };
    log(&format!("[*] Remote ntdll!LdrLoadDll @ {:#x}", ldrload));

    // Layout (remote RW buffer): [out ptr][UNICODE_STRING][path UTF-16][stub]
    let wpath = wbuf(dll_path);
    let stub: Vec<u8>;
    let us_size: usize;
    let mut layout = Layout::default();
    match arch {
        Arch::X64 => {
            us_size = 16;
            layout.out = 0;
            layout.us = 8;
            layout.path = layout.us + us_size;
            layout.stub = layout.path + wpath.len() * 2;
        }
        Arch::X86 => {
            us_size = 8;
            layout.out = 0;
            layout.us = 4;
            layout.path = layout.us + us_size;
            layout.stub = layout.path + wpath.len() * 2;
        }
    }
    let total = layout.stub + 64;
    // RWX: the stub executes from this allocation (DEP would kill an RW-only stub).
    let buf = remote_alloc(h, total, PAGE_EXECUTE_READWRITE)?;

    let out_addr = buf + layout.out;
    let us_addr = buf + layout.us;
    let path_addr = buf + layout.path;
    let stub_addr = buf + layout.stub;

    // UNICODE_STRING
    let byte_len = (wpath.len() * 2) as u16;
    match arch {
        Arch::X64 => {
            let us = Us64 {
                length: byte_len,
                max: byte_len + 2,
                _pad: 0,
                buffer: path_addr as u64,
            };
            remote_write(h, us_addr, &us.as_bytes())?;
        }
        Arch::X86 => {
            let us = Us32 {
                length: byte_len,
                max: byte_len + 2,
                buffer: path_addr as u32,
            };
            remote_write(h, us_addr, &us.as_bytes())?;
        }
    }
    let mut path_bytes = Vec::with_capacity(byte_len as usize + 2);
    for c in &wpath {
        path_bytes.extend_from_slice(&c.to_le_bytes());
    }
    path_bytes.extend_from_slice(&[0, 0]);
    remote_write(h, path_addr, &path_bytes)?;

    match arch {
        Arch::X64 => stub = shellcode::ldrload_stub_x64(ldrload, us_addr, out_addr),
        Arch::X86 => stub = shellcode::ldrload_stub_x86(ldrload, us_addr, out_addr),
    }
    remote_write(h, stub_addr, &stub)?;

    let result = (|| -> Result<usize, String> {
        let th = create_remote_thread(h, arch, stub_addr, 0, opts.effective_hide_thread())?;
        let _g = HandleGuard(th);
        let st = match wait_thread_status(th, 60_000) {
            ThreadWait::Done(c) => c as i32,
            ThreadWait::TimedOut => {
                return Err(
                    "LdrLoadDll did not return within 60 s (the DLL is blocking inside DllMain)".into(),
                )
            }
            ThreadWait::Failed(e) => return Err(e),
        };
        if st < 0 {
            return Err(nt_error(st, "LdrLoadDll"));
        }
        let base = remote_read_ptr(h, out_addr, arch)?;
        if base == 0 {
            return Err("LdrLoadDll succeeded but returned null base".into());
        }
        Ok(base)
    })();

    // The stub, the UNICODE_STRING and the path all live in `buf`; only release it once the
    // loader call has provably finished.
    match &result {
        Ok(_) => {
            let _ = remote_free(h, buf);
        }
        Err(_) => {
            log("[!] Leaving the LdrLoadDll buffer allocated (the loader may still be using it)");
        }
    }
    result
}

#[derive(Default)]
struct Layout {
    out: usize,
    us: usize,
    path: usize,
    stub: usize,
}

trait AsBytes {
    fn as_bytes(&self) -> Vec<u8>;
}
impl AsBytes for Us64 {
    fn as_bytes(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&self.length.to_le_bytes());
        v.extend_from_slice(&self.max.to_le_bytes());
        v.extend_from_slice(&self._pad.to_le_bytes());
        v.extend_from_slice(&self.buffer.to_le_bytes());
        v
    }
}
impl AsBytes for Us32 {
    fn as_bytes(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&self.length.to_le_bytes());
        v.extend_from_slice(&self.max.to_le_bytes());
        v.extend_from_slice(&self.buffer.to_le_bytes());
        v
    }
}

