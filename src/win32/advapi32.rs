//! advapi32 declarations - registry + token privileges.
#![allow(non_snake_case, dead_code)]

use super::*;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct LUID {
    pub LowPart: u32,
    pub HighPart: LONG,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LUID_AND_ATTRIBUTES {
    pub Luid: LUID,
    pub Attributes: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TOKEN_PRIVILEGES_FULL {
    pub PrivilegeCount: u32,
    pub Privileges: [LUID_AND_ATTRIBUTES; 1],
}

#[link(name = "advapi32")]
extern "system" {
    pub fn OpenProcessToken(process: HANDLE, desired: u32, token: *mut HANDLE) -> BOOL;
    pub fn GetTokenInformation(
        token: HANDLE,
        class: u32,
        info: *mut c_void_raw,
        len: u32,
        ret: *mut u32,
    ) -> BOOL;
    pub fn LookupPrivilegeValueW(
        system: *const u16,
        name: *const u16,
        luid: *mut LUID,
    ) -> BOOL;
    pub fn AdjustTokenPrivileges(
        token: HANDLE,
        disableall: BOOL,
        newstate: *const TOKEN_PRIVILEGES_FULL,
        buflen: u32,
        prev: *mut c_void_raw,
        retlen: *mut u32,
    ) -> BOOL;
    pub fn RegCreateKeyExW(
        key: HKEY,
        subkey: *const u16,
        reserved: u32,
        class: *const u16,
        options: u32,
        sam: u32,
        secattr: *mut c_void_raw,
        result: *mut HKEY,
        disposition: *mut u32,
    ) -> i32;
    pub fn RegOpenKeyExW(key: HKEY, subkey: *const u16, options: u32, sam: u32, result: *mut HKEY) -> i32;
    pub fn RegCloseKey(key: HKEY) -> i32;
    pub fn RegSetValueExW(
        key: HKEY,
        name: *const u16,
        reserved: u32,
        kind: u32,
        data: *const u8,
        size: u32,
    ) -> i32;
    pub fn RegQueryValueExW(
        key: HKEY,
        name: *const u16,
        reserved: *mut u32,
        kind: *mut u32,
        data: *mut u8,
        size: *mut u32,
    ) -> i32;
    pub fn RegEnumKeyExW(
        key: HKEY,
        index: u32,
        name: *mut u16,
        name_len: *mut u32,
        reserved: *mut u32,
        class: *mut u16,
        class_len: *mut u32,
        last_write: *mut c_void_raw,
    ) -> i32;
    pub fn RegDeleteValueW(key: HKEY, name: *const u16) -> i32;
    pub fn RegDeleteTreeW(key: HKEY, subkey: *const u16) -> i32;
    pub fn OpenEventLogA(server: *const u8, source: *const u8) -> HANDLE;
    pub fn ReadEventLogA(
        log: HANDLE,
        flags: u32,
        offset: u32,
        buffer: *mut u8,
        bytes_to_read: u32,
        bytes_read: *mut u32,
        bytes_needed: *mut u32,
    ) -> BOOL;
    pub fn CloseEventLog(log: HANDLE) -> BOOL;
}

/// SE_DEBUG_NAME
pub const SE_DEBUG_NAME: &str = "SeDebugPrivilege";

/// TOKEN_INFORMATION_CLASS::TokenElevation
pub const TOKEN_ELEVATION_CLASS: u32 = 20;

pub const EVENTLOG_SEQUENTIAL_READ: u32 = 0x0001;
pub const EVENTLOG_BACKWARDS_READ: u32 = 0x0008;

pub const ERROR_SUCCESS: i32 = 0;
pub const ERROR_FILE_NOT_FOUND: i32 = 2;
pub const ERROR_MORE_DATA: i32 = 234;
