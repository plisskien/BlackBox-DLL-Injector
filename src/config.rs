//! Settings persisted in the registry (HKCU\Software\BlackBoxInjector) - no files.
//! Schema for persistent settings and profiles.

use crate::win32::advapi32::*;
use crate::win32::*;

pub const REG_PATH: &str = "Software\\BlackBoxInjector";
/// Pre-rename key; imported once so an upgrade keeps modules/profiles.
const OLD_REG_PATH: &str = "Software\\hvh-injector";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdvancedOptions {
    pub disable_exception_support: bool,
    pub disable_seh_validation: bool,
    pub hide_from_debugger: bool,
    pub manual_resolve_imports: bool,
}

impl Default for AdvancedOptions {
    fn default() -> Self {
        AdvancedOptions {
            disable_exception_support: false,
            disable_seh_validation: false,
            hide_from_debugger: false,
            manual_resolve_imports: false,
        }
    }
}

/// PE Scrambler configuration options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrambleOptions {
    pub scramble_header_fields: bool,
    pub remove_debug_data: bool,
    pub remove_useless_data: bool,
    pub rename_sections: bool,
    pub strip_section_characteristics: bool,
    pub shift_section_data: bool,
    pub shift_section_memory: bool,
    pub modify_assembly_code: bool,
    pub modify_import_table: bool,
    pub move_relocation_table: bool,
    pub insert_extra_sections: bool,
    pub create_new_entry_point: bool,
    pub create_fake_debug_directory: bool,
}

impl Default for ScrambleOptions {
    fn default() -> Self {
        ScrambleOptions {
            scramble_header_fields: false,
            remove_debug_data: false,
            remove_useless_data: false,
            rename_sections: false,
            strip_section_characteristics: false,
            shift_section_data: false,
            shift_section_memory: false,
            modify_assembly_code: false,
            modify_import_table: false,
            move_relocation_table: false,
            insert_extra_sections: false,
            create_new_entry_point: false,
            create_fake_debug_directory: false,
        }
    }
}

impl ScrambleOptions {
    pub fn any(&self) -> bool {
        self.scramble_header_fields
            || self.remove_debug_data
            || self.remove_useless_data
            || self.rename_sections
            || self.strip_section_characteristics
            || self.shift_section_data
            || self.shift_section_memory
            || self.modify_assembly_code
            || self.modify_import_table
            || self.move_relocation_table
            || self.insert_extra_sections
            || self.create_new_entry_point
            || self.create_fake_debug_directory
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WarningsShown {
    pub ldrp_load_dll: bool,
    pub manual_map: bool,
    pub scramble: bool,
}

/// Where the injector keeps the setup: one list for everything, or a separate one per target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SettingsScope {
    /// One list/order/delays/method for every target
    #[default]
    Global,
    /// Each target process remembers its own list/order/delays/method
    PerProcess,
}

impl SettingsScope {
    pub fn label(&self) -> &'static str {
        match self {
            SettingsScope::Global => "global",
            SettingsScope::PerProcess => "per-process",
        }
    }
}

/// Everything remembered for one target process name.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ProcessProfile {
    pub modules: Vec<Module>,
    pub method: u32,
    pub delay: u32,
    pub delay_between: u32,
    pub auto_inject: bool,
    pub close_on_inject: bool,
    pub erase_pe: bool,
    pub hide_module: bool,
    pub stealth_inject: bool,
    pub advanced: AdvancedOptions,
    pub scramble: ScrambleOptions,
}

/// One entry in the injection list. The position in `Settings::modules` is the
/// injection order: the first entry is injected first, then the second, and so on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub path: String,
    pub enabled: bool,
    /// Milliseconds to wait before this file is injected. 0 means "use the global
    /// defaults": `Settings::delay` for the first entry, `Settings::delay_between`
    /// for every entry after it.
    pub delay_ms: u32,
    /// Per-file injection method override. None = use the global method.
    pub method: Option<u32>,
    /// Wait until this module is loaded in the target before injecting this file.
    /// Empty = no wait.
    pub wait_for_module: String,
    /// Extra wait after this file was injected, before the next one.
    pub delay_after_ms: u32,
    /// Keep the rest of the queue going when this file fails.
    pub ignore_errors: bool,
    /// Inject only when this module is already loaded in the target (empty = always).
    pub require_module: String,
    /// Inject only when the target owns a window.
    pub require_window: bool,
    /// Inject only when the target process name matches this (empty = any target)
    pub require_process: String,
}

