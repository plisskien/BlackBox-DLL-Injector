//! kernel32 declarations.
#![allow(non_snake_case, dead_code)]

use super::*;

#[link(name = "kernel32")]
extern "system" {
    pub fn GetLastError() -> u32;
    pub fn GetCurrentProcess() -> HANDLE;
    pub fn GetCurrentProcessId() -> u32;
    pub fn GetCurrentThreadId() -> u32;
    pub fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: BOOL, dwProcessId: u32) -> HANDLE;
    pub fn CloseHandle(hObject: HANDLE) -> BOOL;
    pub fn VirtualAllocEx(
        hProcess: HANDLE,
        lpAddress: *mut c_void_raw,
        dwSize: usize,
        flAllocationType: u32,
        flProtect: u32,
    ) -> *mut c_void_raw;
    pub fn VirtualFreeEx(hProcess: HANDLE, lpAddress: *mut c_void_raw, dwSize: usize, dwFreeType: u32) -> BOOL;
    pub fn VirtualProtectEx(
        hProcess: HANDLE,
        lpAddress: *mut c_void_raw,
        dwSize: usize,
        flNewProtect: u32,
        lpflOldProtect: *mut u32,
    ) -> BOOL;
    pub fn WriteProcessMemory(
        hProcess: HANDLE,
        lpBaseAddress: *mut c_void_raw,
        lpBuffer: *const c_void_raw,
        nSize: usize,
        lpNumberOfBytesWritten: *mut usize,
    ) -> BOOL;
    pub fn ReadProcessMemory(
        hProcess: HANDLE,
        lpBaseAddress: *mut c_void_raw,
        lpBuffer: *mut c_void_raw,
        nSize: usize,
        lpNumberOfBytesRead: *mut usize,
    ) -> BOOL;
    pub fn CreateRemoteThread(
        hProcess: HANDLE,
        lpThreadAttributes: *mut c_void_raw,
        dwStackSize: usize,
        lpStartAddress: LPTHREAD_START_ROUTINE,
        lpParameter: *mut c_void_raw,
        dwCreationFlags: u32,
        lpThreadId: *mut u32,
    ) -> HANDLE;
    pub fn GetExitCodeThread(hThread: HANDLE, lpExitCode: *mut u32) -> BOOL;
    pub fn GetExitCodeProcess(hProcess: HANDLE, lpExitCode: *mut u32) -> BOOL;
    pub fn WaitForSingleObject(hHandle: HANDLE, dwMilliseconds: u32) -> u32;
    pub fn SuspendThread(hThread: HANDLE) -> u32;
    pub fn ResumeThread(hThread: HANDLE) -> u32;
    pub fn TerminateThread(hThread: HANDLE, dwExitCode: u32) -> BOOL;
    pub fn GetThreadContext(hThread: HANDLE, lpContext: *mut CONTEXT_X64) -> BOOL;
    pub fn SetThreadContext(hThread: HANDLE, lpContext: *const CONTEXT_X64) -> BOOL;
    pub fn Wow64GetThreadContext(hThread: HANDLE, lpContext: *mut WOW64_CONTEXT) -> BOOL;
    pub fn Wow64SetThreadContext(hThread: HANDLE, lpContext: *const WOW64_CONTEXT) -> BOOL;
    pub fn IsWow64Process(hProcess: HANDLE, Wow64Process: *mut BOOL) -> BOOL;
    pub fn GetModuleHandleW(lpModuleName: *const u16) -> HMODULE;
    pub fn GetModuleHandleA(lpModuleName: *const u8) -> HMODULE;
    pub fn GetProcAddress(hModule: HMODULE, lpProcName: *const u8) -> FARPROC;
    pub fn LoadLibraryW(lpLibFileName: *const u16) -> HMODULE;
    pub fn FreeLibrary(hLibModule: HMODULE) -> BOOL;
    pub fn CreateToolhelp32Snapshot(dwFlags: u32, th32ProcessID: u32) -> HANDLE;
    pub fn Process32FirstW(hSnapshot: HANDLE, lppe: *mut PROCESSENTRY32W) -> BOOL;
    pub fn Process32NextW(hSnapshot: HANDLE, lppe: *mut PROCESSENTRY32W) -> BOOL;
    pub fn Thread32First(hSnapshot: HANDLE, lpte: *mut THREADENTRY32) -> BOOL;
    pub fn Thread32Next(hSnapshot: HANDLE, lpte: *mut THREADENTRY32) -> BOOL;
    pub fn Module32FirstW(hSnapshot: HANDLE, lpme: *mut MODULEENTRY32W) -> BOOL;
    pub fn Module32NextW(hSnapshot: HANDLE, lpme: *mut MODULEENTRY32W) -> BOOL;
    pub fn OpenThread(dwDesiredAccess: u32, bInheritHandle: BOOL, dwThreadId: u32) -> HANDLE;
    pub fn GetThreadId(hThread: HANDLE) -> u32;
    pub fn QueueUserAPC(pfnAPC: usize, hThread: HANDLE, dwData: usize) -> u32;
    pub fn Sleep(dwMilliseconds: u32);
    pub fn GetProcessDEPPolicy(
        hProcess: HANDLE,
        lpFlags: *mut u32,
        lpPermanent: *mut BOOL,
    ) -> BOOL;
    pub fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *mut c_void_raw,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: HANDLE,
    ) -> HANDLE;
    pub fn ReadFile(
        hFile: HANDLE,
        lpBuffer: *mut c_void_raw,
        nNumberOfBytesToRead: u32,
        lpNumberOfBytesRead: *mut u32,
        lpOverlapped: *mut c_void_raw,
    ) -> BOOL;
    pub fn WriteFile(
        hFile: HANDLE,
        lpBuffer: *const c_void_raw,
        nNumberOfBytesToWrite: u32,
        lpNumberOfBytesWritten: *mut u32,
        lpOverlapped: *mut c_void_raw,
    ) -> BOOL;
    pub fn GetFileSize(hFile: HANDLE, lpFileSizeHigh: *mut u32) -> u32;
    pub fn SetFilePointer(hFile: HANDLE, lDistanceToMove: LONG, lpDistanceToMoveHigh: *mut LONG, dwMoveMethod: u32) -> u32;
    pub fn SetEndOfFile(hFile: HANDLE) -> BOOL;
    pub fn GetTempPathW(nBufferLength: u32, lpBuffer: *mut u16) -> u32;
    pub fn CreateDirectoryW(lpPathName: *const u16, lpSecurityAttributes: *mut c_void_raw) -> BOOL;
    pub fn DeleteFileW(lpFileName: *const u16) -> BOOL;
    pub fn GetWindowsDirectoryW(lpBuffer: *mut u16, uSize: u32) -> u32;
    pub fn SetProcessWorkingSetSize(hProcess: HANDLE, dwMin: usize, dwMax: usize) -> BOOL;
    pub fn GetModuleFileNameW(hModule: HMODULE, lpFilename: *mut u16, nSize: u32) -> u32;
    pub fn GetStdHandle(nStdHandle: u32) -> HANDLE;
    pub fn FormatMessageW(
        dwFlags: u32,
        lpSource: *const c_void_raw,
        dwMessageId: u32,
        dwLanguageId: u32,
        lpBuffer: *mut u16,
        nSize: u32,
        Arguments: *mut c_void_raw,
    ) -> u32;
    pub fn WriteConsoleW(
        hConsoleOutput: HANDLE,
        lpBuffer: *const u16,
        nNumberOfCharsToWrite: u32,
        lpNumberOfCharsWritten: *mut u32,
        lpReserved: *mut c_void_raw,
    ) -> BOOL;
    pub fn AttachConsole(dwProcessId: u32) -> BOOL;
    pub fn AllocConsole() -> BOOL;
    pub fn SetStdHandle(nStdHandle: u32, hHandle: HANDLE) -> BOOL;
}

