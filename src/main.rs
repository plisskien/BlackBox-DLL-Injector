#![windows_subsystem = "windows"]
#![allow(non_snake_case, non_camel_case_types)]

pub mod version;
mod cli;
mod config;
mod diag;
mod inject;
mod update;
mod pe;
mod process;
mod queue;
mod ui;
mod util;
mod vcredist;
mod win32;
mod worker;

fn main() {
    // Verify elevation token (fallback if manifest is bypassed).
    #[cfg(not(feature = "no_elevation"))]
    if !process::is_elevated() {
        if cli::is_cli_request() {
            cli::ensure_console();
            eprintln!("BlackBox Injector needs Administrator rights. Start it from an elevated command prompt.");
        } else {
            unsafe {
                let text = util::wstr(
                    "BlackBox Injector needs Administrator rights.\r\n\r\nClose it, start the exe again and accept the UAC prompt.",
                );
                let caption = util::wstr("Administrator required");
                crate::win32::user32::MessageBoxW(
                    std::ptr::null_mut(),
                    text.as_ptr(),
                    caption.as_ptr(),
                    0x10, // MB_ICONERROR
                );
            }
        }
        std::process::exit(1);
    }

    // Dispatch CLI mode if launched with command-line arguments.
    if cli::is_cli_request() {
        std::process::exit(cli::run());
    }
    std::process::exit(ui::run_gui());
}
