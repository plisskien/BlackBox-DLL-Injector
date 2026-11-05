//! VC++ 2015-2022 runtime detection.
//!
//! The injector itself is linked against the static C runtime (`+crt-static`), so it starts on
//! a clean Windows install with no redistributable at all. Injected DLLs built with MSVC may
//! still need the runtime, so the CLI can report what the machine has and start an install.

use crate::util::wstr;
use crate::win32::advapi32::*;
use crate::win32::shell32::{ShellExecuteW, SW_SHOWNORMAL};
use crate::win32::*;
use crate::win32::user32::MSG;
use crate::win32::urlmon::URLDownloadToFileW;
use crate::process::Arch;

const RUNTIMES: &str = "SOFTWARE\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes";

#[derive(Clone, Debug, Default)]
pub struct RedistStatus {
    /// Version of the x64 runtime, when installed
    pub x64: Option<String>,
    /// Version of the x86 runtime, when installed
    pub x86: Option<String>,
}

impl RedistStatus {
    #[allow(dead_code)]
    pub fn x64_installed(&self) -> bool {
        self.x64.is_some()
    }
}

/// Extra registry access bit for the 32-bit view (the x86 redistributable is a 32-bit
/// installer and writes its key under WOW6432Node, which the 64-bit view does not show).
const KEY_WOW64_32KEY: u32 = 0x0200;
const KEY_WOW64_64KEY: u32 = 0x0100;

/// Looks for the VC++ 2015-2022 runtime: both registry views of the redistributable key,
/// then the runtime DLL itself (a machine can run a VC++ DLL that was installed by a game
/// or by Windows Update without the redistributable key being present).
pub fn detect() -> RedistStatus {
    RedistStatus {
        x64: detect_one("x64", "System32"),
        x86: detect_one("x86", "SysWOW64"),
    }
}

fn detect_one(arch_key: &str, system_dir: &str) -> Option<String> {
    let path = format!("{RUNTIMES}\\{arch_key}");
    // 64-bit view first (what the x64 redist writes), then the 32-bit view (x86 redist).
    for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
        if let Some(v) = unsafe { read_runtime(&path, view) } {
            return Some(v);
        }
    }
    file_runtime(system_dir)
}

/// The runtime DLLs themselves, if present in the Windows system directory.
fn file_runtime(system_dir: &str) -> Option<String> {
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let dir = format!("{windir}\\{system_dir}");
    let dll = format!("{dir}\\vcruntime140.dll");
    if !std::path::Path::new(&dll).is_file() {
        return None;
    }
    let alt = format!("{dir}\\vcruntime140_1.dll");
    let extra = if std::path::Path::new(&alt).is_file() { " (+_1)" } else { "" };
    Some(format!("installed (runtime DLL present{extra})"))
}

/// True when this build has no C runtime DLL dependency (the default for release builds).
pub fn injector_uses_static_crt() -> bool {
    cfg!(target_feature = "crt-static")
}

unsafe fn read_runtime(path: &str, view: u32) -> Option<String> {
    let mut key: HKEY = std::ptr::null_mut();
    let wpath = wstr(path);
    if RegOpenKeyExW(HKEY_LOCAL_MACHINE, wpath.as_ptr(), 0, KEY_READ | view, &mut key) != ERROR_SUCCESS {
        return None;
    }
    let installed = read_dw(key, "Installed").unwrap_or(0) == 1;
    let version = read_sz(key, "Version");
    let _ = RegCloseKey(key);
    if !installed {
        return None;
    }
    Some(version.unwrap_or_else(|| "installed".to_string()))
}

unsafe fn read_dw(key: HKEY, name: &str) -> Option<u32> {
    let wname = wstr(name);
    let mut kind = 0u32;
    let mut val = 0u32;
    let mut size = 4u32;
    if RegQueryValueExW(key, wname.as_ptr(), std::ptr::null_mut(), &mut kind, &mut val as *mut u32 as *mut u8, &mut size) == ERROR_SUCCESS
        && kind == REG_DWORD
    {
        Some(val)
    } else {
        None
    }
}

