//! Hand-rolled Win32/NT FFI. Zero third-party crates - only the declarations this
//! project actually uses. Types are deliberately loose (`*mut c_void` handles)
//! to keep the layer thin.

#![allow(non_snake_case, non_camel_case_types, dead_code, clippy::missing_safety_doc)]

pub mod advapi32;
pub mod dwmapi;
pub mod gdi32;
pub mod kernel32;
pub mod ntdll;
pub mod shell32;
pub mod user32;
pub mod urlmon;
pub mod wininet;
pub mod uxtheme;

pub type HANDLE = *mut std::ffi::c_void;
pub type HMODULE = HANDLE;
pub type HINSTANCE = HANDLE;
pub type HWND = HANDLE;
pub type HDC = HANDLE;
pub type HBITMAP = HANDLE;
pub type HBRUSH = HANDLE;
pub type HCURSOR = HANDLE;
pub type HICON = HANDLE;
pub type HFONT = HANDLE;
pub type HGDIOBJ = HANDLE;
pub type HMENU = HANDLE;
pub type HKEY = HANDLE;
pub type HGLOBAL = HANDLE;
pub type HRSRC = HANDLE;
pub type HWINSTA = HANDLE;
pub type COLORREF = u32;
pub type DWORD = u32;
pub type WORD = u16;
pub type BYTE = u8;
pub type BOOL = i32;
pub type UINT = u32;
pub type LONG = i32;
pub type ULONG = u32;
pub type ULONGLONG = u64;
pub type LONG_PTR = isize;
pub type ULONG_PTR = usize;
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type LRESULT = isize;
pub type NTSTATUS = i32;

pub const NULL_HANDLE: HANDLE = std::ptr::null_mut();
pub const TRUE: BOOL = 1;
pub const FALSE: BOOL = 0;

pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct SIZE {
    pub cx: LONG,
    pub cy: LONG,
}

// ===== Shared constants =====
pub const FORMAT_MESSAGE_FROM_SYSTEM: u32 = 0x0000_1000;
pub const FORMAT_MESSAGE_IGNORE_INSERTS: u32 = 0x0000_0200;

pub const GENERIC_READ: u32 = 0x8000_0000;
pub const GENERIC_WRITE: u32 = 0x4000_0000;

pub const PROCESS_CREATE_THREAD: u32 = 0x0002;
pub const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
pub const PROCESS_VM_OPERATION: u32 = 0x0008;
pub const PROCESS_VM_READ: u32 = 0x0010;
pub const PROCESS_VM_WRITE: u32 = 0x0020;
pub const PROCESS_SET_INFORMATION: u32 = 0x0200;
pub const PROCESS_ALL_ACCESS: u32 = 0x001F_0FFF;

pub const THREAD_SUSPEND_RESUME: u32 = 0x0002;
pub const THREAD_GET_CONTEXT: u32 = 0x0008;
pub const THREAD_SET_CONTEXT: u32 = 0x0010;
pub const THREAD_QUERY_INFORMATION: u32 = 0x0040;
pub const THREAD_SET_INFORMATION: u32 = 0x0020;
pub const THREAD_ALL_ACCESS: u32 = 0x001F_03FF;

pub const MEM_COMMIT: u32 = 0x1000;
pub const MEM_RESERVE: u32 = 0x2000;
pub const MEM_RELEASE: u32 = 0x8000;

pub const PAGE_NOACCESS: u32 = 0x01;
pub const PAGE_READONLY: u32 = 0x02;
pub const PAGE_READWRITE: u32 = 0x04;
pub const PAGE_WRITECOPY: u32 = 0x08;
pub const PAGE_EXECUTE: u32 = 0x10;
pub const PAGE_EXECUTE_READ: u32 = 0x20;
pub const PAGE_EXECUTE_READWRITE: u32 = 0x40;

pub const INFINITE: u32 = 0xFFFF_FFFF;
pub const WAIT_OBJECT_0: u32 = 0;
pub const WAIT_TIMEOUT: u32 = 0x0000_0102;
pub const STILL_ACTIVE: u32 = 259;
pub const SYNCHRONIZE: u32 = 0x0010_0000;
pub const WAIT_FAILED: u32 = 0xFFFF_FFFF;

// Registry
pub const HKEY_CURRENT_USER: HKEY = 0x8000_0001isize as HKEY;
pub const HKEY_LOCAL_MACHINE: HKEY = 0x8000_0002isize as HKEY;
pub const KEY_READ: u32 = 0x0002_0019;
pub const KEY_WRITE: u32 = 0x0002_0006;
pub const KEY_ALL_ACCESS: u32 = 0x000F_003F;
pub const REG_OPTION_NON_VOLATILE: u32 = 0x0000_0000;
pub const REG_SZ: u32 = 1;
pub const REG_MULTI_SZ: u32 = 7;
pub const REG_DWORD: u32 = 4;
pub const REG_EXPAND_SZ: u32 = 2;

// Tokens
pub const TOKEN_ADJUST_PRIVILEGES: u32 = 0x0020;
pub const TOKEN_QUERY: u32 = 0x0008;
pub const SE_PRIVILEGE_ENABLED: u32 = 0x0000_0002;

// Toolhelp
pub const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
pub const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;
pub const TH32CS_SNAPMODULE: u32 = 0x0000_0008;

pub const MAX_PATH: usize = 260;
pub const MAX_MODULE_NAME32: usize = 255;

// Process information classes
#[allow(non_upper_case_globals)]
pub const ProcessBasicInformation: u32 = 0;
#[allow(non_upper_case_globals)]
pub const ProcessWow64Information: u32 = 26;

// Thread information classes
#[allow(non_upper_case_globals)]
pub const ThreadQuerySetWin32StartAddress: u32 = 9;

