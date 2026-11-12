//! Hand-assembled stub builders (x86 + x64). Every builder returns raw machine code
//! with absolute remote addresses baked in at build time.
//!
//! Stub conventions:
//! * call stubs return the callee's return value in RAX/EAX, which becomes the remote
//!   thread's exit code (CreateRemoteThread / NtCreateThreadEx propagate it), so the
//!   injector can read e.g. TlsAlloc's index or DllMain's BOOL without a shared slot.
//! * hijack stubs set a done flag and then spin; the injector restores the thread.

pub fn push32(v: Vec<u8>, x: u32) -> Vec<u8> {
    let mut v = v;
    v.extend_from_slice(&x.to_le_bytes());
    v
}
pub fn push64(v: Vec<u8>, x: u64) -> Vec<u8> {
    let mut v = v;
    v.extend_from_slice(&x.to_le_bytes());
    v
}

const SPIN: [u8; 4] = [0xF3, 0x90, 0xEB, 0xFC]; // pause; jmp -4

/// x86: LdrLoadDll(NULL, NULL, &US, &out) via absolute addressing, then ret.
pub fn ldrload_stub_x86(fn_addr: usize, us_addr: usize, out_addr: usize) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&[0xB8]);
    v = push32(v, out_addr as u32); // mov eax, out
    v.push(0x50); // push eax
    v.extend_from_slice(&[0xB8]);
    v = push32(v, us_addr as u32); // mov eax, &US
    v.push(0x50); // push eax
    v.extend_from_slice(&[0x6A, 0x00]); // push 0  (DllCharacteristics)
    v.extend_from_slice(&[0x6A, 0x00]); // push 0  (DllPath)
    v.extend_from_slice(&[0xB8]);
    v = push32(v, fn_addr as u32); // mov eax, LdrLoadDll
    v.extend_from_slice(&[0xFF, 0xD0]); // call eax
    v.push(0xC3); // ret (stdcall callee cleaned)
    v
}

/// x64: LdrLoadDll(NULL, NULL, &US, &out) with MS x64 ABI. `out_addr` is a remote
/// buffer slot the stub stores the module base into.
pub fn ldrload_stub_x64(fn_addr: usize, us_addr: usize, out_addr: usize) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&[0x48, 0x83, 0xEC, 0x28]); // sub rsp, 0x28
    v.extend_from_slice(&[0x31, 0xC9]); // xor ecx, ecx (DllPath)
    v.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx (DllCharacteristics)
    v.extend_from_slice(&[0x49, 0xB8]);
    v = push64(v, us_addr as u64); // mov r8, &US
    v.extend_from_slice(&[0x49, 0xB9]);
    v = push64(v, out_addr as u64); // mov r9, &out (remote buffer slot)
    v.extend_from_slice(&[0x48, 0xB8]);
    v = push64(v, fn_addr as u64); // mov rax, LdrLoadDll
    v.extend_from_slice(&[0xFF, 0xD0]); // call rax
    v.extend_from_slice(&[0x48, 0x83, 0xC4, 0x28]); // add rsp, 0x28
    v.push(0xC3); // ret
    v
}

/// Generic x64 "call fn(a1, a2, a3)" stub. The callee's return value stays in RAX and
/// becomes the thread exit code.
pub fn call3_stub_x64(fn_addr: usize, a1: usize, a2: usize, a3: usize) -> Vec<u8> {    let mut v = Vec::new();
    v.extend_from_slice(&[0x48, 0x83, 0xEC, 0x28]); // sub rsp, 0x28 (shadow + align)
    v.extend_from_slice(&[0x48, 0xB9]);
    v = push64(v, a1 as u64); // mov rcx, a1
    v.extend_from_slice(&[0x48, 0xBA]);
    v = push64(v, a2 as u64); // mov rdx, a2
    v.extend_from_slice(&[0x49, 0xB8]);
    v = push64(v, a3 as u64); // mov r8, a3
    v.extend_from_slice(&[0x48, 0xB8]);
    v = push64(v, fn_addr as u64); // mov rax, fn
    v.extend_from_slice(&[0xFF, 0xD0]); // call rax
    v.extend_from_slice(&[0x48, 0x83, 0xC4, 0x28]); // add rsp, 0x28
    v.push(0xC3); // ret
    v
}