impl Module {
    pub fn new(path: &str) -> Module {
        Module {
            path: path.to_string(),
            enabled: true,
            delay_ms: 0,
            method: None,
            wait_for_module: String::new(),
            delay_after_ms: 0,
            ignore_errors: false,
            require_module: String::new(),
            require_window: false,
            require_process: String::new(),
        }
    }

    pub fn with_state(path: &str, enabled: bool, delay_ms: u32) -> Module {
        Module { enabled, delay_ms, ..Module::new(path) }
    }

    /// File name shown in the list (the path is shown in its own column).
    pub fn file_name(&self) -> String {
        std::path::Path::new(&self.path)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(self.path.as_str())
            .to_string()
    }

    /// Method this file is injected with: its own override, else the global one.
    pub fn effective_method(&self, global: u32) -> u32 {
        self.method.unwrap_or(global).min(5)
    }

    /// How long to wait before injecting this entry. `index` is its position among the
    /// files being injected, so entry 0 falls back to the start delay and the rest to the
    /// between-DLLs delay.
    pub fn wait_before(&self, index: usize, start_delay: u32, between_delay: u32) -> u32 {
        if self.delay_ms > 0 {
            self.delay_ms
        } else if index == 0 {
            start_delay
        } else {
            between_delay
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub method: u32,
    pub auto_inject: bool,
    pub close_on_inject: bool,
    pub delay: u32,
    pub delay_between: u32,
    pub erase_pe: bool,
    pub hide_module: bool,
    pub stealth_inject: bool,
    pub process_name: String,
    pub modules: Vec<Module>,
    pub advanced: AdvancedOptions,
    pub scramble: ScrambleOptions,
    pub warnings: WarningsShown,
    pub ui_scale: u32,
    /// Auto-inject waits for a window before injecting
    pub auto_wait_window: bool,
    /// Auto-inject retries a failed file a couple of times
    pub auto_retry: bool,
    /// Auto-inject also injects child processes of the target
    pub inject_children: bool,
    /// Only child processes whose name contains this text (empty = any windowed child)
    pub auto_child_filter: String,
    /// Only wait for a window whose title contains this text (empty = any window)
    pub auto_window_title: String,
    /// Global vs per-process setups
    pub scope: SettingsScope,
    /// Saved per-process setups, keyed by process name (case-insensitive)
    pub profiles: Vec<(String, ProcessProfile)>,
    /// Suppress all popup dialogs; errors go to the log only
    pub log_only: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            method: 0,
            auto_inject: false,
            close_on_inject: false,
            delay: 0,
            delay_between: 0,
            erase_pe: false,
            hide_module: false,
            stealth_inject: false,
            process_name: String::new(),
            modules: Vec::new(),
            advanced: AdvancedOptions::default(),
            scramble: ScrambleOptions::default(),
            warnings: WarningsShown::default(),
            ui_scale: 0,
            auto_wait_window: true,
            auto_retry: true,
            inject_children: false,
            auto_child_filter: String::new(),
            auto_window_title: String::new(),
            log_only: false,
            scope: SettingsScope::default(),
            profiles: Vec::new(),
        }
    }
}

impl Settings {
    pub const METHODS: [&'static str; 6] = [
        "Standard (CreateRemoteThread + LoadLibrary)",
        "LdrLoadDll (bypass kernel32 hooks)",
        "Manual Map (in-memory PE loader)",
        "Thread Hijacking",
        "NtCreateThreadEx",
        "APC (threadless - queued to existing threads)",
    ];

    /// The options both the Inject button and the auto-inject worker run with.
    pub fn inject_options(&self) -> crate::inject::InjectOptions {
        crate::inject::InjectOptions {
            method: self.method,
            erase_pe: self.erase_pe,
            hide_module: self.hide_module,
            stealth_inject: self.stealth_inject,
            hide_from_debugger: self.advanced.hide_from_debugger,
            manual_resolve_imports: self.advanced.manual_resolve_imports,
            disable_seh_validation: self.advanced.disable_seh_validation,
            disable_exception_support: self.advanced.disable_exception_support,
            scramble: self.scramble,
        }
    }

    /// The current setup, as a profile can hold it.
    pub fn snapshot_profile(&self) -> ProcessProfile {
        ProcessProfile {
            modules: self.modules.clone(),
            method: self.method,
            delay: self.delay,
            delay_between: self.delay_between,
            auto_inject: self.auto_inject,
            close_on_inject: self.close_on_inject,
            erase_pe: self.erase_pe,
            hide_module: self.hide_module,
            stealth_inject: self.stealth_inject,
            advanced: self.advanced,
            scramble: self.scramble,
        }
    }

