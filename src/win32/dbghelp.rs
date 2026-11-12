//! Minimal DbgHelp surface used to resolve internal ntdll symbols from a local PDB.
//!
//! `dbghelp.dll` is present on every supported Windows install. The PDB itself is placed in a
//! local symbol-server-style cache (`<root>\\ntdll.pdb\\<GUID><AGE>\\ntdll.pdb`) by the caller.

use super::*;
use std::ffi::c_void;

pub const SYMOPT_UNDNAME: u32 = 0x0000_0002;
pub const SYMOPT_DEFERRED_LOADS: u32 = 0x0000_0004;

/// `SYMBOL_INFOW` for x64. `Reserved` is `ULONG64[2]`, so the layout is 88 bytes and `Name`
/// starts at offset 84 (see the Windows SDK `DbgHelp.h`).
#[repr(C)]
pub struct SymbolInfoW {
    pub size_of_struct: u32,
    pub type_index: u32,
    pub reserved: [u64; 2],
    pub index: u32,
    pub size: u32,
    pub mod_base: u64,
    pub flags: u32,
    pub value: u64,
    pub address: u64,
    pub register: u32,
    pub scope: u32,
    pub tag: u32,
    pub name_len: u32,
    pub max_name_len: u32,
    pub name: [u16; 512],
}

impl Default for SymbolInfoW {
    fn default() -> Self {
        SymbolInfoW {
            size_of_struct: std::mem::size_of::<SymbolInfoW>() as u32,
            type_index: 0,
            reserved: [0; 2],
            index: 0,
            size: 0,
            mod_base: 0,
            flags: 0,
            value: 0,
            address: 0,
            register: 0,
            scope: 0,
            tag: 0,
            name_len: 0,
            max_name_len: 512,
            name: [0; 512],
        }
    }
}

#[link(name = "dbghelp")]
extern "system" {
    pub fn SymSetOptions(options: u32) -> u32;
    pub fn SymInitializeW(hProcess: HANDLE, UserSearchPath: *const u16, fInvadeProcess: BOOL) -> BOOL;
    pub fn SymSetSearchPathW(hProcess: HANDLE, SearchPath: *const u16) -> BOOL;
    pub fn SymLoadModuleExW(
        hProcess: HANDLE,
        hFile: HANDLE,
        ImageName: *const u16,
        ModuleName: *const u16,
        BaseOfDll: u64,
        DllSize: u32,
        Data: *mut c_void,
        Flags: u32,
    ) -> u64;
    pub fn SymFromNameW(hProcess: HANDLE, Name: *const u16, Symbol: *mut SymbolInfoW) -> BOOL;
    pub fn SymCleanup(hProcess: HANDLE) -> BOOL;
}