// NtSetInformationThread class
#[allow(non_upper_case_globals)]
pub const ThreadHideFromDebugger: u32 = 0x11;

// NtCreateThreadEx create flags
pub const THREAD_CREATE_FLAGS_CREATE_SUSPENDED: u32 = 0x0000_0001;
pub const THREAD_CREATE_FLAGS_HIDE_FROM_DEBUGGER: u32 = 0x0000_0004;

// CONTEXT flags (x64)
pub const CONTEXT_AMD64: u32 = 0x0010_0000;
pub const CONTEXT_CONTROL_AMD64: u32 = CONTEXT_AMD64 | 0x0000_0001;
pub const CONTEXT_INTEGER_AMD64: u32 = CONTEXT_AMD64 | 0x0000_0002;
pub const CONTEXT_FULL_AMD64: u32 = CONTEXT_AMD64 | 0x0000_000B;
// WOW64 (x86) CONTEXT
pub const CONTEXT_I386: u32 = 0x0001_0000;
pub const CONTEXT_CONTROL_X86: u32 = CONTEXT_I386 | 0x0000_0001;
pub const CONTEXT_INTEGER_X86: u32 = CONTEXT_I386 | 0x0000_0002;

// ===== Shared structures =====

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct LIST_ENTRY {
    pub Flink: *mut c_void_raw,
    pub Blink: *mut c_void_raw,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct c_void_raw {
    _p: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct UNICODE_STRING {
    pub Length: u16,
    pub MaximumLength: u16,
    pub Buffer: *mut u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PROCESS_BASIC_INFORMATION {
    pub ExitStatus: *mut c_void_raw,
    pub PebBaseAddress: *mut c_void_raw,
    pub Reserved2: [*mut c_void_raw; 2],
    pub UniqueProcessId: *mut c_void_raw,
    pub InheritedFromUniqueProcessId: *mut c_void_raw,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SCROLLINFO {
    pub cbSize: u32,
    pub fMask: u32,
    pub nMin: i32,
    pub nMax: i32,
    pub nPage: u32,
    pub nPos: i32,
    pub nTrackPos: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct TOKEN_PRIVILEGES {
    pub PrivilegeCount: u32,
    pub Luid: u64,
    pub Attributes: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct STARTUPINFOW {
    pub cb: u32,
    pub _pad: [u8; 60],
}
impl Default for STARTUPINFOW {
    fn default() -> Self {
        STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            _pad: [0; 60],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PROCESSENTRY32W {
    pub dwSize: u32,
    pub cntUsage: u32,
    pub th32ProcessID: u32,
    pub th32DefaultHeapID: usize,
    pub th32ModuleID: u32,
    pub cntThreads: u32,
    pub th32ParentProcessID: u32,
    pub pcPriClassBase: LONG,
    pub dwFlags: u32,
    pub szExeFile: [u16; MAX_PATH],
}
impl Default for PROCESSENTRY32W {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct THREADENTRY32 {
    pub dwSize: u32,
    pub cntUsage: u32,
    pub th32ThreadID: u32,
    pub th32OwnerProcessID: u32,
    pub tpBasePri: LONG,
    pub tpDeltaPri: LONG,
    pub dwFlags: u32,
}

/// x64 CONTEXT - header fields we use + raw tail (FP/vector/branch area).
/// Total size must be 1232 bytes (0x4D0) as winnt.h defines it.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CONTEXT_X64 {
    pub P1Home: u64,
    pub P2Home: u64,
    pub P3Home: u64,
    pub P4Home: u64,
    pub P5Home: u64,
    pub P6Home: u64,
    pub ContextFlags: u32,
    pub MxCsr: u32,
    pub SegCs: u16,
    pub SegDs: u16,
    pub SegEs: u16,
    pub SegFs: u16,
    pub SegGs: u16,
    pub SegSs: u16,
    pub EFlags: u32,
    pub Dr0: u64,
    pub Dr1: u64,
    pub Dr2: u64,
    pub Dr3: u64,
    pub Dr6: u64,
    pub Dr7: u64,
    pub Rax: u64,
    pub Rcx: u64,
    pub Rdx: u64,
    pub Rbx: u64,
    pub Rsp: u64,
    pub Rbp: u64,
    pub Rsi: u64,
    pub Rdi: u64,
    pub R8: u64,
    pub R9: u64,
    pub R10: u64,
    pub R11: u64,
    pub R12: u64,
    pub R13: u64,
    pub R14: u64,
    pub R15: u64,
    pub Rip: u64,
    pub _tail: [u8; 976], // FltSave + vector + branch history (0x100..0x4D0)
}
impl Default for CONTEXT_X64 {
    fn default() -> Self {
        // SAFETY: all-zero is a valid "cleared" context.
        unsafe { std::mem::zeroed() }
    }
}

/// WOW64 (x86) CONTEXT - the subset used for hijacking (winnt.h WOW64_CONTEXT).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WOW64_CONTEXT {
    pub ContextFlags: u32,
    pub Dr0: u32,
    pub Dr1: u32,
    pub Dr2: u32,
    pub Dr3: u32,
    pub Dr6: u32,
    pub Dr7: u32,
    pub FloatSave: [u8; 112],
    pub SegGs: u32,
    pub SegFs: u32,
    pub SegEs: u32,
    pub SegDs: u32,
    pub Edi: u32,
    pub Esi: u32,
    pub Ebx: u32,
    pub Edx: u32,
    pub Ecx: u32,
    pub Eax: u32,
    pub Ebp: u32,
    pub Eip: u32,
    pub SegCs: u32,
    pub EFlags: u32,
    pub Esp: u32,
    pub SegSs: u32,
    pub ExtendedRegisters: [u8; 512],
}
impl Default for WOW64_CONTEXT {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}
