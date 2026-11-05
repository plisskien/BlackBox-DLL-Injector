//! Headless CLI mode.
//!
//! Two jobs: drive the injection engine (used by the test harness) and let everything the GUI
//! can do be done from a command prompt - build the injection queue, set order/delays, pick the
//! method and options, switch between global and per-process setups, inspect the result, and
//! inject. Changes are saved unless `--no-save` is given.

use crate::config::{Module, Settings, SettingsScope};
use crate::diag;
use crate::vcredist;
use crate::win32::kernel32;

const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
const MAX_DELAY_MS: u32 = 3_600_000;

/// No arguments starts the GUI; any argument at all runs the command line.
pub fn is_cli_request() -> bool {
    std::env::args_os().len() > 1
}

/// Write to the parent console (windows subsystem has no stdout by default).
/// Also mirrors every line to cli_log.txt so results are verifiable when no console handle
/// exists (e.g. piped/redirected runs).
fn out(line: &str) {
    unsafe {
        let h = kernel32::GetStdHandle(STD_OUTPUT_HANDLE);
        if !h.is_null() && h != crate::win32::INVALID_HANDLE_VALUE {
            let b = format!("{line}\r\n").into_bytes();
            let mut written = 0u32;
            kernel32::WriteFile(h, b.as_ptr() as _, b.len() as u32, &mut written, std::ptr::null_mut());
        }
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("cli_log.txt") {
        use std::io::Write as _;
        let _ = writeln!(f, "{line}");
    }
}

const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
const STD_ERROR_HANDLE: u32 = -12i32 as u32;

/// Connects the process to the calling console or allocates one so CLI output is visible.
pub fn ensure_console() {
    unsafe {
        if kernel32::AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            kernel32::AllocConsole();
        }
        let h_out = kernel32::GetStdHandle(STD_OUTPUT_HANDLE);
        if h_out.is_null() || h_out == crate::win32::INVALID_HANDLE_VALUE {
            let conout = crate::util::wstr("CONOUT$");
            let h = crate::win32::kernel32::CreateFileW(
                conout.as_ptr(),
                0x4000_0000 /* GENERIC_WRITE */,
                kernel32::FILE_SHARE_READ | kernel32::FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                kernel32::OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );
            if h != crate::win32::INVALID_HANDLE_VALUE {
                kernel32::SetStdHandle(STD_OUTPUT_HANDLE, h);
            }
        }
        let h_err = kernel32::GetStdHandle(STD_ERROR_HANDLE);
        if h_err.is_null() || h_err == crate::win32::INVALID_HANDLE_VALUE {
            let conout = crate::util::wstr("CONOUT$");
            let h = crate::win32::kernel32::CreateFileW(
                conout.as_ptr(),
                0x4000_0000 /* GENERIC_WRITE */,
                kernel32::FILE_SHARE_READ | kernel32::FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                kernel32::OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );
            if h != crate::win32::INVALID_HANDLE_VALUE {
                kernel32::SetStdHandle(STD_ERROR_HANDLE, h);
            }
        }
    }
}

/// One change to the injection setup, applied in command-line order.
#[derive(Debug)]
enum Op {
    Add { path: String, enabled: bool },
    Remove(String),
    Clear,
    Enable { target: String, on: bool },
    Move { from: String, to: String },
    Up(String),
    Down(String),
    SetDelay { target: String, ms: u32 },
    Method(u32),
    StartDelay(u32),
    BetweenDelay(u32),
    AutoInject(bool),
    CloseOnInject(bool),
    ErasePe(bool),
    HideModule(bool),
    Stealth(bool),
    HideDebugger(bool),
    ManualImports(bool),
    DisableSeh(bool),
    DisableException(bool),
    Scramble(Option<String>),
    Scope(bool),
    UiScale(u32),
    Forget(String),
    FileTarget(String),
    FileMethod(Option<u32>),
    FileWait(String),
    FileAfter(u32),
    FileIgnore(bool),
    FileRequireModule(String),
    FileRequireWindow(bool),
    AutoWaitWindow(bool),
    AutoRetry(bool),
    InjectChildren(bool),
    AutoChildFilter(String),
    AutoWindowTitle(String),
}

#[derive(Default)]
struct Command {
    ops: Vec<Op>,
    profile: Option<String>,
    pid: Option<u32>,
    proc_name: Option<String>,
    /// `--dll` files for one-off injection, with the delay of the next `--dll`
    adhoc: Vec<(String, u32)>,
    wait_secs: u32,
    list: bool,
    /// `--ps` / `--processes`: print every running process and exit.
    ps: bool,
    save: bool,
    no_save: bool,
    inject: bool,
    check_redist: bool,
    install_redist: bool,
    validate: bool,
    inspect: Option<String>,
    unload: Option<String>,
    wipe_headers: bool,
    retry: u32,
    help: bool,
    version: bool,
    errors: Vec<String>,
}

