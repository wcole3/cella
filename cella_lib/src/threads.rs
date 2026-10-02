//! Thread configuration for parallel stepping.
//!
//! The engine steps big grids on several worker threads at once. A grid is cut
//! into *chunks* (contiguous runs of cells), each chunk goes to one worker, and
//! the step finishes when every worker is done (a *fork/join*: fork the work
//! out, join the results back). This module decides how many threads exist and
//! how many chunks a step is worth.
//!
//! [`thread_count`] returns how many threads the engine should use. The value
//! is resolved once per process from a simple properties file named
//! `cella.properties`, looked for in the current working directory and up to
//! four parent directories above it (the nearest one wins). The line is:
//!
//!   threads=NUM
//!
//! (`NUM` must be a whole number >= 1; the key is case-insensitive and lines
//! starting with `#` or `//` are comments.) If no file is found or the key is
//! missing or invalid, it falls back to `std::thread::available_parallelism()`
//! (or 1 if that fails).
//!
//! Two more knobs, both read once from the environment and both no-ops
//! unless set (see docs/performance.md §9, "Ensemble stepping parallelism",
//! for why they exist): `CELLA_MIN_WORK=<work units>` lowers or raises
//! [`MIN_WORK_PER_CHUNK`] for this process, and `CELLA_MEMBER_PAR=<n>` caps how
//! many ensemble members [`crate::explore::Ensemble::step`] steps concurrently
//! instead of letting its own grid-size heuristic decide.
//!
//! Precedence, highest first: an explicit call in code
//! ([`set_min_work_per_chunk_override`], [`set_member_par_override`]), then the
//! environment variable, then the built-in default. The code call wins even if
//! it happens before the environment variable would first have been read.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

static THREADS: OnceLock<usize> = OnceLock::new();
/// Process-local override; `0` means "no override". An atomic rather than a
/// `Mutex` because `thread_count()` is on the per-step path (docs/performance.md §3.6).
static THREAD_OVERRIDE: AtomicUsize = AtomicUsize::new(0);