unsafe fn read_sz(key: HKEY, name: &str) -> Option<String> {
    let wname = wstr(name);
    let mut kind = 0u32;
    let mut size = 0u32;
    if RegQueryValueExW(key, wname.as_ptr(), std::ptr::null_mut(), &mut kind, std::ptr::null_mut(), &mut size) != ERROR_SUCCESS
        || (kind != REG_SZ && kind != REG_EXPAND_SZ)
        || size < 2
    {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    if RegQueryValueExW(key, wname.as_ptr(), std::ptr::null_mut(), &mut kind, buf.as_mut_ptr(), &mut size) != ERROR_SUCCESS {
        return None;
    }
    let u16s: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some(String::from_utf16_lossy(&u16s).trim_end_matches('\0').to_string())
}

/// Downloads page used when winget is not available.
pub const DOWNLOAD_URL: &str = "https://aka.ms/vs/17/release/vc_redist.x64.exe";
pub const DOWNLOAD_URL_X86: &str = "https://aka.ms/vs/17/release/vc_redist.x86.exe";


fn run_installer(path: &std::path::Path) -> bool {
    let mut child = match std::process::Command::new(path)
        .args(["/install", "/quiet", "/norestart"])
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        match child.try_wait() {
            Ok(Some(st)) => return st.success(),
            Ok(None) => {}
            Err(_) => return false,
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

fn install_package(arch: Arch, package: &str, url: &str) -> String {
    let which = match arch { Arch::X64 => "x64", Arch::X86 => "x86" };
    // Offline-friendly: a vc_redist next to the injector is used first.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let local = dir.join(format!("vc_redist.{which}.exe"));
            if local.is_file() {
                let ok = run_installer(&local);
                if ok && installed(arch) {
                    return format!("installed from the bundled vc_redist.{which}.exe");
                }
            }
        }
    }
    let winget = std::process::Command::new("winget")
        .args([
            "install",
            "--id",
            package,
            "-e",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])
        .output();
    if let Ok(out) = winget {
        if out.status.success() {
            return format!("installed with winget ({package})");
        }
        let err = String::from_utf8_lossy(&out.stderr);
        let hint = err.lines().last().unwrap_or("").trim();
        if !hint.is_empty() {
            eprintln!("winget: {hint}");
        }
    }

    // Direct fallback: download the official installer and run it silently.
    let dir = std::env::temp_dir().join("BlackBoxInjector");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(format!("vc_redist.{which}.exe"));
    let wurl = wstr(url);
    let wfile = wstr(&file.display().to_string());
    let hr = unsafe { URLDownloadToFileW(std::ptr::null_mut(), wurl.as_ptr(), wfile.as_ptr(), 0, std::ptr::null_mut()) };
    if hr == 0 {
        let ok = run_installer(&file);
        if ok && installed(arch) {
            let _ = std::fs::remove_file(&file);
            return format!("installed silently from the Microsoft vc_redist {which} package");
        }
    }

    let op = wstr("open");
    let wurl = wstr(url);
    let rc = unsafe { ShellExecuteW(std::ptr::null_mut(), op.as_ptr(), wurl.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
    if rc as isize > 32 {
        format!("opened {url} in the browser - run it to install the {which} runtime")
    } else {
        format!("could not start an install; download {url} manually")
    }
}

/// Tries winget first, then falls back to opening the Microsoft download page.
/// Returns a human-readable result line.
pub fn install_x64() -> String {
    install_package(Arch::X64, "Microsoft.VCRedist.2015+.x64", DOWNLOAD_URL)
}

pub fn install_x86() -> String {
    install_package(Arch::X86, "Microsoft.VCRedist.2015+.x86", DOWNLOAD_URL_X86)
}

#[allow(dead_code)]
pub fn install_for_arch(arch: Arch) -> String {
    match arch {
        Arch::X64 => install_x64(),
        Arch::X86 => install_x86(),
    }
}

/// Interactive installation requested by the GUI prompt:
/// 1. Downloads the official Microsoft vc_redist package to %TEMP%\BlackBoxInjector\
/// 2. Executes the installer with GUI so the user sees the setup window and accepts license
/// 3. Monitors execution status, waits for exit, logs errors, and verifies installation.
unsafe fn find_installer_window(pid: u32) -> Option<HWND> {
    unsafe extern "system" fn enum_proc(hwnd: HWND, lp: LPARAM) -> BOOL {
        let pair = &mut *(lp as *mut (u32, Option<HWND>));
        if user32::IsWindowVisible(hwnd) != 0 {
            let mut w_pid = 0u32;
            user32::GetWindowThreadProcessId(hwnd, &mut w_pid);
            if w_pid == pair.0 {
                pair.1 = Some(hwnd);
                return 0; // found
            }
            // Fallback match: if Burn launched a child process with Visual C++ title
            let mut buf = [0u16; 256];
            let len = user32::GetWindowTextW(hwnd, buf.as_mut_ptr(), 256);
            if len > 0 {
                let title = String::from_utf16_lossy(&buf[..len as usize]);
                if title.contains("Visual C++") && (title.contains("Redistributable") || title.contains("Setup")) {
                    pair.1 = Some(hwnd);
                    return 0;
                }
            }
        }
        1
    }
    let mut data: (u32, Option<HWND>) = (pid, None);
    user32::EnumWindows(Some(enum_proc), &mut data as *mut _ as LPARAM);
    data.1
}

pub fn install_interactive_for_arch<F: FnMut(&str)>(arch: Arch, mut log: F) -> Result<(), String> {
    let (which, url) = match arch {
        Arch::X64 => ("x64", DOWNLOAD_URL),
        Arch::X86 => ("x86", DOWNLOAD_URL_X86),
    };

    let dir = std::env::temp_dir().join("BlackBoxInjector");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(format!("vc_redist.{which}.exe"));

    // Check if installer is already downloaded in temp
    if !file.is_file() {
        log(&format!("[*] Downloading Microsoft Visual C++ 2015-2022 {which} redistributable..."));
        let wurl = wstr(url);
        let wfile = wstr(&file.display().to_string());
        let hr = unsafe { URLDownloadToFileW(std::ptr::null_mut(), wurl.as_ptr(), wfile.as_ptr(), 0, std::ptr::null_mut()) };
        if hr != 0 {
            let err = format!("Failed to download from Microsoft CDN (HRESULT 0x{hr:08X}). Please install manually from {url}");
            log(&format!("[!] {err}"));
            return Err(err);
        }
    } else {
        log(&format!("[*] Using cached Microsoft installer from temp: {}", file.display()));
    }

    log("[*] Opening Microsoft Visual C++ setup window...");
    log("[*] Please check 'I agree to the license terms' and click Install in the Microsoft dialog.");

    // Launch with GUI so user accepts terms and clicks Install
    let mut child = match std::process::Command::new(&file)
        .arg("/install")
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let err = format!("Failed to launch installer process: {e}");
            log(&format!("[!] {err}"));
            return Err(err);
        }
    };

    let child_pid = child.id();
    // Explicitly allow the installer process to take foreground focus immediately
    unsafe {
        user32::AllowSetForegroundWindow(child_pid);
        user32::AllowSetForegroundWindow(user32::ASFW_ANY);
    }

    // Monitor the process with a 10-minute timeout while keeping BlackBox UI responsive
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
    let mut brought_to_front = false;
    let code = loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                break st.code().unwrap_or(-1);
            }
            Ok(None) => {}
            Err(e) => {
                let err = format!("Error waiting for installer process: {e}");
                log(&format!("[!] {err}"));
                return Err(err);
            }
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let err = "Installation timed out after 10 minutes.".to_string();
            log(&format!("[!] {err}"));
            return Err(err);
        }

        // Bring the Microsoft setup window to the front as soon as it appears
        if !brought_to_front {
            if let Some(wnd) = unsafe { find_installer_window(child_pid) } {
                unsafe {
                    user32::ShowWindow(wnd, SW_SHOWNORMAL);
                    user32::BringWindowToTop(wnd);
                    user32::SetForegroundWindow(wnd);
                }
                brought_to_front = true;
            }
        }

        // Pump Windows messages so the BlackBox main window stays fully responsive and never shows "Not Responding"
        unsafe {
            let mut msg = MSG::default();
            while user32::PeekMessageW(&mut msg, NULL_HANDLE, 0, 0, user32::PM_REMOVE) != 0 {
                if msg.message == user32::WM_QUIT {
                    let _ = child.kill();
                    return Err("Application closed during installation.".into());
                }
                user32::TranslateMessage(&msg);
                user32::DispatchMessageW(&msg);
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
    };

    match code {
        0 => {
            log(&format!("[+] Microsoft Visual C++ {which} runtime installed successfully."));
        }
        3010 => {
            log(&format!("[+] Microsoft Visual C++ {which} runtime installed (reboot recommended)."));
        }
        1602 => {
            let err = "Installation cancelled by user in Microsoft setup.".to_string();
            log(&format!("[-] {err}"));
            return Err(err);
        }
        1618 => {
            let err = "Another MSI installation is already running on this system. Finish or close it, then try again.".to_string();
            log(&format!("[!] Error 1618: {err}"));
            return Err(err);
        }
        1603 => {
            let err = "Fatal error during MSI installation (check Windows Application Event Log).".to_string();
            log(&format!("[!] Error 1603: {err}"));
            return Err(err);
        }
        _ => {
            let err = format!("Installer exited with error code {code} (0x{code:08X})");
            log(&format!("[!] {err}"));
            return Err(err);
        }
    }

    // Verify whether runtime DLLs and registry entries are present
    let verified = {
        let s = detect();
        match arch {
            Arch::X64 => s.x64.is_some(),
            Arch::X86 => s.x86.is_some(),
        }
    };
    if verified {
        log(&format!("[+] System check verified: VC++ {which} runtime is now active."));
        Ok(())
    } else {
        let err = format!("Installer completed with code {code}, but VC++ {which} runtime was not detected.");
        log(&format!("[-] {err}"));
        Err(err)
    }
}