pub type LPTHREAD_START_ROUTINE = Option<unsafe extern "system" fn(*mut c_void_raw) -> u32>;
pub type FARPROC = Option<unsafe extern "system" fn() -> isize>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MODULEENTRY32W {
    pub dwSize: u32,
    pub th32ModuleID: u32,
    pub th32ProcessID: u32,
    pub GlblcntUsage: u32,
    pub ProccntUsage: u32,
    pub modBaseAddr: *mut u8,
    pub modBaseSize: u32,
    pub hModule: HMODULE,
    pub szModule: [u16; MAX_MODULE_NAME32 + 1],
    pub szExePath: [u16; MAX_PATH],
}
impl Default for MODULEENTRY32W {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

pub const OPEN_EXISTING: u32 = 3;
pub const FILE_SHARE_READ: u32 = 0x1;
pub const FILE_SHARE_WRITE: u32 = 0x2;
pub const FILE_BEGIN: u32 = 0;
pub const INVALID_FILE_SIZE: u32 = 0xFFFF_FFFF;

/// Convenience: read a whole file into a Vec<u8>.
pub fn read_file(path: &str) -> Result<Vec<u8>, String> {
    unsafe {
        let wpath = crate::util::wstr(path);
        let h = CreateFileW(
            wpath.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            NULL_HANDLE,
        );
        if h == INVALID_HANDLE_VALUE || h.is_null() {
            return Err(crate::util::last_error("CreateFileW"));
        }
        let _g = scopeguard_close(h);
        let hi = std::ptr::null_mut();
        let size = GetFileSize(h, hi);
        if size == INVALID_FILE_SIZE {
            return Err("file too large".into());
        }
        let mut buf = vec![0u8; size as usize];
        let mut read = 0u32;
        if ReadFile(h, buf.as_mut_ptr() as _, size, &mut read, std::ptr::null_mut()) == 0 {
            return Err(crate::util::last_error("ReadFile"));
        }
        buf.truncate(read as usize);
        Ok(buf)
    }
}

struct ScopeGuardClose(HANDLE);
impl Drop for ScopeGuardClose {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn scopeguard_close(h: HANDLE) -> ScopeGuardClose {
    ScopeGuardClose(h)
}
