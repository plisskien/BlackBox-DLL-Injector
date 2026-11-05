//! Background auto-injection polling worker.
//! Monitors for target process creation and dispatches injection queues.

use crate::queue::{self, QueueOptions};
use crate::ui::{Shared, WM_APP_LOG};
use crate::win32::user32::PostMessageW;
use std::collections::HashSet;
use std::sync::Arc;

const POLL_INTERVAL_MS: u64 = 250;
const WINDOW_WAIT_MS: u32 = 8_000;

pub fn auto_inject_loop(shared: Arc<std::sync::Mutex<Shared>>) {
    let mut injected_pids: HashSet<u32> = HashSet::new();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));

        let snapshot = {
            let sh = shared.lock().unwrap();
            if !sh.auto_inject || sh.process_name.is_empty() {
                continue;
            }
            (
                sh.hwnd,
                sh.process_name.clone(),
                sh.modules.clone(),
                sh.delay,
                sh.delay_between,
                sh.opts.clone(),
                sh.close_on_inject,
                sh.auto_wait_window,
                sh.auto_retry,
                sh.inject_children,
                sh.auto_child_filter.clone(),
                sh.auto_window_title.clone(),
            )
        };
        let (hwnd_usize, process_name, modules, delay, delay_between, opts, close_on_inject, wait_window, auto_retry, inject_children, child_filter, window_title) = snapshot;
        let hwnd = hwnd_usize as crate::win32::HWND;
        let enabled = modules.iter().filter(|m| m.enabled).count();
        if enabled == 0 {
            continue;
        }

        let found = match crate::process::find_by_name(&process_name) {
            Some(p) => p,
            None => continue,
        };
        if injected_pids.contains(&found.pid) {
            continue;
        }

        let log = |line: &str| post_log(hwnd, line);
        log(&format!("[*] Auto-inject: {} [{}]", found.name, found.pid));

        // Games often show the process before their window exists
        if wait_window {
            if window_title.trim().is_empty() {
                log("[*] Waiting for target window...");
                if !queue::wait_for_window(found.pid, WINDOW_WAIT_MS, &log) {
                    log("[*] Window wait timed out, injecting");
                }
            } else {
                log(&format!("[*] Waiting for window '{}'...", window_title));
                if !queue::wait_for_window_title(found.pid, &window_title, WINDOW_WAIT_MS, &log) {
                    log("[*] Window wait timed out, injecting");
                }
            }
        }

        let qopts = QueueOptions {
            start_delay: delay,
            between_delay: delay_between,
            base: opts.clone(),
            retries: if auto_retry { 2 } else { 0 },
        };

        let first = queue::run(found.pid, &modules, &qopts, &log);
        let mut any_failure = !first.failed.is_empty();

        // Child processes of a launcher (only ones that own a window, so we do not inject
        // into helpers and crash handlers)
        if inject_children {
            let child_filter = child_filter.to_lowercase();
            for child in queue::child_pids(found.pid) {
                if injected_pids.contains(&child) || !queue::process_has_window(child) {
                    continue;
                }
                if !child_filter.is_empty() {
                    let name = crate::process::enumerate()
                        .into_iter()
                        .find(|p| p.pid == child)
                        .map(|p| p.name.to_lowercase())
                        .unwrap_or_default();
                    if !name.contains(&child_filter) {
                        continue;
                    }
                }
                log(&format!("[*] Auto-inject: child {child}"));
                let child_run = queue::run(child, &modules, &qopts, &log);
                any_failure |= !child_run.failed.is_empty();
                injected_pids.insert(child);
            }
        }

        injected_pids.insert(found.pid);

        // Expire pids that no longer exist so relaunches get re-injected.
        let alive: HashSet<u32> = crate::process::enumerate().into_iter().map(|p| p.pid).collect();
        injected_pids.retain(|pid| alive.contains(pid));

        if any_failure {
            log("[!] Auto-inject: errors occurred");
        } else {
            log("[+] Auto-inject complete");
            if close_on_inject {
                unsafe {
                    PostMessageW(hwnd, 0x0010 /* WM_CLOSE */, 0, 0);
                }
            }
        }
    }
}

fn post_log(hwnd: crate::win32::HWND, line: &str) {
    unsafe {
        PostMessageW(hwnd, WM_APP_LOG, Box::into_raw(Box::new(line.to_string())) as usize, 0);
    }
}