fn parse(args: &[String]) -> Command {
    let mut cmd = Command::default();
    // `--dll-delay` applies to the next `--dll`
    let mut pending_dll_delay = 0u32;
    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--cli" => {}
            "--help" | "-h" | "/?" => cmd.help = true,
            "--version" | "-v" => cmd.version = true,
            "--list" => cmd.list = true,
            "--ps" | "--processes" | "--procs" => cmd.ps = true,
            "--save" => cmd.save = true,
            "--no-save" => cmd.no_save = true,
            "--inject" => cmd.inject = true,
            "--validate" => cmd.validate = true,
            "--inspect" => match take_str(args, &mut i) {
                Some(v) => cmd.inspect = Some(v),
                None => cmd.errors.push("--inspect needs a DLL path".to_string()),
            },
            "--check-redist" => cmd.check_redist = true,
            "--install-redist" => cmd.install_redist = true,
            "--wipe-headers" => cmd.wipe_headers = true,
            "--retry" => match take_num(args, &mut i) {
                Some(v) => cmd.retry = v.clamp(0, 10) as u32,
                None => cmd.errors.push("--retry needs a number".to_string()),
            },
            "--unload" => match take_str(args, &mut i) {
                Some(v) => cmd.unload = Some(v),
                None => cmd.errors.push("--unload needs a module name or path".to_string()),
            },
            "--file" => match take_str(args, &mut i) {
                Some(v) => cmd.ops.push(Op::FileTarget(v)),
                None => cmd.errors.push("--file needs a row number, path or file name".to_string()),
            },
            "--file-method" => match take_str(args, &mut i) {
                Some(v) if v.eq_ignore_ascii_case("global") || v.eq_ignore_ascii_case("auto") || v.is_empty() => {
                    cmd.ops.push(Op::FileMethod(None))
                }
                Some(v) => match v.parse::<u32>() {
                    Ok(n) if n <= 5 => cmd.ops.push(Op::FileMethod(Some(n))),
                    _ => cmd.errors.push("--file-method needs 0-5, global or auto".to_string()),
                },
                None => cmd.errors.push("--file-method needs 0-5, global or auto".to_string()),
            },
            "--file-wait" => match take_str(args, &mut i) {
                Some(v) => cmd.ops.push(Op::FileWait(v)),
                None => cmd.errors.push("--file-wait needs a module name".to_string()),
            },
            "--file-after" => match take_num(args, &mut i) {
                Some(v) => cmd.ops.push(Op::FileAfter(v.max(0) as u32)),
                None => cmd.errors.push("--file-after needs milliseconds".to_string()),
            },
            "--file-ignore" => {
                let v = take_bool(args, &mut i);
                cmd.ops.push(Op::FileIgnore(v));
            }
            "--file-if-module" => match take_str(args, &mut i) {
                Some(v) => cmd.ops.push(Op::FileRequireModule(v)),
                None => cmd.errors.push("--file-if-module needs a module name".to_string()),
            },
            "--file-if-window" => {
                let v = take_bool(args, &mut i);
                cmd.ops.push(Op::FileRequireWindow(v));
            }
            "--auto-child-filter" => match take_str(args, &mut i) {
                Some(v) => cmd.ops.push(Op::AutoChildFilter(v)),
                None => cmd.errors.push("--auto-child-filter needs text".to_string()),
            },
            "--auto-window-title" => match take_str(args, &mut i) {
                Some(v) => cmd.ops.push(Op::AutoWindowTitle(v)),
                None => cmd.errors.push("--auto-window-title needs text".to_string()),
            }
            "--auto-wait-window" => {
                let v = take_bool(args, &mut i);
                cmd.ops.push(Op::AutoWaitWindow(v));
            }
            "--auto-retry" => {
                let v = take_bool(args, &mut i);
                cmd.ops.push(Op::AutoRetry(v));
            }
            "--inject-children" => {
                let v = take_bool(args, &mut i);
                cmd.ops.push(Op::InjectChildren(v));
            }
            "--pid" => match take_num(args, &mut i) {
                Some(v) => cmd.pid = Some(v as u32),
                None => cmd.errors.push("--pid needs a number".to_string()),
            },
            "--proc" | "--process" => match take_str(args, &mut i) {
                Some(v) => cmd.proc_name = Some(v),
                None => cmd.errors.push("--proc needs a process name".to_string()),
            },
            "--profile" => match take_str(args, &mut i) {
                Some(v) => cmd.profile = Some(v),
                None => cmd.errors.push("--profile needs a process name".to_string()),
            },
            "--forget" => match take_str(args, &mut i) {
                Some(v) => cmd.ops.push(Op::Forget(v)),
                None => cmd.errors.push("--forget needs a process name".to_string()),
            },
            "--add" => match take_str(args, &mut i) {
                Some(v) => cmd.ops.push(Op::Add { path: v, enabled: true }),
                None => cmd.errors.push("--add needs a DLL path".to_string()),
            },
            "--add-disabled" => match take_str(args, &mut i) {
                Some(v) => cmd.ops.push(Op::Add { path: v, enabled: false }),
                None => cmd.errors.push("--add-disabled needs a DLL path".to_string()),
            },
            "--remove" => match take_str(args, &mut i) {
                Some(v) => cmd.ops.push(Op::Remove(v)),
                None => cmd.errors.push("--remove needs an index or path".to_string()),
            },
            "--clear" => cmd.ops.push(Op::Clear),
            "--enable" => match take_str(args, &mut i) {
                Some(v) => cmd.ops.push(Op::Enable { target: v, on: true }),
                None => cmd.errors.push("--enable needs an index or path".to_string()),
            },
            "--disable" => match take_str(args, &mut i) {
                Some(v) => cmd.ops.push(Op::Enable { target: v, on: false }),
                None => cmd.errors.push("--disable needs an index or path".to_string()),
            },
            "--move" => {
                let from = take_str(args, &mut i);
                let to = take_str(args, &mut i);
                match (from, to) {
                    (Some(f), Some(t)) => cmd.ops.push(Op::Move { from: f, to: t }),
                    _ => cmd.errors.push("--move needs <from> <to>".to_string()),
                }
            }
            "--up" => match take_str(args, &mut i) {
                Some(v) => cmd.ops.push(Op::Up(v)),
                None => cmd.errors.push("--up needs an index or path".to_string()),
            },
            "--down" => match take_str(args, &mut i) {
                Some(v) => cmd.ops.push(Op::Down(v)),
                None => cmd.errors.push("--down needs an index or path".to_string()),
            },
            "--set-delay" => {
                let target = take_str(args, &mut i);
                let ms = take_num(args, &mut i);
                match (target, ms) {
                    (Some(t), Some(m)) => cmd.ops.push(Op::SetDelay { target: t, ms: m as u32 }),
                    _ => cmd.errors.push("--set-delay needs <index|path> <ms>".to_string()),
                }
            }
            "--method" => match take_num(args, &mut i) {
                Some(v) => cmd.ops.push(Op::Method(v as u32)),
                None => cmd.errors.push("--method needs a number 0-5".to_string()),
            },
            "--delay" => match take_num(args, &mut i) {
                Some(v) => cmd.ops.push(Op::StartDelay(v as u32)),
                None => cmd.errors.push("--delay needs milliseconds".to_string()),
            },
            "--delay-between" => match take_num(args, &mut i) {
                Some(v) => cmd.ops.push(Op::BetweenDelay(v as u32)),
                None => cmd.errors.push("--delay-between needs milliseconds".to_string()),
            },
            "--auto-inject" => cmd.ops.push(Op::AutoInject(take_bool(args, &mut i))),
            "--close-on-inject" => cmd.ops.push(Op::CloseOnInject(take_bool(args, &mut i))),
            "--erase-pe" => cmd.ops.push(Op::ErasePe(take_bool(args, &mut i))),
            "--hide-module" => cmd.ops.push(Op::HideModule(take_bool(args, &mut i))),
            "--stealth" => cmd.ops.push(Op::Stealth(take_bool(args, &mut i))),
            "--hide-debugger" => cmd.ops.push(Op::HideDebugger(take_bool(args, &mut i))),
            "--manual-imports" => cmd.ops.push(Op::ManualImports(take_bool(args, &mut i))),
            "--disable-seh" => cmd.ops.push(Op::DisableSeh(take_bool(args, &mut i))),
            "--disable-exception" | "--disable-exceptions" => {
                cmd.ops.push(Op::DisableException(take_bool(args, &mut i)))
            }
            "--scramble" => match take_str(args, &mut i) {
                Some(v) => cmd.ops.push(Op::Scramble(Some(v))),
                None => cmd.errors.push("--scramble needs a preset or 'none'".to_string()),
            },
            "--scope" => match take_str(args, &mut i).map(|v| v.to_ascii_lowercase()) {
                Some(v) if v == "global" => cmd.ops.push(Op::Scope(false)),
                Some(v) if v == "per-process" || v == "per_process" || v == "process" => {
                    cmd.ops.push(Op::Scope(true))
                }
                _ => cmd.errors.push("--scope needs 'global' or 'per-process'".to_string()),
            },
            "--ui-scale" => match take_num(args, &mut i) {
                Some(v) => cmd.ops.push(Op::UiScale(v.max(0) as u32)),
                None => cmd.errors.push("--ui-scale needs a percentage (0 = system DPI)".to_string()),
            },
            "--dll" => match take_str(args, &mut i) {
                Some(v) => {
                    cmd.adhoc.push((v, pending_dll_delay));
                    pending_dll_delay = 0;
                }
                None => cmd.errors.push("--dll needs a path".to_string()),
            },
            "--dll-delay" => match take_num(args, &mut i) {
                Some(ms) => pending_dll_delay = ms.max(0) as u32,
                None => cmd.errors.push("--dll-delay needs milliseconds".to_string()),
            },
            "--wait" => cmd.wait_secs = take_num(args, &mut i).unwrap_or(0).max(0) as u32,
            other => cmd.errors.push(format!("unknown argument: {other}")),
        }
        i += 1;
    }
    cmd
}

