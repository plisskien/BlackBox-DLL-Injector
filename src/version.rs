//! Application version definition and window title formatting.
//!
//! Single source of truth: `version.txt` in the repository root.
//! The build script reads `version.txt`, propagates it into the PE version resource,
//! and defines `BLACKBOX_VERSION` at compile-time.
//! The update checker compares the contents of GitHub's `version.txt` against this version.

/// Current version string from `version.txt`.
pub const VERSION: &str = env!("BLACKBOX_VERSION");

/// Consistent window title formatter across the entire application:
/// "BlackBox Injector v{VERSION} - {Page}"
pub fn window_title(page: &str) -> String {
    let p = page.trim();
    if p.is_empty() {
        format!("BlackBox Injector v{VERSION} - Main")
    } else {
        format!("BlackBox Injector v{VERSION} - {p}")
    }
}