/// Generic x86 "call fn(a1, a2, a3)" stub (cdecl/stdcall, callee cleans). The return
/// value in EAX becomes the thread exit code.
pub fn call3_stub_x86(fn_addr: usize, a1: usize, a2: usize, a3: usize) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&[0x68]);
    v = push32(v, a3 as u32); // push a3
    v.extend_from_slice(&[0x68]);
    v = push32(v, a2 as u32); // push a2
    v.extend_from_slice(&[0x68]);
    v = push32(v, a1 as u32); // push a1
    v.extend_from_slice(&[0xB8]);
    v = push32(v, fn_addr as u32); // mov eax, fn
    v.extend_from_slice(&[0xFF, 0xD0]); // call eax
    v.push(0xC3); // ret
    v
}

/// x64 call stub that also stores the 64-bit return value in a remote slot (the thread
/// exit code is only 32 bits, so pointers cannot be returned through it).
pub fn call3_stub_result_x64(fn_addr: usize, a1: usize, a2: usize, a3: usize, slot: usize) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&[0x48, 0x83, 0xEC, 0x28]);
    v.extend_from_slice(&[0x48, 0xB9]);
    v = push64(v, a1 as u64);
    v.extend_from_slice(&[0x48, 0xBA]);
    v = push64(v, a2 as u64);
    v.extend_from_slice(&[0x49, 0xB8]);
    v = push64(v, a3 as u64);
    v.extend_from_slice(&[0x48, 0xB8]);
    v = push64(v, fn_addr as u64);
    v.extend_from_slice(&[0xFF, 0xD0]); // call rax
    v.extend_from_slice(&[0x49, 0xBB]);
    v = push64(v, slot as u64); // mov r11, slot
    v.extend_from_slice(&[0x49, 0x89, 0x03]); // mov [r11], rax
    v.extend_from_slice(&[0x48, 0x83, 0xC4, 0x28]);
    v.push(0xC3);
    v
}

/// x86 call stub that also stores the return value in a remote slot.
pub fn call3_stub_result_x86(fn_addr: usize, a1: usize, a2: usize, a3: usize, slot: usize) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&[0x68]);
    v = push32(v, a3 as u32);
    v.extend_from_slice(&[0x68]);
    v = push32(v, a2 as u32);
    v.extend_from_slice(&[0x68]);
    v = push32(v, a1 as u32);
    v.extend_from_slice(&[0xB8]);
    v = push32(v, fn_addr as u32);
    v.extend_from_slice(&[0xFF, 0xD0]); // call eax
    v.extend_from_slice(&[0xA3]);
    v = push32(v, slot as u32); // mov [slot], eax
    v.push(0xC3);
    v
}

/// Gated x64 call that stores its result and then terminates the thread with
/// `TerminateThread(GetCurrentThread(), 0)` (pseudo-handle -2).
///
/// The loader's thread-exit path frees every TLS block/array it owns. For a thread whose
/// TLS state we extended that teardown is not safe, and terminating skips it entirely.
pub fn call3_gated_result_term_x64(
    fn_addr: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    gate: usize,
    slot: usize,
    terminate: usize,
) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&[0x48, 0x83, 0xEC, 0x28]);
    v.extend_from_slice(&[0x48, 0xB8]);
    v = push64(v, gate as u64);
    let spin_at = v.len();
    v.extend_from_slice(&[0x80, 0x38, 0x00]);
    let rel = (spin_at as isize) - ((v.len() + 2) as isize);
    v.extend_from_slice(&[0x74, rel as u8]);
    v.extend_from_slice(&[0x48, 0xB9]);
    v = push64(v, a1 as u64);
    v.extend_from_slice(&[0x48, 0xBA]);
    v = push64(v, a2 as u64);
    v.extend_from_slice(&[0x49, 0xB8]);
    v = push64(v, a3 as u64);
    v.extend_from_slice(&[0x48, 0xB8]);
    v = push64(v, fn_addr as u64);
    v.extend_from_slice(&[0xFF, 0xD0]); // call rax
    v.extend_from_slice(&[0x49, 0xBB]);
    v = push64(v, slot as u64); // mov r11, slot
    v.extend_from_slice(&[0x49, 0x89, 0x03]); // mov [r11], rax
    v.extend_from_slice(&[0x48, 0xC7, 0xC1, 0xFE, 0xFF, 0xFF, 0xFF]); // mov rcx, -2
    v.extend_from_slice(&[0x48, 0xB8]);
    v = push64(v, terminate as u64);
    v.extend_from_slice(&[0xFF, 0xD0]); // call TerminateThread
    v.extend_from_slice(&[0x48, 0x83, 0xC4, 0x28]);
    v.push(0xC3);
    v
}

