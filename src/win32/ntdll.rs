//! ntdll declarations - resolved at runtime via GetProcAddress (no import-lib).
#![allow(non_snake_case, dead_code)]

use super::*;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CLIENT_ID {
    pub UniqueProcess: HANDLE,
    pub UniqueThread: HANDLE,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct OBJECT_ATTRIBUTES {
    pub Length: u32,
    pub RootDirectory: HANDLE,
    pub ObjectName: *mut c_void_raw,
    pub Attributes: u32,
    pub SecurityDescriptor: *mut c_void_raw,
    pub SecurityQualityOfService: *mut c_void_raw,
}

pub type PNTSTATUS_FN = unsafe extern "system" fn() -> NTSTATUS;

/// Dynamically resolved ntdll functions (resolved once, process-wide).
pub struct Ntdll {
    pub NtQueryInformationProcess:
        unsafe extern "system" fn(HANDLE, u32, *mut c_void_raw, u32, *mut u32) -> NTSTATUS,
    pub NtQueryInformationThread:
        unsafe extern "system" fn(HANDLE, u32, *mut c_void_raw, u32, *mut u32) -> NTSTATUS,
    pub NtSetInformationThread:
        unsafe extern "system" fn(HANDLE, u32, *mut c_void_raw, u32) -> NTSTATUS,
    pub NtCreateThreadEx: unsafe extern "system" fn(
        *mut HANDLE,
        u32,
        *mut OBJECT_ATTRIBUTES,
        HANDLE,
        *mut c_void_raw,
        *mut c_void_raw,
        u32,
        usize,
        usize,
        usize,
        *mut c_void_raw,
    ) -> NTSTATUS,
    pub RtlNtStatusToDosError: unsafe extern "system" fn(NTSTATUS) -> u32,
    pub RtlGetVersion: unsafe extern "system" fn(*mut RTL_OSVERSIONINFOW) -> NTSTATUS,
    /// Raw module base of ntdll in THIS process (for remote export reuse logic).
    pub base: HMODULE,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RTL_OSVERSIONINFOW {
    pub dwOSVersionInfoSize: u32,
    pub dwMajorVersion: u32,
    pub dwMinorVersion: u32,
    pub dwBuildNumber: u32,
    pub dwPlatformId: u32,
    pub szCSDVersion: [u16; 128],
}
impl Default for RTL_OSVERSIONINFOW {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

static mut NTDLL: Option<Ntdll> = None;

/// Resolve the NT functions once. Panics if ntdll is missing (impossible on Windows).
pub fn get() -> &'static Ntdll {
    unsafe {
        if let Some(n) = &*std::ptr::addr_of!(NTDLL) {
            return n;
        }
        let name = crate::util::wstr("ntdll.dll");
        let base = kernel32::GetModuleHandleW(name.as_ptr());
        if base.is_null() {
            panic!("ntdll.dll not found");
        }
        macro_rules! proc {
            ($name:literal) => {{
                let cname = concat!($name, "\0");
                match kernel32::GetProcAddress(base, cname.as_ptr()) {
                    Some(f) => std::mem::transmute::<_, unsafe extern "system" fn() -> NTSTATUS>(f),
                    None => panic!(concat!($name, " not found in ntdll")),
                }
            }};
        }
        let ntdll = Ntdll {
            NtQueryInformationProcess: std::mem::transmute(proc!("NtQueryInformationProcess")),
            NtQueryInformationThread: std::mem::transmute(proc!("NtQueryInformationThread")),
            NtSetInformationThread: std::mem::transmute(proc!("NtSetInformationThread")),
            NtCreateThreadEx: std::mem::transmute(proc!("NtCreateThreadEx")),
            RtlNtStatusToDosError: std::mem::transmute(proc!("RtlNtStatusToDosError")),
            RtlGetVersion: std::mem::transmute(proc!("RtlGetVersion")),
            base,
        };
        NTDLL = Some(ntdll);
        (*std::ptr::addr_of!(NTDLL)).as_ref().unwrap()
    }
}

/// OS version (major, minor, build) via RtlGetVersion.
pub fn os_version() -> (u32, u32, u32) {
    unsafe {
        let mut vi = RTL_OSVERSIONINFOW::default();
        vi.dwOSVersionInfoSize = std::mem::size_of::<RTL_OSVERSIONINFOW>() as u32;
        if (get().RtlGetVersion)(&mut vi) == 0 {
            (vi.dwMajorVersion, vi.dwMinorVersion, vi.dwBuildNumber)
        } else {
            (0, 0, 0)
        }
    }
}
