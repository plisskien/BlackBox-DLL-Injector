use std::ffi::c_void;
pub type HINTERNET = *mut c_void;

#[link(name = "wininet")]
extern "system" {
    pub fn InternetOpenW(
        agent: *const u16,
        access_type: u32,
        proxy: *const u16,
        proxy_bypass: *const u16,
        flags: u32,
    ) -> HINTERNET;
    pub fn InternetOpenUrlW(
        session: HINTERNET,
        url: *const u16,
        headers: *const u16,
        headers_len: u32,
        flags: u32,
        context: usize,
    ) -> HINTERNET;
    pub fn InternetReadFile(
        file: HINTERNET,
        buffer: *mut c_void,
        bytes_to_read: u32,
        bytes_read: *mut u32,
    ) -> i32;
    pub fn InternetCloseHandle(handle: HINTERNET) -> i32;
}