/// x86 variant of the gated, result-storing, self-terminating call.
pub fn call3_gated_result_term_x86(
    fn_addr: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    gate: usize,
    slot: usize,
    terminate: usize,
) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&[0xB8]);
    v = push32(v, gate as u32);
    let spin_at = v.len();
    v.extend_from_slice(&[0x80, 0x38, 0x00]);
    let rel = (spin_at as isize) - ((v.len() + 2) as isize);
    v.extend_from_slice(&[0x74, rel as u8]);
    v.extend_from_slice(&[0x68]);
    v = push32(v, a3 as u32);
    v.extend_from_slice(&[0x68]);
    v = push32(v, a2 as u32);
    v.extend_from_slice(&[0x68]);
    v = push32(v, a1 as u32);
    v.extend_from_slice(&[0xB8]);
    v = push32(v, fn_addr as u32);
    v.extend_from_slice(&[0xFF, 0xD0]); // call eax
    v.extend_from_slice(&[0xA3]);
    v = push32(v, slot as u32); // mov [slot], eax
    v.extend_from_slice(&[0x6A, 0x00]); // push 0 (exit code)
    v.extend_from_slice(&[0x68, 0xFE, 0xFF, 0xFF, 0xFF]); // push -2 (current thread)
    v.extend_from_slice(&[0xB8]);
    v = push32(v, terminate as u32);
    v.extend_from_slice(&[0xFF, 0xD0]); // call TerminateThread
    v.push(0xC3);
    v
}

pub struct HijackStub {
    pub code: Vec<u8>,
    /// Offset of the rel32 field inside `code` (x64 only).
    pub rel32_at: usize,
    /// Offset just past the store instruction (x64 rel32 is relative to base+this).
    pub after_flag_off: usize,
}

/// x64 hijack stub: LoadLibraryW(path), set done flag, then spin. The INJECTOR
/// re-suspends the thread and restores the saved context once the flag is set - this is
/// required because the thread may be suspended inside a syscall stub where volatile
/// registers (RAX = syscall number) are live; returning in-thread would corrupt them.
pub fn hijack_stub_x64(loadlib: usize, path: usize, _orig_rip: usize, _done: usize) -> HijackStub {
    let mut v = Vec::new();
    v.extend_from_slice(&[0x48, 0x83, 0xEC, 0x28]); // sub rsp, 0x28 (shadow + alignment)
    v.extend_from_slice(&[0x48, 0xB9]);
    v = push64(v, path as u64); // mov rcx, path
    v.extend_from_slice(&[0x48, 0xB8]);
    v = push64(v, loadlib as u64); // mov rax, LoadLibraryW
    v.extend_from_slice(&[0xFF, 0xD0]); // call rax
    // done flag: C7 05 rel32 imm32
    v.extend_from_slice(&[0xC7, 0x05]);
    let rel32_at = v.len();
    v = push32(v, 0); // patched by patch_done_rel32
    v = push32(v, 1);
    let after_flag_off = v.len();
    v.extend_from_slice(&SPIN);
    HijackStub { code: v, rel32_at, after_flag_off }
}

/// x86 hijack stub: LoadLibraryW(path), then `mov dword [done], 1` with the absolute
/// address patched in by the caller, then spin. The injector restores the saved context
/// afterwards, so the thread never returns into code it already executed.
pub struct HijackStub86 {
    pub code: Vec<u8>,
    /// Offset of the absolute done address inside `code`.
    pub done_at: usize,
}