fn take_str(args: &[String], i: &mut usize) -> Option<String> {
    let v = args.get(*i + 1)?.clone();
    *i += 1;
    Some(v)
}

fn take_num(args: &[String], i: &mut usize) -> Option<i64> {
    let v = args.get(*i + 1)?.parse().ok()?;
    *i += 1;
    Some(v)
}

/// Flags like `--erase-pe` take an optional `on`/`off` (default: on).
fn take_bool(args: &[String], i: &mut usize) -> bool {
    if let Some(v) = args.get(*i + 1) {
        match v.to_ascii_lowercase().as_str() {
            "on" | "true" | "1" | "yes" => {
                *i += 1;
                return true;
            }
            "off" | "false" | "0" | "no" => {
                *i += 1;
                return false;
            }
            _ => {}
        }
    }
    true
}

pub fn run() -> i32 {
    ensure_console();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = parse(&args);

    if cmd.help {
        print_help();
        return 0;
    }
    if cmd.version {
        out(&format!("BlackBox Injector v{}", crate::version::VERSION));
        return 0;
    }
    if cmd.ps {
        print_process_list();
        return 0;
    }
    // Anything we did not understand: say what, then print the whole command list
    if !cmd.errors.is_empty() {
        for e in &cmd.errors {
            out(&format!("[!] {e}"));
        }
        out("");
        out("Run with --help to see every command, or start BlackBoxInjector with no arguments for the GUI.");
        out("");
        print_help();
        return 2;
    }
    let nothing_requested = cmd.ops.is_empty()
        && !cmd.list
        && !cmd.inject
        && cmd.adhoc.is_empty()
        && !cmd.check_redist
        && !cmd.install_redist
        && !cmd.validate
        && cmd.inspect.is_none()
        && cmd.unload.is_none()
        && cmd.profile.is_none()
        && cmd.proc_name.is_none()
        && cmd.pid.is_none();
    if nothing_requested {
        print_help();
        return 2;
    }

    // ---- VC++ runtime report / install ----
    if cmd.check_redist || cmd.install_redist {
        for line in vcredist::report_lines() {
            out(&format!("[*] {line}"));
        }
        if cmd.install_redist {
            let s = vcredist::detect();
            if s.x64.is_some() {
                out("[+] VC++ x64 runtime is already installed, nothing to do");
            } else {
                out("[*] Installing VC++ 2015-2022 x64 runtime...");
                out(&format!("[+] {}", vcredist::install_x64()));
            }
            if s.x86.is_some() {
                out("[+] VC++ x86 runtime is already installed, nothing to do");
            } else {
                out("[*] Installing VC++ 2015-2022 x86 runtime...");
                out(&format!("[+] {}", vcredist::install_x86()));
            }
        }
        let only_redist = cmd.ops.is_empty() && !cmd.list && !cmd.inject && cmd.adhoc.is_empty();
        if only_redist {
            return if cmd.errors.is_empty() { 0 } else { 2 };
        }
    }

    // ---- DLL details / import report ----
    if let Some(path) = &cmd.inspect {
        let pid = match (cmd.pid, &cmd.proc_name) {
            (Some(p), _) => p,
            (None, Some(name)) => crate::process::find_by_name(name).map(|p| p.pid).unwrap_or(0),
            _ => 0,
        };
        match diag::dll_report(path, pid) {
            Ok(lines) => {
                for line in lines {
                    out(&line);
                }
                return 0;
            }
            Err(e) => {
                out(&format!("[!] {e}"));
                return 1;
            }
        }
    }

    // ---- load, pick the profile, apply changes ----
    let mut settings = Settings::load();
    // One-time warnings are not shown in headless mode
    settings.warnings.manual_map = true;
    settings.warnings.ldrp_load_dll = true;
    settings.warnings.scramble = true;

    // `--profile name` edits that process' setup; in per-process mode `--proc name` does the same
    let mut active_profile = cmd.profile.clone();
    if active_profile.is_none() && settings.scope == SettingsScope::PerProcess {
        active_profile = cmd.proc_name.clone();
    }
    if let Some(name) = &active_profile {
        settings.process_name = name.clone();
        if settings.profile(name).is_some() {
            settings.apply_profile(name);
        }
    }

    let mut mutated = false;
    let mut errors = 0;
    let mut current_file: Option<usize> = None;
    for op in &cmd.ops {
        // Per-file edits apply to the row chosen by the last --file
        match op {
            Op::FileTarget(sel) => {
                match resolve_index(&settings.modules, sel) {
                    Some(i) => {
                        current_file = Some(i);
                        out(&format!("[*] editing {}", settings.modules[i].path));
                    }
                    None => {
                        out(&format!("[!] no row matches '{}'", sel));
                        errors += 1;
                    }
                }
                continue;
            }
            Op::FileMethod(v) => {
                if let Some(i) = current_file {
                    settings.modules[i].method = *v;
                    mutated = true;
                } else {
                    out("[!] use --file <row> before --file-method");
                    errors += 1;
                }
                continue;
            }
            Op::FileWait(v) => {
                if let Some(i) = current_file {
                    settings.modules[i].wait_for_module = v.trim().to_string();
                    mutated = true;
                } else {
                    out("[!] use --file <row> before --file-wait");
                    errors += 1;
                }
                continue;
            }
            Op::FileAfter(v) => {
                if let Some(i) = current_file {
                    settings.modules[i].delay_after_ms = (*v).min(3_600_000);
                    mutated = true;
                } else {
                    out("[!] use --file <row> before --file-after");
                    errors += 1;
                }
                continue;
            }
            Op::FileIgnore(v) => {
                if let Some(i) = current_file {
                    settings.modules[i].ignore_errors = *v;
                    mutated = true;
                } else {
                    out("[!] use --file <row> before --file-ignore");
                    errors += 1;
                }
                continue;
            }
            Op::FileRequireModule(v) => {
                if let Some(i) = current_file {
                    settings.modules[i].require_module = v.trim().to_string();
                    mutated = true;
                } else {
                    out("[!] use --file <row> before --file-if-module");
                    errors += 1;
                }
                continue;
            }
            Op::FileRequireWindow(v) => {
                if let Some(i) = current_file {
                    settings.modules[i].require_window = *v;
                    mutated = true;
                } else {
                    out("[!] use --file <row> before --file-if-window");
                    errors += 1;
                }
                continue;
            }
            _ => {}
        }
        if !apply_op(&mut settings, op) {
            errors += 1;
        } else {
            mutated = true;
        }
    }

    // Forgetting the profile that was being edited must not write it back on save
    if let Some(name) = &active_profile {
        let forgotten = cmd
            .ops
            .iter()
            .any(|op| matches!(op, Op::Forget(n) if n.eq_ignore_ascii_case(name)));
        if forgotten {
            active_profile = None;
        }
    }

    // ---- save ----
    if !cmd.no_save && (mutated || cmd.save || active_profile.is_some()) {
        if let Some(name) = &active_profile {
            settings.process_name = name.clone();
            settings.remember_profile(name);
        }
        settings.save();
        if mutated {
            out("[+] settings saved");
        }
    }

    if cmd.list {
        print_state(&settings, active_profile.as_deref());
    }

    // ---- validate / unload ----
    let mut validate_failed = false;
    if cmd.validate || cmd.unload.is_some() {
        let target_pid = match (cmd.pid, &cmd.proc_name) {
            (Some(p), _) => Some(p),
            (None, Some(name)) => crate::process::find_by_name(name).map(|p| p.pid),
            _ => None,
        };
        let Some(pid) = target_pid else {
            out("[!] No target process. Use --pid <n> or --proc <name.exe>");
            return 1;
        };
        if cmd.validate {
            let problems = crate::queue::validate(pid, &settings.modules);
            if problems.is_empty() {
                out("[+] the queue looks ready to inject");
            }
            for pr in &problems {
                out(&format!("[{}] {}", if pr.fatal { "!" } else { "*" }, pr.line()));
                validate_failed |= pr.fatal;
            }
        }
        if let Some(module) = &cmd.unload {
            let logger = |line: &str| out(line);
            match crate::queue::unload(pid, module, cmd.wipe_headers, &logger) {
                Ok(()) => out("[+] unloaded"),
                Err(e) => {
                    out(&format!("[!] unload failed: {e}"));
                    validate_failed = true;
                }
            }
        }
        let only_check = !cmd.inject && cmd.adhoc.is_empty();
        if only_check {
            return if validate_failed { 1 } else { 0 };
        }
    }

    // ---- inject ----
    let want_inject = cmd.inject || !cmd.adhoc.is_empty();
    if want_inject {
        return inject(&settings, &cmd);
    }

    if errors > 0 {
        2
    } else {
        0
    }
}

