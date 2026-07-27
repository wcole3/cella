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

/// Get the configured thread count for parallel stepping.
/// 
/// This first honors a process-local override (used by tests/benchmarks),
/// otherwise reads from cella.properties once per process and caches it.
pub fn thread_count() -> usize {
    let overridden = THREAD_OVERRIDE.load(Ordering::Relaxed);
    if overridden != 0 { return overridden; }
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

/// Get (or build) the persistent worker pool with `n` threads.
pub(crate) fn pool(n: usize) -> &'static rayon::ThreadPool {
    let pools = POOLS.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = pools.lock().expect("pool registry lock");
    if let Some((_, p)) = guard.iter().find(|(k, _)| *k == n) { return p; }
    let built: &'static rayon::ThreadPool = Box::leak(Box::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .thread_name(move |i| format!("cella-step-{i}"))
            .build()
            .expect("build cella worker pool"),
    ));
    guard.push((n, built));
    built
}
