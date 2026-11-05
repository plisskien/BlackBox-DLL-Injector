use std::ffi::c_void;

#[link(name = "urlmon")]
extern "system" {
    /// HRESULT (0 = S_OK)
    pub fn URLDownloadToFileW(
        pCaller: *mut c_void,
        szURL: *const u16,
        szFileName: *const u16,
        dwReserved: u32,
        lpfnCB: *mut c_void,
    ) -> i32;
}