    pub fn profile(&self, name: &str) -> Option<&ProcessProfile> {
        self.profiles
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, p)| p)
    }

    /// Stores the current setup under a process name (overwriting an older one).
    pub fn remember_profile(&mut self, name: &str) {
        if name.trim().is_empty() {
            return;
        }
        let snapshot = self.snapshot_profile();
        if let Some(slot) = self.profiles.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            slot.1 = snapshot;
        } else {
            self.profiles.push((name.to_string(), snapshot));
        }
    }

    /// Copies a saved profile into the live settings. False when there is none.
    pub fn apply_profile(&mut self, name: &str) -> bool {
        let Some(p) = self.profile(name).cloned() else {
            return false;
        };
        self.modules = p.modules;
        self.method = p.method;
        self.delay = p.delay;
        self.delay_between = p.delay_between;
        self.auto_inject = p.auto_inject;
        self.close_on_inject = p.close_on_inject;
        self.erase_pe = p.erase_pe;
        self.hide_module = p.hide_module;
        self.stealth_inject = p.stealth_inject;
        self.advanced = p.advanced;
        self.scramble = p.scramble;
        true
    }

    pub fn forget_profile(&mut self, name: &str) -> bool {
        let before = self.profiles.len();
        self.profiles.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        self.profiles.len() != before
    }

    /// Removes a saved profile's registry key (used by `--forget`).
    pub fn delete_profile_key(&self, name: &str) {
        let sub = profile_key_name(name);
        if sub.is_empty() {
            return;
        }
        unsafe {
            let mut key: HKEY = std::ptr::null_mut();
            let path = crate::util::wstr(REG_PATH);
            if RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_WRITE, &mut key) != ERROR_SUCCESS {
                return;
            }
            let _g = KeyGuard(key);
            let profiles = crate::util::wstr("Profiles");
            let mut pk: HKEY = std::ptr::null_mut();
            if RegOpenKeyExW(key, profiles.as_ptr(), 0, KEY_WRITE, &mut pk) == ERROR_SUCCESS {
                let wsub = crate::util::wstr(&sub);
                RegDeleteTreeW(pk, wsub.as_ptr());
                let _ = RegCloseKey(pk);
            }
        }
    }

    pub fn load() -> Settings {
        let mut s = Settings::default();
        let mut migrated = false;
        unsafe {
            let mut key: HKEY = std::ptr::null_mut();
            let mut path = crate::util::wstr(REG_PATH);
            if RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_READ, &mut key) != ERROR_SUCCESS {
                path = crate::util::wstr(OLD_REG_PATH);
                if RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_READ, &mut key) != ERROR_SUCCESS {
                    return s; // first run: defaults
                }
                migrated = true;
            }
            let _g = KeyGuard(key);

            // Scalars
            if let Some(v) = get_dw(key, "Method") {
                s.method = v.min(5);
            }
            s.auto_inject = get_dw(key, "AutoInject").unwrap_or(0) != 0;
            s.close_on_inject = get_dw(key, "CloseOnInject").unwrap_or(0) != 0;
            s.delay = get_dw(key, "Delay").unwrap_or(0);
            s.delay_between = get_dw(key, "DelayBetween").unwrap_or(0);
            s.erase_pe = get_dw(key, "ErasePE").unwrap_or(0) != 0;
            s.hide_module = get_dw(key, "HideModule").unwrap_or(0) != 0;
            s.stealth_inject = get_dw(key, "StealthInject").unwrap_or(0) != 0;
            s.scope = if get_dw(key, "RememberPerProcess").unwrap_or(0) != 0 {
                SettingsScope::PerProcess
            } else {
                SettingsScope::Global
            };
            if let Some(v) = get_sz(key, "ProcessName") {
                s.process_name = v;
            }
            s.modules = read_modules(key);
            s.auto_wait_window = get_dw(key, "AutoWaitWindow").unwrap_or(1) != 0;
            s.auto_retry = get_dw(key, "AutoRetry").unwrap_or(1) != 0;
            s.inject_children = get_dw(key, "InjectChildren").unwrap_or(0) != 0;
            s.auto_child_filter = get_sz(key, "AutoChildFilter").unwrap_or_default();
            s.auto_window_title = get_sz(key, "AutoWindowTitle").unwrap_or_default();
        s.log_only = get_dw(key, "LogOnly").unwrap_or(0) != 0;
            let raw_scale = get_dw(key, "UiScale").unwrap_or(0);
            s.ui_scale = if raw_scale == 0 { 0 } else { raw_scale.clamp(75, 150) };

            // Advanced subkey
            let adv = crate::util::wstr("Advanced");
            let mut hk: HKEY = std::ptr::null_mut();
            if RegOpenKeyExW(key, adv.as_ptr(), 0, KEY_READ, &mut hk) == ERROR_SUCCESS {
                s.advanced.disable_exception_support = get_dw(hk, "DisableExceptionSupport").unwrap_or(0) != 0;
                s.advanced.disable_seh_validation = get_dw(hk, "DisableSEHValidation").unwrap_or(0) != 0;
                s.advanced.hide_from_debugger = get_dw(hk, "HideFromDebugger").unwrap_or(0) != 0;
                s.advanced.manual_resolve_imports = get_dw(hk, "ManualResolveImports").unwrap_or(0) != 0;
                let _ = RegCloseKey(hk);
            }

            // Scramble subkey
            let scr = crate::util::wstr("Scramble");
            if RegOpenKeyExW(key, scr.as_ptr(), 0, KEY_READ, &mut hk) == ERROR_SUCCESS {
                let sc = &mut s.scramble;
                sc.scramble_header_fields = get_dw(hk, "ScrambleHeaderFields").unwrap_or(0) != 0;
                sc.remove_debug_data = get_dw(hk, "RemoveDebugData").unwrap_or(0) != 0;
                sc.remove_useless_data = get_dw(hk, "RemoveUselessData").unwrap_or(0) != 0;
                sc.rename_sections = get_dw(hk, "RenameSections").unwrap_or(0) != 0;
                sc.strip_section_characteristics = get_dw(hk, "StripSectionCharacteristics").unwrap_or(0) != 0;
                sc.shift_section_data = get_dw(hk, "ShiftSectionData").unwrap_or(0) != 0;
                sc.shift_section_memory = get_dw(hk, "ShiftSectionMemory").unwrap_or(0) != 0;
                sc.modify_assembly_code = get_dw(hk, "ModifyAssemblyCode").unwrap_or(0) != 0;
                sc.modify_import_table = get_dw(hk, "ModifyImportTable").unwrap_or(0) != 0;
                sc.move_relocation_table = get_dw(hk, "MoveRelocationTable").unwrap_or(0) != 0;
                sc.insert_extra_sections = get_dw(hk, "InsertExtraSections").unwrap_or(0) != 0;
                sc.create_new_entry_point = get_dw(hk, "CreateNewEntryPoint").unwrap_or(0) != 0;
                sc.create_fake_debug_directory = get_dw(hk, "CreateFakeDebugDirectory").unwrap_or(0) != 0;
                let _ = RegCloseKey(hk);
            }

            // Warnings subkey
            let warn = crate::util::wstr("Warnings");
            if RegOpenKeyExW(key, warn.as_ptr(), 0, KEY_READ, &mut hk) == ERROR_SUCCESS {
                s.warnings.ldrp_load_dll = get_dw(hk, "LdrpLoadDll").unwrap_or(0) != 0;
                s.warnings.manual_map = get_dw(hk, "ManualMap").unwrap_or(0) != 0;
                s.warnings.scramble = get_dw(hk, "Scramble").unwrap_or(0) != 0;
                let _ = RegCloseKey(hk);
            }

            // Per-process profiles, one subkey per target name
            let profiles_path = crate::util::wstr("Profiles");
            if RegOpenKeyExW(key, profiles_path.as_ptr(), 0, KEY_READ, &mut hk) == ERROR_SUCCESS {
                for name in enum_subkeys(hk) {
                    let wname = crate::util::wstr(&name);
                    let mut pk: HKEY = std::ptr::null_mut();
                    if RegOpenKeyExW(hk, wname.as_ptr(), 0, KEY_READ, &mut pk) == ERROR_SUCCESS {
                        s.profiles.push((name, read_profile(pk)));
                        let _ = RegCloseKey(pk);
                    }
                }
                let _ = RegCloseKey(hk);
            }
        }
        if migrated {
            s.save();
        }
        s
    }

    pub fn save(&self) {
        unsafe {
            let mut key: HKEY = std::ptr::null_mut();
            let path = crate::util::wstr(REG_PATH);
            if RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                std::ptr::null_mut(),
                &mut key,
                std::ptr::null_mut(),
            ) != ERROR_SUCCESS
            {
                return;
            }
            let _g = KeyGuard(key);

            set_dw(key, "Method", self.method);
            set_dw(key, "AutoInject", self.auto_inject as u32);
            set_dw(key, "CloseOnInject", self.close_on_inject as u32);
            set_dw(key, "Delay", self.delay);
            set_dw(key, "DelayBetween", self.delay_between);
            set_dw(key, "ErasePE", self.erase_pe as u32);
            set_dw(key, "HideModule", self.hide_module as u32);
            set_dw(key, "StealthInject", self.stealth_inject as u32);
            set_sz(key, "ProcessName", &self.process_name);
            write_modules(key, &self.modules);
            set_dw(key, "UiScale", self.ui_scale);
            set_dw(key, "AutoWaitWindow", self.auto_wait_window as u32);
            set_dw(key, "AutoRetry", self.auto_retry as u32);
            set_dw(key, "InjectChildren", self.inject_children as u32);
            set_sz(key, "AutoChildFilter", &self.auto_child_filter);
            set_sz(key, "AutoWindowTitle", &self.auto_window_title);
        set_dw(key, "LogOnly", if self.log_only { 1 } else { 0 });
            set_dw(
                key,
                "RememberPerProcess",
                matches!(self.scope, SettingsScope::PerProcess) as u32,
            );

            let adv = crate::util::wstr("Advanced");
            let mut hk: HKEY = std::ptr::null_mut();
            if RegCreateKeyExW(key, adv.as_ptr(), 0, std::ptr::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, std::ptr::null_mut(), &mut hk, std::ptr::null_mut()) == ERROR_SUCCESS {
                set_dw(hk, "DisableExceptionSupport", self.advanced.disable_exception_support as u32);
                set_dw(hk, "DisableSEHValidation", self.advanced.disable_seh_validation as u32);
                set_dw(hk, "HideFromDebugger", self.advanced.hide_from_debugger as u32);
                set_dw(hk, "ManualResolveImports", self.advanced.manual_resolve_imports as u32);
                let _ = RegCloseKey(hk);
            }

            let scr = crate::util::wstr("Scramble");
            if RegCreateKeyExW(key, scr.as_ptr(), 0, std::ptr::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, std::ptr::null_mut(), &mut hk, std::ptr::null_mut()) == ERROR_SUCCESS {
                let sc = &self.scramble;
                set_dw(hk, "ScrambleHeaderFields", sc.scramble_header_fields as u32);
                set_dw(hk, "RemoveDebugData", sc.remove_debug_data as u32);
                set_dw(hk, "RemoveUselessData", sc.remove_useless_data as u32);
                set_dw(hk, "RenameSections", sc.rename_sections as u32);
                set_dw(hk, "StripSectionCharacteristics", sc.strip_section_characteristics as u32);
                set_dw(hk, "ShiftSectionData", sc.shift_section_data as u32);
                set_dw(hk, "ShiftSectionMemory", sc.shift_section_memory as u32);
                set_dw(hk, "ModifyAssemblyCode", sc.modify_assembly_code as u32);
                set_dw(hk, "ModifyImportTable", sc.modify_import_table as u32);
                set_dw(hk, "MoveRelocationTable", sc.move_relocation_table as u32);
                set_dw(hk, "InsertExtraSections", sc.insert_extra_sections as u32);
                set_dw(hk, "CreateNewEntryPoint", sc.create_new_entry_point as u32);
                set_dw(hk, "CreateFakeDebugDirectory", sc.create_fake_debug_directory as u32);
                let _ = RegCloseKey(hk);
            }

            let warn = crate::util::wstr("Warnings");
            if RegCreateKeyExW(key, warn.as_ptr(), 0, std::ptr::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, std::ptr::null_mut(), &mut hk, std::ptr::null_mut()) == ERROR_SUCCESS {
                set_dw(hk, "LdrpLoadDll", self.warnings.ldrp_load_dll as u32);
                set_dw(hk, "ManualMap", self.warnings.manual_map as u32);
                set_dw(hk, "Scramble", self.warnings.scramble as u32);
                let _ = RegCloseKey(hk);
            }

            // Per-process profiles. In per-process mode the live setup is the current
            // target's profile, so it is written from the live values, not from the copy.
            let mut profiles = self.profiles.clone();
            if matches!(self.scope, SettingsScope::PerProcess) && !self.process_name.trim().is_empty() {
                let live = self.snapshot_profile();
                match profiles.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(&self.process_name)) {
                    Some(slot) => slot.1 = live,
                    None => profiles.push((self.process_name.clone(), live)),
                }
            }
            if !profiles.is_empty() {
                let profiles_path = crate::util::wstr("Profiles");
                let mut pk: HKEY = std::ptr::null_mut();
                if RegCreateKeyExW(
                    key,
                    profiles_path.as_ptr(),
                    0,
                    std::ptr::null(),
                    REG_OPTION_NON_VOLATILE,
                    KEY_WRITE,
                    std::ptr::null_mut(),
                    &mut pk,
                    std::ptr::null_mut(),
                ) == ERROR_SUCCESS
                {
                    for (name, profile) in &profiles {
                        let sub = profile_key_name(name);
                        if sub.is_empty() {
                            continue;
                        }
                        let wsub = crate::util::wstr(&sub);
                        let mut sk: HKEY = std::ptr::null_mut();
                        if RegCreateKeyExW(
                            pk,
                            wsub.as_ptr(),
                            0,
                            std::ptr::null(),
                            REG_OPTION_NON_VOLATILE,
                            KEY_WRITE,
                            std::ptr::null_mut(),
                            &mut sk,
                            std::ptr::null_mut(),
                        ) == ERROR_SUCCESS
                        {
                            write_profile(sk, profile);
                            let _ = RegCloseKey(sk);
                        }
                    }
                    let _ = RegCloseKey(pk);
                }
            }
        }
    }
}