fn apply_op(settings: &mut Settings, op: &Op) -> bool {
    match op {
        Op::Add { path, enabled } => {
            // Store an absolute path so the saved queue works from any working directory
            let path = std::path::absolute(path)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| path.clone());
            if !std::path::Path::new(&path).is_file() {
                out(&format!("[!] file does not exist yet: {path}"));
            }
            if settings.modules.iter().any(|m| m.path.eq_ignore_ascii_case(&path)) {
                out(&format!("[!] already in the list: {path}"));
                return false;
            }
            let mut m = Module::new(&path);
            m.enabled = *enabled;
            settings.modules.push(m);
            true
        }
        Op::Remove(target) => match resolve_index(&settings.modules, target) {
            Some(i) => {
                settings.modules.remove(i);
                true
            }
            None => {
                out(&format!("[!] no such file in the list: {target}"));
                false
            }
        },
        Op::Clear => {
            settings.modules.clear();
            true
        }
        Op::Enable { target, on } => match resolve_index(&settings.modules, target) {
            Some(i) => {
                settings.modules[i].enabled = *on;
                true
            }
            None => {
                out(&format!("[!] no such file in the list: {target}"));
                false
            }
        },
        Op::Move { from, to } => {
            let (Some(f), Some(t)) = (resolve_index(&settings.modules, from), resolve_index(&settings.modules, to)) else {
                out("[!] --move needs two existing indexes or paths");
                return false;
            };
            let item = settings.modules.remove(f);
            let dest = t.min(settings.modules.len());
            settings.modules.insert(dest, item);
            true
        }
        Op::Up(target) => match resolve_index(&settings.modules, target) {
            Some(i) if i > 0 => {
                settings.modules.swap(i, i - 1);
                true
            }
            _ => false,
        },
        Op::Down(target) => match resolve_index(&settings.modules, target) {
            Some(i) if i + 1 < settings.modules.len() => {
                settings.modules.swap(i, i + 1);
                true
            }
            _ => false,
        },
        Op::SetDelay { target, ms } => match resolve_index(&settings.modules, target) {
            Some(i) => {
                settings.modules[i].delay_ms = (*ms).min(MAX_DELAY_MS);
                true
            }
            None => {
                out(&format!("[!] no such file in the list: {target}"));
                false
            }
        },
        Op::Method(m) => {
            settings.method = (*m).min(5);
            true
        }
        Op::StartDelay(ms) => {
            settings.delay = *ms;
            true
        }
        Op::BetweenDelay(ms) => {
            settings.delay_between = *ms;
            true
        }
        Op::AutoInject(v) => {
            settings.auto_inject = *v;
            true
        }
        Op::CloseOnInject(v) => {
            settings.close_on_inject = *v;
            true
        }
        Op::ErasePe(v) => {
            settings.erase_pe = *v;
            true
        }
        Op::HideModule(v) => {
            settings.hide_module = *v;
            true
        }
        Op::Stealth(v) => {
            settings.stealth_inject = *v;
            true
        }
        Op::HideDebugger(v) => {
            settings.advanced.hide_from_debugger = *v;
            true
        }
        Op::ManualImports(v) => {
            settings.advanced.manual_resolve_imports = *v;
            true
        }
        Op::DisableSeh(v) => {
            settings.advanced.disable_seh_validation = *v;
            true
        }
        Op::DisableException(v) => {
            settings.advanced.disable_exception_support = *v;
            true
        }
        Op::Scramble(preset) => {
            match preset.as_deref().unwrap_or("none") {
                "none" | "" => settings.scramble = Default::default(),
                other => apply_scramble_preset(&mut settings.scramble, other),
            }
            true
        }
        Op::Scope(per_process) => {
            settings.scope = if *per_process { SettingsScope::PerProcess } else { SettingsScope::Global };
            true
        }
        Op::UiScale(v) => {
            settings.ui_scale = if *v == 0 { 0 } else { (*v).clamp(75, 150) };
            true
        }
        Op::Forget(name) => {
            settings.forget_profile(name);
            settings.delete_profile_key(name);
            out(&format!("[+] forgot the saved setup for {name}"));
            true
        }
        Op::AutoWaitWindow(v) => {
            settings.auto_wait_window = *v;
            true
        }
        Op::AutoRetry(v) => {
            settings.auto_retry = *v;
            true
        }
        Op::InjectChildren(v) => {
            settings.inject_children = *v;
            true
        }
        Op::AutoChildFilter(v) => {
            settings.auto_child_filter = v.trim().to_string();
            true
        }
        Op::AutoWindowTitle(v) => {
            settings.auto_window_title = v.trim().to_string();
            true
        }
        // Handled by run() before apply_op, or only meaningful with --inject
        Op::FileTarget(_)
        | Op::FileMethod(_)
        | Op::FileWait(_)
        | Op::FileAfter(_)
        | Op::FileIgnore(_)
        | Op::FileRequireModule(_)
        | Op::FileRequireWindow(_) => false,
    }
}