pub fn is_installed(arch: Arch) -> bool {
    let s = detect();
    match arch {
        Arch::X64 => s.x64.is_some(),
        Arch::X86 => s.x86.is_some(),
    }
}

fn installed(arch: Arch) -> bool {
    is_installed(arch)
}

/// Automatic install used before injecting: only touches the runtime matching the target.
/// Reports the runtime state of the target's bitness before an injection. It never installs
/// anything: silently downloading/installing software (or opening a browser) as a side effect
/// of pressing "Inject" is not acceptable. `--install-redist` remains the explicit path.
pub fn ensure_for_arch(arch: Arch, log: &dyn Fn(&str)) {
    if installed(arch) {
        return;
    }
    let which = match arch { Arch::X64 => "x64", Arch::X86 => "x86" };
    log(&format!(
        "[!] VC++ {which} runtime was not detected - an MSVC-built DLL may fail to load. \
         Run --install-redist (or install the redistributable) if it does."
    ));
}

/// Human-readable status lines shared by the CLI and the About dialog.
pub fn report_lines() -> Vec<String> {
    let s = detect();
    let mut lines = Vec::new();
    if injector_uses_static_crt() {
        lines.push("Injector runtime: static C runtime (no VC++ redistributable needed)".to_string());
    } else {
        lines.push("Injector runtime: dynamic C runtime (VC++ 2015-2022 x64 recommended)".to_string());
    }
    lines.push(match &s.x64 {
        Some(v) => format!("VC++ x64 runtime: installed ({v})"),
        None => "VC++ x64 runtime: not installed".to_string(),
    });
    lines.push(match &s.x86 {
        Some(v) => format!("VC++ x86 runtime: installed ({v})"),
        None => "VC++ x86 runtime: not installed".to_string(),
    });
    lines
}

