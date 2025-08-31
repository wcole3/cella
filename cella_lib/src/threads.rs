//! Thread configuration for parallel stepping.
//! 
//! This module exposes `thread_count()` which returns how many threads
//! the engine should use when stepping grids. The value is loaded once
//! from a simple properties file named `cella.properties` located at
//! the repository root (or any parent directory of the current working
//! directory), with a key:
//!
//!   threads=NUM
//!
//! If the file or key is missing or invalid, the function falls back to
//! `std::thread::available_parallelism()` (or 1 on error).
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static THREADS: OnceLock<usize> = OnceLock::new();
static THREAD_OVERRIDE: OnceLock<Mutex<Option<usize>>> = OnceLock::new();
fn override_slot() -> &'static Mutex<Option<usize>> { THREAD_OVERRIDE.get_or_init(|| Mutex::new(None)) }

fn find_properties_file() -> Option<PathBuf> {
    // Start from current_dir and walk up a few levels to find `cella.properties`.
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..5 {
        let candidate = dir.join("cella.properties");
        if candidate.exists() { return Some(candidate); }
        if !dir.pop() { break; }
    }
    None
}

fn parse_threads_from_props(path: &Path) -> Option<usize> {
    let data = fs::read_to_string(path).ok()?;
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") { continue; }
        if let Some(eq) = line.find('=') {
            let (k, v) = line.split_at(eq);
            let key = k.trim();
            let val = v.trim_start_matches('=').trim();
            if key.eq_ignore_ascii_case("threads") {
                if let Ok(n) = val.parse::<usize>() {
                    if n >= 1 { return Some(n); }
                }
            }
        }
    }
    None
}

/// Get the configured thread count for parallel stepping.
/// 
/// This first honors a process-local override (used by tests/benchmarks),
/// otherwise reads from cella.properties once per process and caches it.
pub fn thread_count() -> usize {
    if let Some(n) = *override_slot().lock().expect("thread override lock") { return n.max(1); }
    *THREADS.get_or_init(|| {
        if let Some(path) = find_properties_file() {
            if let Some(n) = parse_threads_from_props(&path) { return n; }
        }
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    })
}

/// Set a process-local override thread count (>=1) used by `thread_count()`.
/// Useful for tests/benchmarks to run with specific parallelism settings.
pub fn set_thread_override(n: usize) {
    let mut slot = override_slot().lock().expect("thread override lock");
    *slot = Some(n.max(1));
}

/// Clear the process-local override so `thread_count()` resumes using config.
pub fn clear_thread_override() {
    let mut slot = override_slot().lock().expect("thread override lock");
    *slot = None;
}