fn find_properties_file() -> Option<PathBuf> {
    // Start from the current directory and walk up (at most 4 parents) to find
    // `cella.properties`; the nearest file wins.
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..5 {
        let candidate = dir.join("cella.properties");
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

fn parse_threads_from_props(path: &Path) -> Option<usize> {
    let data = fs::read_to_string(path).ok()?;
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if let Some(eq) = line.find('=') {
            let (k, v) = line.split_at(eq);
            let key = k.trim();
            let val = v.trim_start_matches('=').trim();
            if key.eq_ignore_ascii_case("threads")
                && let Ok(n) = val.parse::<usize>()
                && n >= 1
            {
                return Some(n);
            }
        }
    }
    None
}

fn resolve_thread_count_uncached() -> usize {
    if let Some(path) = find_properties_file()
        && let Some(n) = parse_threads_from_props(&path)
    {
        return n;
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Get the configured thread count for parallel stepping (always >= 1).
///
/// This first honors a process-local override (used by tests/benchmarks).
/// Otherwise it resolves the `cella.properties` / `available_parallelism`
/// value once per process and caches it.
pub fn thread_count() -> usize {
    let overridden = THREAD_OVERRIDE.load(Ordering::Relaxed);
    if overridden != 0 {
        return overridden;
    }
    *THREADS.get_or_init(resolve_thread_count_uncached)
}

/// Set a process-local override thread count used by `thread_count()` (`0` is
/// raised to 1). Useful for tests/benchmarks to run with specific parallelism
/// settings. Tests must hold `lock_override_for_test` while doing so.
pub fn set_thread_override(n: usize) {
    THREAD_OVERRIDE.store(n.max(1), Ordering::Relaxed);
}

/// Clear the process-local override so `thread_count()` resumes using config.
pub fn clear_thread_override() {
    THREAD_OVERRIDE.store(0, Ordering::Relaxed);
}

/// Persistent worker pools, keyed by thread count.
///
/// Spawning fresh OS threads on every `step()` (as the engine once did with
/// `std::thread::scope`) cost more than the work it handed out on small and
/// mid-sized grids. So a pool (a set of long-lived rayon worker threads) is
/// created on first use for each thread count and lives for the rest of the
/// process; a step only pays to wake already-parked workers. The pools are
/// leaked on purpose (`Box::leak`): a run uses at most a handful of distinct
/// thread counts.
///
/// This `Mutex<Vec>` registry is the slow path for counts above [`POOL_SLOTS`];
/// smaller counts use [`POOL_CACHE`].
static POOLS: OnceLock<Mutex<Vec<(usize, &'static rayon::ThreadPool)>>> = OnceLock::new();

/// Minimum estimated work (neighbor visits) one chunk must carry to be worth
/// handing to a worker.
///
/// Waking a parked worker is not free — measured at tens of microseconds on some
/// platforms (about 30-50 µs on the WSL2 reference machine), comparable to the
/// OS thread spawn it replaced. So parallelism is sized by *work*, not by grid
/// size alone: a step is split into
/// `clamp(total_work / MIN_WORK_PER_CHUNK, 1, thread_count())` chunks, where
/// `total_work` is `cells * work_per_cell` (see `chunks_for_work`). Example:
/// with the default 400 000, a step estimated at 1 000 000 neighbor visits gets
/// 2 chunks, and anything under 800 000 stays serial. Cheap rules on
/// mid-sized grids therefore stay serial or use a couple of workers instead of
/// paying eight wakeups to save a few microseconds of compute. See
/// docs/performance.md §3.2.
pub const MIN_WORK_PER_CHUNK: usize = 400_000;

/// Process-local override for [`MIN_WORK_PER_CHUNK`]; `0` means "no override".
static MIN_WORK_OVERRIDE: AtomicUsize = AtomicUsize::new(0);

/// Lower (or raise) the work-per-chunk threshold for this process.
///
/// Tests use this to force the multi-threaded path on grids small enough to check
/// exhaustively — otherwise the work heuristic keeps them serial and the parallel
/// code paths go untested. (`0` is raised to 1.) Tests must hold
/// `lock_override_for_test` while doing so.
///
/// An explicit call here always beats the `CELLA_MIN_WORK` environment
/// variable, whether it runs before or after the first [`chunks_for_work`].
pub fn set_min_work_per_chunk_override(work: usize) {
    // Let a pending `CELLA_MIN_WORK` land first, so the explicit value below
    // is stored after it and wins.
    apply_min_work_env_once();
    MIN_WORK_OVERRIDE.store(work.max(1), Ordering::Relaxed);
}

/// Clear the work-per-chunk override, so [`MIN_WORK_PER_CHUNK`] applies again.
/// This also discards any value that `CELLA_MIN_WORK` had put there (and the
/// variable is not read again afterwards).
pub fn clear_min_work_per_chunk_override() {
    apply_min_work_env_once();
    MIN_WORK_OVERRIDE.store(0, Ordering::Relaxed);
}

/// Set once `CELLA_MIN_WORK` has been read into [`MIN_WORK_OVERRIDE`].
///
/// The variable is applied at most once per process, the first time
/// [`chunks_for_work`], [`set_min_work_per_chunk_override`] or
/// [`clear_min_work_per_chunk_override`] runs, whichever comes first. Because
/// the two setters apply it *before* they store their own value, an explicit
/// call in code always beats the environment variable, in any order.
///
/// This is a thin bootstrap over the override, not a second knob: a one-off
/// benchmark process sets the env var before it does any work and never
/// changes it, so "read once" is enough and keeps the hot path
/// (`chunks_for_work` runs on every step) down to one atomic load after the
/// first call. A plain flag plus [`MIN_WORK_ENV_LOCK`] is used instead of a
/// `OnceLock` so a test can reset it.
static MIN_WORK_ENV_APPLIED: AtomicBool = AtomicBool::new(false);

/// Makes sure two threads do not both run the first-time env read.
static MIN_WORK_ENV_LOCK: Mutex<()> = Mutex::new(());

/// Parses the raw value of one of these env knobs: `Some(n)` for a positive
/// `usize`, `None` for anything unusable (absent, not a number, or zero).
/// Zero is meaningless as a chunk threshold or a member-batch size, so it is
/// treated as "unset" here, whereas the direct `set_*_override` calls quietly
/// raise 0 to 1. Split out from `apply_*_env_once` so the parsing can be tested
/// without the once-per-process `OnceLock`.
fn parse_env_override(raw: Option<String>) -> Option<usize> {
    raw.and_then(|v| v.parse::<usize>().ok()).filter(|&n| n >= 1)
}

/// Reads `var` from the environment and, if it parses to a positive
/// `usize`, stores it in `target`. Otherwise `target` is left untouched.
/// Not gated by a `OnceLock` itself, so a test can call it directly with
/// `std::env::set_var`; the once-per-process gating lives one level up, in
/// `apply_*_env_once` (the same split as `resolve_thread_count_uncached` and
/// `thread_count`).
fn apply_env_override(var: &str, target: &AtomicUsize) {
    if let Some(n) = parse_env_override(std::env::var(var).ok()) {
        target.store(n, Ordering::Relaxed);
    }
}

fn apply_min_work_env_once() {
    if MIN_WORK_ENV_APPLIED.load(Ordering::Acquire) {
        return;
    }
    let _guard = MIN_WORK_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !MIN_WORK_ENV_APPLIED.load(Ordering::Relaxed) {
        apply_env_override("CELLA_MIN_WORK", &MIN_WORK_OVERRIDE);
        MIN_WORK_ENV_APPLIED.store(true, Ordering::Release);
    }
}

/// How many chunks a step estimated at `total_work` neighbor visits should be
/// split into: `clamp(total_work / min_work, 1, thread_count())`, where
/// `min_work` is [`MIN_WORK_PER_CHUNK`] unless overridden. `1` means run
/// serially on the calling thread; with a single configured thread the answer is
/// always `1`.
pub(crate) fn chunks_for_work(total_work: usize) -> usize {
    apply_min_work_env_once();
    let threads = thread_count();
    if threads <= 1 {
        return 1;
    }
    let min_work = match MIN_WORK_OVERRIDE.load(Ordering::Relaxed) {
        0 => MIN_WORK_PER_CHUNK,
        n => n,
    };
    (total_work / min_work).clamp(1, threads)
}

/// Process-local override: the maximum number of ensemble members
/// [`crate::explore::Ensemble::step`] batches together for concurrent
/// stepping. `0` means "no override" — the engine's own size heuristic
/// decides. See docs/performance.md §9 ("Ensemble stepping parallelism",
/// 2026-09-12) for why this exists and what it changes (nothing, unless it or
/// `CELLA_MIN_WORK` is set).
static MEMBER_PAR_OVERRIDE: AtomicUsize = AtomicUsize::new(0);

/// Set a process-local override for how many ensemble members step
/// concurrently (`0` is raised to 1). Used by tests/benchmarks; mirrors
/// [`set_thread_override`], and tests must hold `lock_override_for_test` too.
pub fn set_member_par_override(n: usize) {
    // Same rule as the min-work override: apply `CELLA_MEMBER_PAR` first so
    // this explicit value wins.
    apply_member_par_env_once();
    MEMBER_PAR_OVERRIDE.store(n.max(1), Ordering::Relaxed);
}

/// Clear the member-parallelism override so the engine's own heuristic
/// decides again.
pub fn clear_member_par_override() {
    apply_member_par_env_once();
    MEMBER_PAR_OVERRIDE.store(0, Ordering::Relaxed);
}

/// Applies `CELLA_MEMBER_PAR` to [`MEMBER_PAR_OVERRIDE`] once per process,
/// the first time [`member_par_override`] (or a setter) runs. Same bootstrap-only
/// reasoning as [`apply_min_work_env_once`].
static MEMBER_PAR_ENV_APPLIED: OnceLock<()> = OnceLock::new();

fn apply_member_par_env_once() {
    MEMBER_PAR_ENV_APPLIED
        .get_or_init(|| apply_env_override("CELLA_MEMBER_PAR", &MEMBER_PAR_OVERRIDE));
}

/// The current member-parallelism override, if any. `Ensemble::step` calls
/// this once per step; `None` means "let the engine's own heuristic decide",
/// which is the case whenever neither `set_member_par_override` nor
/// `CELLA_MEMBER_PAR` has been used in this process.
pub(crate) fn member_par_override() -> Option<usize> {
    apply_member_par_env_once();
    match MEMBER_PAR_OVERRIDE.load(Ordering::Relaxed) {
        0 => None,
        n => Some(n),
    }
}

/// Fixed-slot cache for the common thread counts: `pool(n)` for `n <= 64` is a
/// single atomic load in steady state (slot `n` of [`POOL_CACHE`], so slot 0
/// is unused). Larger counts fall back to the Mutex-guarded [`POOLS`] registry.
/// `pool()` runs on every parallel step, and the old lock-and-scan on each call
/// was the same class of waste as the `thread_count()` Mutex removed earlier
/// (docs/performance.md §3.10 and §3.6).
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

/// Get (or build) the persistent worker pool with `n` threads. The pool is
/// shared and lives for the whole process.
pub(crate) fn pool(n: usize) -> &'static rayon::ThreadPool {
    if n <= POOL_SLOTS {
        return POOL_CACHE[n].get_or_init(|| build_pool(n));
    }
    let pools = POOLS.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = pools.lock().expect("pool registry lock");
    if let Some((_, p)) = guard.iter().find(|(k, _)| *k == n) {
        return p;
    }
    let built = build_pool(n);
    guard.push((n, built));
    built
}

/// Test-only lock for the process-global overrides above.
///
/// `cargo test` runs tests on many threads in one process, but
/// [`set_thread_override`] and friends change a single global. If two tests
/// set it at once, one can clear it while the other is mid-run, and a "4
/// thread" run silently becomes a default-thread run. Every in-crate test that
/// sets an override (in this module or any other) holds this guard for its
/// whole body; take it via `lock_override_for_test`.
#[cfg(test)]
static OVERRIDE_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Take [`OVERRIDE_TEST_LOCK`]. A test that panics while holding the lock
/// "poisons" it; we ignore that so one failure does not fail every later test.
#[cfg(test)]
pub(crate) fn lock_override_for_test() -> std::sync::MutexGuard<'static, ()> {
    OVERRIDE_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};

    /// Serializes the tests in this module that change the working directory
    /// or read config files. Tests that set the thread / work overrides use
    /// `lock_override_for_test` instead, which also guards other modules.
    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn parse_threads_rejects_nonpositive_and_invalid_values() {
        let _guard = test_lock().lock().unwrap();
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "cella_threads_{}_bad.properties",
            std::process::id()
        ));
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
        let path = dir.join(format!(
            "cella_threads_{}_good.properties",
            std::process::id()
        ));
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

        let path = std::env::temp_dir().join(format!(
            "cella_threads_{}_nokey.properties",
            std::process::id()
        ));
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
        let _guard = lock_override_for_test();

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
    fn parse_env_override_rejects_absent_invalid_and_zero() {
        assert_eq!(parse_env_override(None), None);
        assert_eq!(parse_env_override(Some("bad".into())), None);
        assert_eq!(parse_env_override(Some("0".into())), None);
        assert_eq!(parse_env_override(Some("5".into())), Some(5));
    }

    #[test]
    fn apply_env_override_reads_the_named_var_into_the_target_atomic() {
        // Exercises the same read-env-then-store logic `CELLA_MIN_WORK`/
        // `CELLA_MEMBER_PAR` use, without touching either of their
        // once-per-process `OnceLock`s (see `apply_env_override`'s doc
        // comment) — a var name unrelated to any real knob, so this can
        // run in any order relative to the other tests here.
        let _guard = test_lock().lock().unwrap();
        let var = "CELLA_TEST_APPLY_ENV_OVERRIDE_UNUSED";
        let target = AtomicUsize::new(0);

        // Absent: left at 0.
        unsafe { std::env::remove_var(var) };
        apply_env_override(var, &target);
        assert_eq!(target.load(Ordering::Relaxed), 0);

        // Invalid: left unchanged.
        unsafe { std::env::set_var(var, "not-a-number") };
        apply_env_override(var, &target);
        assert_eq!(target.load(Ordering::Relaxed), 0);

        // Valid: stored.
        unsafe { std::env::set_var(var, "7") };
        apply_env_override(var, &target);
        assert_eq!(target.load(Ordering::Relaxed), 7);

        unsafe { std::env::remove_var(var) };
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

    #[test]
    fn code_override_beats_min_work_env_var() {
        let _guard = lock_override_for_test();
        let old_env = std::env::var("CELLA_MIN_WORK").ok();
        let old_threads = THREAD_OVERRIDE.load(Ordering::Relaxed);
        // Pretend the env var has not been read yet, as in a fresh process.
        MIN_WORK_ENV_APPLIED.store(false, Ordering::Release);

        // The env var says 1_000_000 per chunk; code says 10. With 4 threads and
        // 40 units of work, only the code value gives 4 chunks (40 / 1_000_000
        // would be 0, clamped to 1).
        unsafe { std::env::set_var("CELLA_MIN_WORK", "1000000") };
        set_min_work_per_chunk_override(10);
        set_thread_override(4);
        let chunks = chunks_for_work(40);

        // Restore the process-global state before asserting.
        clear_min_work_per_chunk_override();
        THREAD_OVERRIDE.store(old_threads, Ordering::Relaxed);
        // SAFETY: tests that touch overrides/env hold `lock_override_for_test`.
        unsafe {
            old_env.map_or_else(
                || std::env::remove_var("CELLA_MIN_WORK"),
                |v| std::env::set_var("CELLA_MIN_WORK", v),
            )
        };
        assert_eq!(chunks, 4);
    }
}
