// Build script: embed the application manifest, icon and version resource.
//
// Release builds get app.manifest, which requests requireAdministrator - the injector always
// runs elevated. The `no_elevation` feature embeds app.noelevate.manifest (asInvoker) instead,
// so the automated tests can launch it from an unelevated session.
//
// The icon/version resource needs Microsoft's rc.exe. It is optional: if the Windows SDK is
// not installed the build still succeeds (just without the executable icon).

use std::path::{Path, PathBuf};

fn find_rc() -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Ok(base) = std::env::var(var) {
            roots.push(PathBuf::from(base).join("Windows Kits").join("10").join("bin"));
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for entry in entries.flatten() {
            let p = entry.path().join("x64").join("rc.exe");
            if p.is_file() {
                candidates.push(p);
            }
        }
    }
    candidates.sort();
    candidates.pop()
}

fn get_version() -> (String, (u32, u32, u32, u32)) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let paths = [
        manifest_dir.join("..").join("version.txt"),
        manifest_dir.join("version.txt"),
    ];
    for p in &paths {
        if let Ok(content) = std::fs::read_to_string(p) {
            let v = content.trim().trim_start_matches(['v', 'V']).to_string();
            if !v.is_empty() {
                let parts: Vec<u32> = v.split('.').filter_map(|s| s.parse().ok()).collect();
                let nums = (
                    parts.first().copied().unwrap_or(1),
                    parts.get(1).copied().unwrap_or(0),
                    parts.get(2).copied().unwrap_or(0),
                    parts.get(3).copied().unwrap_or(0),
                );
                return (v, nums);
            }
        }
    }
    let pkg_v = env!("CARGO_PKG_VERSION");
    let parts: Vec<u32> = pkg_v.split('.').filter_map(|s| s.parse().ok()).collect();
    (pkg_v.to_string(), (
        parts.first().copied().unwrap_or(1),
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
        0,
    ))
}

fn embed_icon_and_version(out_dir: &Path, ver_str: &str, v: (u32, u32, u32, u32)) {
    let ico = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join("blackbox.ico");
    if !ico.is_file() {
        return;
    }
    let Some(rc_exe) = find_rc() else { return };

    // rc.exe wants doubled backslashes inside the quoted resource path
    let ico_rc = ico.display().to_string().replace('\\', "\\\\");
    let rc_file = out_dir.join("app.rc");
    let res_file = out_dir.join("app.res");
    let script = format!(
        "1 ICON \"{ico_rc}\"\r\n\
         1 VERSIONINFO\r\n\
         FILEVERSION {},{},{},{}\r\n\
         PRODUCTVERSION {},{},{},{}\r\n\
         FILEFLAGSMASK 0x3fL\r\n\
         FILEFLAGS 0x0L\r\n\
         FILEOS 0x40004L\r\n\
         FILETYPE 0x1L\r\n\
         FILESUBTYPE 0x0L\r\n\
         BEGIN\r\n\
             BLOCK \"StringFileInfo\"\r\n\
             BEGIN\r\n\
                 BLOCK \"040904b0\"\r\n\
                 BEGIN\r\n\
                     VALUE \"CompanyName\", \"hackvshack.net\"\r\n\
                     VALUE \"FileDescription\", \"Pure Rust Windows DLL injector (Manual Map, Hijack, APC, Syscalls) with automated WER crash diagnostics for x64 and WOW64. Zero dependencies.\"\r\n\
                     VALUE \"FileVersion\", \"{ver_str}\"\r\n\
                     VALUE \"InternalName\", \"BlackBox\"\r\n\
                     VALUE \"OriginalFilename\", \"BlackBoxInjector.exe\"\r\n\
                     VALUE \"ProductName\", \"BlackBox\"\r\n\
                     VALUE \"ProductVersion\", \"{ver_str}\"\r\n\
                 END\r\n\
             END\r\n\
             BLOCK \"VarFileInfo\"\r\n\
             BEGIN\r\n\
                 VALUE \"Translation\", 0x409, 1200\r\n\
             END\r\n\
         END\r\n",
        v.0, v.1, v.2, v.3, v.0, v.1, v.2, v.3
    );
    if std::fs::write(&rc_file, script).is_err() {
        return;
    }
    let ok = std::process::Command::new(&rc_exe)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res_file)
        .arg(&rc_file)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok && res_file.is_file() {
        println!("cargo:rustc-link-arg={}", res_file.display());
        println!("cargo:rerun-if-changed={}", ico.display());
    }
}

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let (ver_str, (v1, v2, v3, v4)) = get_version();
    println!("cargo:rustc-env=BLACKBOX_VERSION={ver_str}");

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_v = manifest_dir.join("..").join("version.txt");
    if root_v.is_file() {
        println!("cargo:rerun-if-changed={}", root_v.display());
    }
    let local_v = manifest_dir.join("version.txt");
    if local_v.is_file() {
        println!("cargo:rerun-if-changed={}", local_v.display());
    }

    embed_icon_and_version(&out_dir, &ver_str, (v1, v2, v3, v4));

    let manifest = if std::env::var("CARGO_FEATURE_NO_ELEVATION").is_ok() {
        "app.noelevate.manifest"
    } else {
        "app.manifest"
    };
    let manifest_path = manifest_dir.join(manifest);
    if manifest_path.is_file() {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTUAC:NO");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest_path.display());
        println!("cargo:rerun-if-changed={}", manifest_path.display());
    }

    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_NO_ELEVATION");
    println!("cargo:rerun-if-changed=build.rs");
}