pub fn hijack_stub_x86(loadlib: usize, path: usize) -> HijackStub86 {
    let mut v = Vec::new();
    // push path; mov eax, LoadLibraryW; call eax
    v.extend_from_slice(&[0x68]);
    v = push32(v, path as u32);
    v.extend_from_slice(&[0xB8]);
    v = push32(v, loadlib as u32);
    v.extend_from_slice(&[0xFF, 0xD0]);
    // mov dword ptr [imm32], 1  =>  C7 05 imm32 01 00 00 00
    v.extend_from_slice(&[0xC7, 0x05]);
    let done_at = v.len();
    v = push32(v, 0); // patched with the absolute done address
    v = push32(v, 1);
    v.extend_from_slice(&SPIN);
    HijackStub86 { code: v, done_at }
}

/// Patch the rel32 in a built x64 hijack stub once the stub's remote base is known.
pub fn patch_done_rel32(stub: &mut [u8], stub_base: usize, done: usize, after_flag_off: usize, rel32_at: usize) {
    let next = (stub_base + after_flag_off) as i64;
    let rel = (done as i64 - next) as i32;
    stub[rel32_at..rel32_at + 4].copy_from_slice(&rel.to_le_bytes());
}

/// Patch the absolute done address in a built x86 hijack stub.
pub fn patch_done_abs32(stub: &mut [u8], done: usize, done_at: usize) {
    stub[done_at..done_at + 4].copy_from_slice(&(done as u32).to_le_bytes());
}

/// x64 stub used to register a manually mapped module's TLS directory with the loader:
/// `LdrLockLoaderLock(0, 0, &cookie); status = LdrpHandleTlsData(entry, 0);
/// LdrUnlockLoaderLock(0, cookie); return status`. `cookie` is a remote 8-byte slot.
pub fn tls_register_stub_x64(
    lock: usize,
    handle: usize,
    unlock: usize,
    entry: usize,
    cookie: usize,
) -> Vec<u8> {
    let mut v = Vec::with_capacity(96);
    v.extend_from_slice(&[0x48, 0x83, 0xEC, 0x38]); // sub rsp, 0x38
    v.extend_from_slice(&[0x31, 0xC9]); // xor ecx, ecx
    v.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    v.extend_from_slice(&[0x49, 0xB8]);
    v = push64(v, cookie as u64); // mov r8, &cookie
    v.extend_from_slice(&[0x48, 0xB8]);
    v = push64(v, lock as u64); // mov rax, LdrLockLoaderLock
    v.extend_from_slice(&[0xFF, 0xD0]); // call rax

    v.extend_from_slice(&[0x48, 0xB9]);
    v = push64(v, entry as u64); // mov rcx, entry
    v.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    v.extend_from_slice(&[0x48, 0xB8]);
    v = push64(v, handle as u64); // mov rax, LdrpHandleTlsData
    v.extend_from_slice(&[0xFF, 0xD0]); // call rax
    v.extend_from_slice(&[0x49, 0x89, 0xC4]); // mov r12, rax

    v.extend_from_slice(&[0x31, 0xC9]); // xor ecx, ecx
    v.extend_from_slice(&[0x48, 0xBA]);
    v = push64(v, cookie as u64); // mov rdx, &cookie
    v.extend_from_slice(&[0x48, 0x8B, 0x12]); // mov rdx, [rdx]
    v.extend_from_slice(&[0x48, 0xB8]);
    v = push64(v, unlock as u64); // mov rax, LdrUnlockLoaderLock
    v.extend_from_slice(&[0xFF, 0xD0]); // call rax
    v.extend_from_slice(&[0x4C, 0x89, 0xE0]); // mov rax, r12
    v.extend_from_slice(&[0x48, 0x83, 0xC4, 0x38]); // add rsp, 0x38
    v.push(0xC3); // ret
    v
}

/// NOP variants for ModifyAssemblyCode (safe: only padding runs are rewritten).
#[allow(dead_code)]
pub const X86_NOP_ALT: [&[u8]; 4] = [&[0x90], &[0x66, 0x90], &[0x0F, 0x1F, 0x00], &[0x0F, 0x1F, 0x40, 0x00]];

