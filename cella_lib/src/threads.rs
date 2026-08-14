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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

static THREADS: OnceLock<usize> = OnceLock::new();
/// Process-local override; `0` means "no override". An atomic rather than a
/// `Mutex` because `thread_count()` is on the per-step path.
static THREAD_OVERRIDE: AtomicUsize = AtomicUsize::new(0);

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

fn resolve_thread_count_uncached() -> usize {
    if let Some(path) = find_properties_file() {
        if let Some(n) = parse_threads_from_props(&path) { return n; }
    }
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

/// Get the configured thread count for parallel stepping.
/// 
/// This first honors a process-local override (used by tests/benchmarks),
/// otherwise reads from cella.properties once per process and caches it.
pub fn thread_count() -> usize {
    let overridden = THREAD_OVERRIDE.load(Ordering::Relaxed);
    if overridden != 0 { return overridden; }
    *THREADS.get_or_init(resolve_thread_count_uncached)
}

/// Set a process-local override thread count (>=1) used by `thread_count()`.
/// Useful for tests/benchmarks to run with specific parallelism settings.
pub fn set_thread_override(n: usize) {
    THREAD_OVERRIDE.store(n.max(1), Ordering::Relaxed);
}

/// Clear the process-local override so `thread_count()` resumes using config.
pub fn clear_thread_override() {
    THREAD_OVERRIDE.store(0, Ordering::Relaxed);
}

/// Persistent worker pools, keyed by thread count.
///
/// Grids used to `std::thread::scope`-spawn fresh OS threads on every `step()`,
/// which cost more than the work it distributed for small and mid-sized grids.
/// Pools are created on first use for a given size and live for the process, so
/// stepping only pays for fork/join of already-parked workers. Leaked
/// deliberately: there are at most a handful of distinct thread counts per run.
static POOLS: OnceLock<Mutex<Vec<(usize, &'static rayon::ThreadPool)>>> = OnceLock::new();

/// Minimum estimated work (neighbor visits) one chunk must carry to be worth
/// handing to a worker.
///
/// Waking a parked worker is not free — measured at tens of microseconds on some
/// platforms, comparable to the OS thread spawn it replaced. So parallelism is
/// sized by *work*, not by grid size alone: a step is split into
/// `clamp(total_work / MIN_WORK_PER_CHUNK, 1, thread_count())` chunks. Cheap
/// rules on mid-sized grids therefore stay serial or use a couple of workers
/// instead of paying eight wakeups to save a few microseconds of compute.
pub const MIN_WORK_PER_CHUNK: usize = 400_000;

/// Process-local override for [`MIN_WORK_PER_CHUNK`]; `0` means "no override".
static MIN_WORK_OVERRIDE: AtomicUsize = AtomicUsize::new(0);

/// Lower (or raise) the work-per-chunk threshold for this process.
///
/// Tests use this to force the multi-threaded path on grids small enough to check
/// exhaustively — otherwise the work heuristic keeps them serial and the parallel
/// code paths go untested.
pub fn set_min_work_per_chunk_override(work: usize) {
    MIN_WORK_OVERRIDE.store(work.max(1), Ordering::Relaxed);
}

/// Clear the work-per-chunk override.
pub fn clear_min_work_per_chunk_override() {
    MIN_WORK_OVERRIDE.store(0, Ordering::Relaxed);
}

/// How many chunks a step estimated at `total_work` neighbor visits should be
/// split into. `1` means run serially on the calling thread.
pub(crate) fn chunks_for_work(total_work: usize) -> usize {
    let threads = thread_count();
    if threads <= 1 { return 1; }
    let min_work = match MIN_WORK_OVERRIDE.load(Ordering::Relaxed) {
        0 => MIN_WORK_PER_CHUNK,
        n => n,
    };
    (total_work / min_work).clamp(1, threads)
}

/// Fixed-slot cache for the common thread counts: `pool(n)` for `n <= 64` is a
/// single atomic load in steady state. Larger counts fall back to the
/// Mutex-guarded registry — `pool()` runs on every parallel step, and the old
/// lock-and-scan on each call was the same class of waste as the
/// `thread_count()` Mutex removed earlier (performance.md §3.6).
const POOL_SLOTS: usize = 64;
static POOL_CACHE: [std::sync::OnceLock<&'static rayon::ThreadPool>; POOL_SLOTS + 1] =
    [const { std::sync::OnceLock::new() }; POOL_SLOTS + 1];

fn build_pool(n: usize) -> &'static rayon::ThreadPool {
    Box::leak(Box::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .thread_name(move |i| format!("cella-step-{i}"))
            .build()
            .expect("build cella worker pool"),
    ))
}

/// Get (or build) the persistent worker pool with `n` threads.
pub(crate) fn pool(n: usize) -> &'static rayon::ThreadPool {
    if n <= POOL_SLOTS {
        return POOL_CACHE[n].get_or_init(|| build_pool(n));
    }
    let pools = POOLS.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = pools.lock().expect("pool registry lock");
    if let Some((_, p)) = guard.iter().find(|(k, _)| *k == n) { return p; }
    let built = build_pool(n);
    guard.push((n, built));
    built
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn parse_threads_rejects_nonpositive_and_invalid_values() {
        let _guard = test_lock().lock().unwrap();
        let dir = std::env::temp_dir();
        let path = dir.join(format!("cella_threads_{}_bad.properties", std::process::id()));
        {
            let mut file = std::fs::File::create(&path).unwrap();
            writeln!(file, "threads=0").unwrap();
            writeln!(file, "threads=bad").unwrap();
        }

        assert_eq!(parse_threads_from_props(&path), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn parse_threads_accepts_valid_value() {
        let _guard = test_lock().lock().unwrap();
        let dir = std::env::temp_dir();
        let path = dir.join(format!("cella_threads_{}_good.properties", std::process::id()));
        {
            let mut file = std::fs::File::create(&path).unwrap();
            writeln!(file, "threads=4").unwrap();
        }

        assert_eq!(parse_threads_from_props(&path), Some(4));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn find_properties_and_parse_missing_key_paths_are_covered() {
        let _guard = test_lock().lock().unwrap();
        let base = std::env::temp_dir().join(format!("cella_no_props_{}", std::process::id()));
        let deep = base.join("a").join("b").join("c").join("d").join("e");
        std::fs::create_dir_all(&deep).unwrap();

        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(&deep).unwrap();
        assert!(find_properties_file().is_none());
        let fallback = resolve_thread_count_uncached();
        assert!(fallback >= 1);
        std::env::set_current_dir(old).unwrap();
        let _ = std::fs::remove_dir_all(&base);

        let path = std::env::temp_dir().join(format!("cella_threads_{}_nokey.properties", std::process::id()));
        {
            let mut file = std::fs::File::create(&path).unwrap();
            writeln!(file, "workers=8").unwrap();
            writeln!(file, "threads").unwrap();
        }
        assert_eq!(parse_threads_from_props(&path), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn override_chunks_and_pool_paths_are_covered() {
        let _guard = test_lock().lock().unwrap();

        let baseline = thread_count();
        assert!(baseline >= 1);

        set_thread_override(3);
        assert_eq!(thread_count(), 3);

        set_min_work_per_chunk_override(2);
        assert_eq!(chunks_for_work(1), 1);
        assert_eq!(chunks_for_work(8), 3);

        clear_min_work_per_chunk_override();
        assert!(chunks_for_work(MIN_WORK_PER_CHUNK) >= 1);

        let p1 = pool(2) as *const _;
        let p2 = pool(2) as *const _;
        assert_eq!(p1, p2);

        // The > POOL_SLOTS fallback goes through the Mutex registry and must
        // also return the same instance on repeat calls.
        let big1 = pool(POOL_SLOTS + 1) as *const _;
        let big2 = pool(POOL_SLOTS + 1) as *const _;
        assert_eq!(big1, big2);
        assert_eq!(pool(POOL_SLOTS + 1).current_num_threads(), POOL_SLOTS + 1);

        clear_thread_override();
    }

    #[test]
    fn find_properties_and_parse_error_and_success_edges() {
        let _guard = test_lock().lock().unwrap();

        // At filesystem root, `pop()` returns false and the search loop exits.
        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir("/").unwrap();
        let _ = find_properties_file();
        std::env::set_current_dir(old).unwrap();

        // Missing file path should fail to read and return None.
        let missing = std::env::temp_dir().join("cella_missing_threads.properties");
        assert_eq!(parse_threads_from_props(&missing), None);

        // Ensure uncached resolver can return a configured value from discovered properties.
        let base = std::env::temp_dir().join(format!("cella_props_ok_{}", std::process::id()));
        let deep = base.join("x").join("y");
        std::fs::create_dir_all(&deep).unwrap();
        let props = base.join("cella.properties");
        std::fs::write(&props, "threads=3\n").unwrap();
        let old2 = std::env::current_dir().unwrap();
        std::env::set_current_dir(&deep).unwrap();
        assert_eq!(resolve_thread_count_uncached(), 3);
        std::env::set_current_dir(old2).unwrap();
        let _ = std::fs::remove_file(props);
        let _ = std::fs::remove_dir_all(base);
    }
}