/// 1-based index, full path, or file name
fn resolve_index(modules: &[Module], target: &str) -> Option<usize> {
    if let Ok(n) = target.parse::<usize>() {
        if n >= 1 && n <= modules.len() {
            return Some(n - 1);
        }
    }
    modules
        .iter()
        .position(|m| m.path.eq_ignore_ascii_case(target))
        .or_else(|| {
            modules.iter().position(|m| {
                std::path::Path::new(&m.path)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .map(|f| f.eq_ignore_ascii_case(target))
                    .unwrap_or(false)
            })
        })
}

fn print_state(settings: &Settings, profile: Option<&str>) {
    out(&format!("Scope: {}", settings.scope.label()));
    out(&format!(
        "Profile: {}",
        profile.unwrap_or("(global)")
    ));
    if !settings.process_name.is_empty() {
        out(&format!("Target: {}", settings.process_name));
    }
    let method_name = Settings::METHODS
        .get(settings.method as usize)
        .copied()
        .unwrap_or("unknown");
    out(&format!("Method: {} - {}", settings.method, method_name));
    out(&format!("Global start delay: {} ms", settings.delay));
    out(&format!("Global between DLLs: {} ms", settings.delay_between));
    out(&format!("Auto-inject: {}", on_off(settings.auto_inject)));
    out(&format!(
        "Auto-inject options: wait-window={} retry={} children={} child-filter={:?} title-filter={:?}",
        on_off(settings.auto_wait_window),
        on_off(settings.auto_retry),
        on_off(settings.inject_children),
        settings.auto_child_filter,
        settings.auto_window_title
    ));
    out(&format!("Close after inject: {}", on_off(settings.close_on_inject)));
    out(&format!(
        "UI scale: {}",
        if settings.ui_scale == 0 { "auto".to_string() } else { format!("{}%", settings.ui_scale) }
    ));
    out(&format!(
        "Options: erase-pe={} hide-module={} stealth={} hide-debugger={} manual-imports={} disable-seh={} disable-exception={}",
        on_off(settings.erase_pe),
        on_off(settings.hide_module),
        on_off(settings.stealth_inject),
        on_off(settings.advanced.hide_from_debugger),
        on_off(settings.advanced.manual_resolve_imports),
        on_off(settings.advanced.disable_seh_validation),
        on_off(settings.advanced.disable_exception_support),
    ));
    let presets = scramble_labels(settings);
    out(&format!(
        "Scramble: {}",
        if presets.is_empty() { "none".to_string() } else { presets.join(",") }
    ));

    out(&format!("Queue ({} files):", settings.modules.len()));
    out("  #  ON   METHOD  DELAY       AFTER   WAIT-FOR        DLL");
    for (i, m) in settings.modules.iter().enumerate() {
        let method = match m.method {
            None => "auto".to_string(),
            Some(v) => v.to_string(),
        };
        let wait_for = if m.wait_for_module.is_empty() { "-".to_string() } else { m.wait_for_module.clone() };
        out(&format!(
            "  {:<2} {:<4} {:<7} {:<11} {:<7} {:<15} {}{}",
            i + 1,
            if m.enabled { "yes" } else { "no" },
            method,
            delay_label(m),
            m.delay_after_ms,
            wait_for,
            m.path,
            format!(
                "{}{}{}",
                if m.ignore_errors { "  [ignore errors]" } else { "" },
                if m.require_module.is_empty() { String::new() } else { format!("  [if {}]", m.require_module) },
                if m.require_window { "  [needs window]" } else { "" }
            )
        ));
    }
    if settings.modules.is_empty() {
        out("  (empty)");
    }

    if !settings.profiles.is_empty() {
        let names: Vec<&str> = settings.profiles.iter().map(|(n, _)| n.as_str()).collect();
        out(&format!("Saved profiles: {}", names.join(", ")));
    }
}