struct KeyGuard(HKEY);
impl Drop for KeyGuard {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

// ---- value helpers ----

unsafe fn get_dw(key: HKEY, name: &str) -> Option<u32> {
    let wname = crate::util::wstr(name);
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

unsafe fn set_dw(key: HKEY, name: &str, val: u32) {
    let wname = crate::util::wstr(name);
    RegSetValueExW(key, wname.as_ptr(), 0, REG_DWORD, &val as *const u32 as *const u8, 4);
}

unsafe fn get_sz(key: HKEY, name: &str) -> Option<String> {
    let wname = crate::util::wstr(name);
    let mut kind = 0u32;
    let mut size = 0u32;
    if RegQueryValueExW(key, wname.as_ptr(), std::ptr::null_mut(), &mut kind, std::ptr::null_mut(), &mut size) != ERROR_SUCCESS
        || (kind != REG_SZ && kind != REG_MULTI_SZ)
    {
        return None;
    }
    let mut buf = vec![0u8; (size as usize).max(2)];
    if RegQueryValueExW(key, wname.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), buf.as_mut_ptr(), &mut size) != ERROR_SUCCESS {
        return None;
    }
    let u16s: Vec<u16> = buf
        .chunks_exact(2)
        .take((size as usize) / 2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let s = String::from_utf16_lossy(&u16s);
    Some(s.trim_end_matches('\0').to_string())
}

unsafe fn set_sz(key: HKEY, name: &str, val: &str) {
    let wname = crate::util::wstr(name);
    let wval = crate::util::wstr(val);
    RegSetValueExW(
        key,
        wname.as_ptr(),
        0,
        REG_SZ,
        wval.as_ptr() as *const u8,
        (wval.len() * 2) as u32,
    );
}

unsafe fn get_multi_sz(key: HKEY, name: &str) -> Option<Vec<String>> {
    let wname = crate::util::wstr(name);
    let mut kind = 0u32;
    let mut size = 0u32;
    if RegQueryValueExW(key, wname.as_ptr(), std::ptr::null_mut(), &mut kind, std::ptr::null_mut(), &mut size) != ERROR_SUCCESS
        || kind != REG_MULTI_SZ
        || size < 2
    {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    if RegQueryValueExW(key, wname.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), buf.as_mut_ptr(), &mut size) != ERROR_SUCCESS {
        return None;
    }
    let u16s: Vec<u16> = buf
        .chunks_exact(2)
        .take((size as usize) / 2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some(
        u16s
            .split(|&c| c == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf16_lossy(p))
            .collect(),
    )
}

unsafe fn set_multi_sz(key: HKEY, name: &str, vals: &[String]) {
    let wname = crate::util::wstr(name);
    let mut buf: Vec<u16> = Vec::new();
    for v in vals {
        buf.extend(v.encode_utf16());
        buf.push(0);
    }
    buf.push(0); // double-null terminator
    if buf.len() == 1 {
        buf.push(0);
    }
    RegSetValueExW(key, wname.as_ptr(), 0, REG_MULTI_SZ, buf.as_ptr() as *const u8, (buf.len() * 2) as u32);
}

// ---- the injection queue, stored as three parallel MULTI_SZ values ----

/// Reads the ordered list plus its enabled flags, per-file delays and per-file options.
/// A settings file from before these columns still loads: missing values default to
/// enabled / no delay / global method / no wait / no after-delay / stop on error.
unsafe fn read_modules(key: HKEY) -> Vec<Module> {
    let Some(paths) = get_multi_sz(key, "Modules") else {
        return Vec::new();
    };
    let enabled = get_multi_sz(key, "ModuleEnabled").unwrap_or_default();
    let delays = get_multi_sz(key, "ModuleDelays").unwrap_or_default();
    let methods = get_multi_sz(key, "ModuleMethods").unwrap_or_default();
    let waits = get_multi_sz(key, "ModuleWaitFor").unwrap_or_default();
    let afters = get_multi_sz(key, "ModuleAfter").unwrap_or_default();
    let ignores = get_multi_sz(key, "ModuleIgnore").unwrap_or_default();
    let requires = get_multi_sz(key, "ModuleRequire").unwrap_or_default();
    let req_windows = get_multi_sz(key, "ModuleRequireWindow").unwrap_or_default();
    let req_processes = get_multi_sz(key, "ModuleRequireProcess").unwrap_or_default();
    let mut seen: Vec<String> = Vec::new();
    paths
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            // Skip blanks and repeats, so the list is a clean injection sequence
            if p.is_empty() || seen.iter().any(|q| q.eq_ignore_ascii_case(p)) {
                return None;
            }
            seen.push(p.clone());
            let on = enabled.get(i).map(|v| v != "0").unwrap_or(true);
            let ms = delays.get(i).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
            let method = methods.get(i).and_then(|v| v.trim().parse::<u32>().ok()).map(|m| m.min(5));
            let wait_for = waits.get(i).map(|v| v.trim().to_string()).unwrap_or_default();
            let after = afters.get(i).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
            let ignore = ignores.get(i).map(|v| v != "0").unwrap_or(false);
            let require_module = requires.get(i).map(|v| v.trim().to_string()).unwrap_or_default();
            let require_window = req_windows.get(i).map(|v| v != "0").unwrap_or(false);
            let require_process = req_processes.get(i).map(|v| v.trim().to_string()).unwrap_or_default();
            Some(Module {
                path: p.clone(),
                enabled: on,
                delay_ms: ms,
                method,
                wait_for_module: wait_for,
                delay_after_ms: after,
                ignore_errors: ignore,
                require_module,
                require_window,
                require_process,
            })
        })
        .collect()
}

unsafe fn write_modules(key: HKEY, modules: &[Module]) {
    set_multi_sz(key, "Modules", &modules.iter().map(|m| m.path.clone()).collect::<Vec<_>>());
    set_multi_sz(
        key,
        "ModuleEnabled",
        &modules.iter().map(|m| if m.enabled { "1".to_string() } else { "0".to_string() }).collect::<Vec<_>>(),
    );
    set_multi_sz(
        key,
        "ModuleDelays",
        &modules.iter().map(|m| m.delay_ms.to_string()).collect::<Vec<_>>(),
    );
    set_multi_sz(
        key,
        "ModuleMethods",
        &modules
            .iter()
            .map(|m| m.method.map(|v| v.to_string()).unwrap_or_default())
            .collect::<Vec<_>>(),
    );
    set_multi_sz(key, "ModuleWaitFor", &modules.iter().map(|m| m.wait_for_module.clone()).collect::<Vec<_>>());
    set_multi_sz(
        key,
        "ModuleAfter",
        &modules.iter().map(|m| m.delay_after_ms.to_string()).collect::<Vec<_>>(),
    );
    set_multi_sz(
        key,
        "ModuleIgnore",
        &modules.iter().map(|m| if m.ignore_errors { "1".to_string() } else { "0".to_string() }).collect::<Vec<_>>(),
    );
    set_multi_sz(key, "ModuleRequire", &modules.iter().map(|m| m.require_module.clone()).collect::<Vec<_>>());
    set_multi_sz(
        key,
        "ModuleRequireWindow",
        &modules.iter().map(|m| if m.require_window { "1".to_string() } else { "0".to_string() }).collect::<Vec<_>>(),
    );
    set_multi_sz(
        key,
        "ModuleRequireProcess",
        &modules.iter().map(|m| m.require_process.clone()).collect::<Vec<_>>(),
    );
}

// ---- per-process profiles ----

unsafe fn read_profile(key: HKEY) -> ProcessProfile {
    let mut p = ProcessProfile {
        modules: read_modules(key),
        method: get_dw(key, "Method").unwrap_or(0).min(5),
        delay: get_dw(key, "Delay").unwrap_or(0),
        delay_between: get_dw(key, "DelayBetween").unwrap_or(0),
        auto_inject: get_dw(key, "AutoInject").unwrap_or(0) != 0,
        close_on_inject: get_dw(key, "CloseOnInject").unwrap_or(0) != 0,
        erase_pe: get_dw(key, "ErasePE").unwrap_or(0) != 0,
        hide_module: get_dw(key, "HideModule").unwrap_or(0) != 0,
        stealth_inject: get_dw(key, "StealthInject").unwrap_or(0) != 0,
        ..Default::default()
    };
    p.advanced.disable_exception_support = get_dw(key, "DisableExceptionSupport").unwrap_or(0) != 0;
    p.advanced.disable_seh_validation = get_dw(key, "DisableSEHValidation").unwrap_or(0) != 0;
    p.advanced.hide_from_debugger = get_dw(key, "HideFromDebugger").unwrap_or(0) != 0;
    p.advanced.manual_resolve_imports = get_dw(key, "ManualResolveImports").unwrap_or(0) != 0;

    let sc = &mut p.scramble;
    sc.scramble_header_fields = get_dw(key, "ScrambleHeaderFields").unwrap_or(0) != 0;
    sc.remove_debug_data = get_dw(key, "RemoveDebugData").unwrap_or(0) != 0;
    sc.remove_useless_data = get_dw(key, "RemoveUselessData").unwrap_or(0) != 0;
    sc.rename_sections = get_dw(key, "RenameSections").unwrap_or(0) != 0;
    sc.strip_section_characteristics = get_dw(key, "StripSectionCharacteristics").unwrap_or(0) != 0;
    sc.shift_section_data = get_dw(key, "ShiftSectionData").unwrap_or(0) != 0;
    sc.shift_section_memory = get_dw(key, "ShiftSectionMemory").unwrap_or(0) != 0;
    sc.modify_assembly_code = get_dw(key, "ModifyAssemblyCode").unwrap_or(0) != 0;
    sc.modify_import_table = get_dw(key, "ModifyImportTable").unwrap_or(0) != 0;
    sc.move_relocation_table = get_dw(key, "MoveRelocationTable").unwrap_or(0) != 0;
    sc.insert_extra_sections = get_dw(key, "InsertExtraSections").unwrap_or(0) != 0;
    sc.create_new_entry_point = get_dw(key, "CreateNewEntryPoint").unwrap_or(0) != 0;
    sc.create_fake_debug_directory = get_dw(key, "CreateFakeDebugDirectory").unwrap_or(0) != 0;
    p
}

unsafe fn write_profile(key: HKEY, p: &ProcessProfile) {
    set_dw(key, "Method", p.method);
    set_dw(key, "Delay", p.delay);
    set_dw(key, "DelayBetween", p.delay_between);
    set_dw(key, "AutoInject", p.auto_inject as u32);
    set_dw(key, "CloseOnInject", p.close_on_inject as u32);
    set_dw(key, "ErasePE", p.erase_pe as u32);
    set_dw(key, "HideModule", p.hide_module as u32);
    set_dw(key, "StealthInject", p.stealth_inject as u32);
    write_modules(key, &p.modules);
    set_dw(key, "DisableExceptionSupport", p.advanced.disable_exception_support as u32);
    set_dw(key, "DisableSEHValidation", p.advanced.disable_seh_validation as u32);
    set_dw(key, "HideFromDebugger", p.advanced.hide_from_debugger as u32);
    set_dw(key, "ManualResolveImports", p.advanced.manual_resolve_imports as u32);
    let sc = &p.scramble;
    set_dw(key, "ScrambleHeaderFields", sc.scramble_header_fields as u32);
    set_dw(key, "RemoveDebugData", sc.remove_debug_data as u32);
    set_dw(key, "RemoveUselessData", sc.remove_useless_data as u32);
    set_dw(key, "RenameSections", sc.rename_sections as u32);
    set_dw(key, "StripSectionCharacteristics", sc.strip_section_characteristics as u32);
    set_dw(key, "ShiftSectionData", sc.shift_section_data as u32);
    set_dw(key, "ShiftSectionMemory", sc.shift_section_memory as u32);
    set_dw(key, "ModifyAssemblyCode", sc.modify_assembly_code as u32);
    set_dw(key, "ModifyImportTable", sc.modify_import_table as u32);
    set_dw(key, "MoveRelocationTable", sc.move_relocation_table as u32);
    set_dw(key, "InsertExtraSections", sc.insert_extra_sections as u32);
    set_dw(key, "CreateNewEntryPoint", sc.create_new_entry_point as u32);
    set_dw(key, "CreateFakeDebugDirectory", sc.create_fake_debug_directory as u32);
}

unsafe fn enum_subkeys(key: HKEY) -> Vec<String> {
    let mut out = Vec::new();
    let mut index = 0u32;
    loop {
        let mut buf = [0u16; 256];
        let mut len = buf.len() as u32;
        let rc = RegEnumKeyExW(
            key,
            index,
            buf.as_mut_ptr(),
            &mut len,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        if rc != ERROR_SUCCESS {
            break;
        }
        if len > 0 {
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
        }
        index += 1;
    }
    out
}

/// A process name made safe for use as a registry key name.
pub fn profile_key_name(name: &str) -> String {
    let mut s: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    if s.is_empty() || s == "." || s == ".." {
        s = s.replace('.', "_");
    }
    s
}