fn delay_label(m: &Module) -> String {
    if m.delay_ms == 0 {
        "global".to_string()
    } else {
        format!("{} ms", m.delay_ms)
    }
}

fn on_off(v: bool) -> &'static str {
    if v { "on" } else { "off" }
}

fn scramble_labels(s: &Settings) -> Vec<&'static str> {
    let sc = &s.scramble;
    let mut out = Vec::new();
    for (on, name) in [
        (sc.scramble_header_fields, "header"),
        (sc.remove_debug_data, "debug"),
        (sc.remove_useless_data, "useless"),
        (sc.rename_sections, "rename"),
        (sc.strip_section_characteristics, "strip"),
        (sc.shift_section_data, "shiftdata"),
        (sc.shift_section_memory, "shiftmem"),
        (sc.modify_assembly_code, "code"),
        (sc.modify_import_table, "imports"),
        (sc.move_relocation_table, "reloc"),
        (sc.insert_extra_sections, "extrasec"),
        (sc.create_new_entry_point, "newep"),
        (sc.create_fake_debug_directory, "fakedebug"),
    ] {
        if on {
            out.push(name);
        }
    }
    out
}

fn inject(settings: &Settings, cmd: &Command) -> i32 {
    if crate::process::enable_debug_privilege().is_err() {
        out("[!] SeDebugPrivilege unavailable (not elevated?)");
    }

    let target = match (cmd.pid, &cmd.proc_name) {
        (Some(p), _) => Some(p),
        (None, Some(name)) => match crate::process::find_by_name(name) {
            Some(p) => {
                out(&format!("[*] Found {} [{}]", p.name, p.pid));
                Some(p.pid)
            }
            None => None,
        },
        _ => None,
    };
    let Some(pid) = target else {
        out("[!] No target process. Use --pid <n> or --proc <name.exe>");
        return 1;
    };
    // Resolve the name before injecting: after a crash the process is gone
    let target_name = cmd
        .proc_name
        .clone()
        .or_else(|| {
            crate::process::enumerate()
                .into_iter()
                .find(|p| p.pid == pid)
                .map(|p| p.name)
        })
        .unwrap_or_default();

    // One-off --dll files, or the saved queue
    let modules: Vec<Module> = if !cmd.adhoc.is_empty() {
        cmd.adhoc
            .iter()
            .map(|(path, own)| Module::with_state(path, true, *own))
            .collect()
    } else {
        settings.modules.iter().filter(|m| m.enabled).cloned().collect()
    };

    if modules.is_empty() {
        out("[!] Nothing to inject: the list is empty or every file is disabled");
        return 1;
    }

    // Validate first: wrong architecture, missing files, already-loaded modules, missing
    // runtime. Fatal problems abort before the target is touched.
    let problems = crate::queue::validate(pid, &modules);
    for pr in &problems {
        out(&format!("[{}] {}", if pr.fatal { "!" } else { "*" }, pr.line()));
    }
    if problems.iter().any(|p| p.fatal) {
        out("[!] Injection aborted: fix the problems above (or remove --validate to ignore them)");
        return 1;
    }

    let qopts = crate::queue::QueueOptions {
        start_delay: settings.delay,
        between_delay: settings.delay_between,
        base: settings.inject_options(),
        retries: cmd.retry,
    };
    let logger = |line: &str| out(line);
    let result = crate::queue::run(pid, &modules, &qopts, &logger);

    if let Some(crash) = &result.crash {
        // A clean exit code is not an access violation: the target or a payload guard called
        // ExitProcess. Say so, otherwise "crashed (0x00000000)" is misleading.
        let what = if crash.exit_code >= 0xC000_0000 {
            "TARGET CRASHED"
        } else {
            "TARGET EXITED DURING INJECTION"
        };
        out(&format!(
            "[!] {what}: {} (detected about {} ms after the injection)",
            diag::exit_text(crash.exit_code),
            crash.after_ms
        ));
        for e in &crash.events {
            out(&format!("[*] {e}"));
        }
    }

    // Keep a full report for crashes and failed runs
    if result.crash.is_some() || !result.failed.is_empty() {
        let arch = crate::process::process_arch_quick(pid)
            .map(diag::arch_name)
            .unwrap_or("unknown");
        let mut queue_lines = Vec::new();
        for (i, m) in modules.iter().enumerate() {
            queue_lines.push(format!(
                "#{} {} | method {} | wait {} ms | after {} ms | wait-for {} | ignore-errors {}",
                i + 1,
                m.path,
                m.method.map(|v| v.to_string()).unwrap_or_else(|| "global".to_string()),
                m.delay_ms,
                m.delay_after_ms,
                if m.wait_for_module.is_empty() { "-" } else { m.wait_for_module.as_str() },
                m.ignore_errors
            ));
        }
        let mut results: Vec<String> = result.verifications.iter().map(diag::verification_line).collect();
        for (p, e) in &result.failed {
            results.push(format!("FAILED {p}: {e}"));
        }
        let injected: Vec<(String, u32)> = modules
            .iter()
            .map(|m| (m.path.clone(), m.effective_method(settings.method)))
            .collect();
        let diagnosis = result
            .crash
            .as_ref()
            .map(|c| diag::diagnose(c, &target_name, &injected))
            .unwrap_or_default();
        for hint in &diagnosis {
            out(&format!("[!] {hint}"));
        }
        let report = diag::Report {
            target: &target_name,
            pid,
            arch,
            header: vec![format!("cli options: method {}", settings.method)],
            queue: queue_lines,
            results,
            crash: result.crash.clone(),
            diagnosis,
        };
        if let Some(saved) = diag::write_report(&report) {
            let kind = if result.crash.is_some() { "crash" } else { "failure" };
            out(&format!("[!] {kind} report saved: {}", saved.path));
        }
    }

    if result.failed.is_empty() && result.crash.is_none() {
        out(&format!("[+] injected {} module(s) into pid {}", result.injected, pid));
    } else if result.crash.is_none() {
        out(&format!("[!] {} of {} file(s) failed", result.failed.len(), modules.len()));
    }

    if cmd.wait_secs > 0 {
        out(&format!("[*] Waiting {}s (target keeps the DLL loaded)...", cmd.wait_secs));
        std::thread::sleep(std::time::Duration::from_secs(cmd.wait_secs as u64));
    }
    if result.failed.is_empty() && result.crash.is_none() { 0 } else { 1 }
}
fn apply_scramble_preset(o: &mut crate::config::ScrambleOptions, preset: &str) {
    match preset {
        "all" => {
            *o = crate::config::ScrambleOptions {
                scramble_header_fields: true,
                remove_debug_data: true,
                remove_useless_data: true,
                rename_sections: true,
                strip_section_characteristics: true,
                shift_section_data: false, // riskier ones off by default even in "all"
                shift_section_memory: false,
                modify_assembly_code: true,
                modify_import_table: true,
                move_relocation_table: false,
                insert_extra_sections: false,
                create_new_entry_point: false,
                create_fake_debug_directory: true,
            };
        }
        "header" => o.scramble_header_fields = true,
        "debug" => o.remove_debug_data = true,
        "useless" => o.remove_useless_data = true,
        "rename" => o.rename_sections = true,
        "strip" => o.strip_section_characteristics = true,
        "shiftdata" => o.shift_section_data = true,
        "shiftmem" => o.shift_section_memory = true,
        "code" => o.modify_assembly_code = true,
        "imports" => o.modify_import_table = true,
        "reloc" => o.move_relocation_table = true,
        "extrasec" => o.insert_extra_sections = true,
        "newep" => o.create_new_entry_point = true,
        "fakedebug" => o.create_fake_debug_directory = true,
        _ => {}
    }
}

fn print_process_list() {
    let mut procs = crate::process::enumerate();
    procs.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
            .then_with(|| a.pid.cmp(&b.pid))
    });
    out(&format!("{:>7}  {:<7}  {}", "PID", "ARCH", "PROCESS"));
    for p in &procs {
        let arch = crate::process::process_arch_quick(p.pid)
            .map(crate::diag::arch_name)
            .unwrap_or("unknown");
        out(&format!("{:>7}  {:<7}  {}", p.pid, arch, p.name));
    }
    out(&format!("[*] {} process(es)", procs.len()));
}

fn print_help() {
    out(&format!("BlackBox Injector v{} CLI - everything the GUI can do, from a command prompt", crate::version::VERSION));
    out("");
    out("Start with no arguments for the GUI. Any argument runs this command line;");
    out("an unknown command prints this list.");
    out("");
    out("HELP");
    out("  --help, -h, /?                   print this command list");
    out("  --version, -v                    print the version and exit");
    out("  --cli                            force the command line (any argument already does)");
    out("");
    out("TARGET");
    out("  --pid <n>                        target process id");
    out("  --proc, --process <name.exe>     target process name");
    out("  --ps, --processes, --procs       list running processes (PID, architecture, name)");
    out("");
    out("INJECT");
    out("  --inject                         inject the saved queue (enabled files, in order)");
    out("  --dll <path>                     inject this file instead (repeatable; implies --inject)");
    out("  --dll-delay <ms>                 wait before the next --dll");
    out("  --wait <sec>                     keep running this long after injecting");
    out("");
    out("QUEUE");
    out("  --add <path>                     append a file (enabled)");
    out("  --add-disabled <path>            append a file disabled");
    out("  --remove <n|path>                remove by position or path");
    out("  --clear                          empty the list");
    out("  --enable <n|path>                tick a file");
    out("  --disable <n|path>               untick a file");
    out("  --up <n|path>                    move a file one place up");
    out("  --down <n|path>                  move a file one place down");
    out("  --move <from> <to>               move a file to another position");
    out("  --set-delay <n|path> <ms>        per-file delay (0 = use the global delays)");
    out("  --file <n|path>                  choose the row the next --file-* options edit");
    out("  --file-method <0-5|global>       per-file method override");
    out("  --file-wait <module>             wait for this module before injecting the file");
    out("  --file-after <ms>                extra wait after injecting the file");
    out("  --file-ignore <on|off>           keep going when this file fails");
    out("  --file-if-module <module>        inject this file only if the module is loaded");
    out("  --file-if-window <on|off>        inject this file only if the target has a window");
    out("");
    out("SETTINGS");
    out("  --method <0-5>                   0 Standard, 1 LdrLoadDll, 2 Manual Map, 3 Hijack, 4 NtCreateThreadEx, 5 APC (threadless)");
    out("  --delay <ms>                     global start delay (first file / after the process appears)");
    out("  --delay-between <ms>             global default gap between files");
    out("  --auto-inject <on|off>           wait for the target and inject when it appears");
    out("  --close-on-inject <on|off>       close the GUI after a successful injection");
    out("  --erase-pe <on|off>              erase PE headers from memory");
    out("  --hide-module <on|off>           unlink the module from the PEB lists");
    out("  --stealth <on|off>               stealth inject (hide thread / cloaked creation)");
    out("  --hide-debugger <on|off>         ThreadHideFromDebugger");
    out("  --manual-imports <on|off>        resolve imports manually");
    out("  --disable-exception, --disable-exceptions   drop the exception directory (manual map)");
    out("  --disable-seh <on|off>           disable SEH validation");
    out("  --scramble <preset|none>         all, header, debug, useless, rename, strip, shiftdata,");
    out("                                   shiftmem, code, imports, reloc, extrasec, newep, fakedebug");
    out("  --scope global|per-process       one setup for everything, or one per target process");
    out("  --profile <name.exe>             edit that process' saved setup");
    out("  --forget <name.exe>              delete a saved per-process setup");
    out("  --ui-scale <0|75-150>            GUI zoom percentage (0 = system DPI, max 150)");
    out("  --auto-wait-window <on|off>      auto-inject waits for the target window");
    out("  --auto-retry <on|off>            auto-inject retries failed files");
    out("  --inject-children <on|off>       auto-inject also targets child processes with a window");
    out("  --auto-child-filter <text>       only children whose name contains this text");
    out("  --auto-window-title <text>       wait for a window title containing this text");
    out("  --retry <n>                      retry a failed file n times when injecting");
    out("  --list                           print the current setup and queue");
    out("  --save / --no-save               force or skip saving (changes save by default)");
    out("");
    out("CHECKS");
    out("  --validate                       check the queue against the target, then exit");
    out("  --inspect <dll>                  report a DLL's imports/exports resolved against the target");
    out("  --unload <name|path>             unload a module from the target");
    out("  --wipe-headers                   with --unload: erase the PE headers first");
    out("");
    out("RUNTIME");
    out("  --check-redist                   report the VC++ 2015-2022 runtime");
    out("  --install-redist                 install it with winget (or open the download page)");
    out("");
    out("EXAMPLES");
    out("  BlackBoxInjector --cli --add C:\\modules\\a.dll --set-delay 1 250 --add C:\\modules\\b.dll --list");
    out("  BlackBoxInjector --cli --proc target.exe --move 2 1 --method 2 --inject");
    out("  BlackBoxInjector --cli --scope per-process --profile target.exe --add C:\\modules\\a.dll --save");
    out("  BlackBoxInjector --cli --profile target.exe --list");
    out("  BlackBoxInjector --cli --check-redist");
    out("  BlackBoxInjector --cli --proc target.exe --validate");
    out("  BlackBoxInjector --cli --proc target.exe --unload a.dll --wipe-headers");
    out("  BlackBoxInjector --cli --file 2 --file-method 2 --file-wait d3d11.dll --file-after 500 --list");
}
