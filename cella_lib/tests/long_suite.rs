// Long-running stress tests for cella_lib (ignored by default).
//
// How to use:
// - List ignored tests: cargo test -p cella_lib -- --ignored --list
// - Run all ignored tests: cargo test -p cella_lib -- --ignored
// - Update snapshots (writes tests/snapshots/*.txt):
//     CELLA_UPDATE_SNAPSHOTS=1 cargo test -p cella_lib -- --ignored
//   On Windows PowerShell:
//     $env:CELLA_UPDATE_SNAPSHOTS=1; cargo test -p cella_lib -- --ignored; Remove-Item Env:CELLA_UPDATE_SNAPSHOTS
// - Optional benchmark-friendly order: add --test-threads=1 so the summary test
//   runs last (named zzz_benchmark_summary). Example:
//     cargo test -p cella_lib -- --ignored --test-threads=1 --show-output
// - To update benchmark baselines (tests/benchmarks_last.json), set:
//     CELLA_UPDATE_BENCH=1 cargo test -p cella_lib -- --ignored --test-threads=1
//   On Windows PowerShell:
//     $env:CELLA_UPDATE_BENCH=1; cargo test -p cella_lib -- --ignored --test-threads=1; Remove-Item Env:CELLA_UPDATE_BENCH
//
// The snapshots are deterministic hashes of the final grid state after a large
// number of steps. If engine behavior changes (intentionally or not), the hash
// will differ, prompting a snapshot update.
//
// Benchmarks: Each test records elapsed wall time. A final ignored test
// `zzz_benchmark_summary` prints a summary of all recorded times. You can also
// set CELLA_BENCH=1 to print per-test timings immediately.
//
// Benchmark protocol (see "Benchmark harness" below and docs/performance.md
// section 4 for the why):
// - CELLA_BENCH_WARMUP=<n>  untimed warm-up runs before the timed ones
//                           (default 1; the first run is cold-cache slow).
// - CELLA_BENCH_RUNS=<n>    timed runs per bench (default 10).
// - Every bench reports mean +/- std (all timed runs), min, median, and the
//   runs flagged as outliers (median +/- max(3*1.4826*MAD, 2 % of median)).
//   Outliers are reported, never dropped. The summary prints the change vs
//   the baseline on min AND mean; trust min (noise only slows runs down).
// - /proc/loadavg is read at suite start and after each bench (Linux only;
//   silently skipped elsewhere). A 1-minute load above 4 prints a warning:
//   the machine is too busy for trustworthy timings.
// - For A/B comparisons of two builds use `make bench-ab` (interleaved runs,
//   Mann-Whitney test); do not eyeball two separate suite runs.
//
// ASCII dumps: Set CELLA_ASCII=1 to write ASCII renders of the initial and
// final states for each test to text files under tests/ascii/<testname>.txt.
//
// Config export: Set CELLA_EXPORT_CONFIGS=1 to write a JSON config file for
// each long test to configs/<testname>.json at the project root. These files
// can be loaded in the GUI to inspect the test case visually.
//   On Windows PowerShell:
//     $env:CELLA_EXPORT_CONFIGS=1; cargo test -p cella_lib -- --ignored; Remove-Item Env:CELLA_EXPORT_CONFIGS

use cella_lib::config::{CellaConfig, Config2D};
use cella_lib::threads::{clear_thread_override, set_thread_override, thread_count};
use cella_lib::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325; // FNV offset basis
    let prime: u64 = 0x00000100000001B3; // FNV prime
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(prime);
    }
    h
}

fn hash_grid1d_state(g: &Grid1D) -> u64 {
    let mut acc: u64 = 0;
    acc ^= fnv1a64(&g.width.to_le_bytes());
    acc ^= fnv1a64(&g.step.to_le_bytes());
    for i in 0..g.width {
        acc ^= fnv1a64(g.cell_type(i).as_str().as_bytes());
        acc = acc.wrapping_add(g.cell_age(i) as u64);
    }
    acc
}

fn hash_grid2d_state(g: &Grid2D) -> u64 {
    let mut acc: u64 = 0;
    acc ^= fnv1a64(&g.width.to_le_bytes());
    acc ^= fnv1a64(&g.height.to_le_bytes());
    acc ^= fnv1a64(&g.step.to_le_bytes());
    let total = g.width * g.height;
    for i in 0..total {
        acc ^= fnv1a64(g.cell_type(i).as_str().as_bytes());
        acc = acc.wrapping_add(g.cell_age(i) as u64);
    }
    acc
}

fn snapshots_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
}

fn assert_snapshot(name: &str, value: u64) {
    let dir = snapshots_dir();
    let path = dir.join(format!("{}.txt", name));
    let val_hex = format!("{:016x}", value);
    let update = std::env::var("CELLA_UPDATE_SNAPSHOTS")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if update {
        let _ = fs::create_dir_all(&dir);
        fs::write(&path, &val_hex).expect("write snapshot");
        println!("updated snapshot {} => {}", name, path.display());
        return;
    }
    match fs::read_to_string(&path) {
        Ok(s) => {
            let s = s.trim();
            assert_eq!(
                s,
                val_hex,
                "snapshot mismatch for {} ({}): expected {}, got {}",
                name,
                path.display(),
                s,
                val_hex
            );
        }
        Err(_) => {
            eprintln!(
                "snapshot missing for {} at {}. Set CELLA_UPDATE_SNAPSHOTS=1 to create it.",
                name,
                path.display()
            );
            panic!("missing snapshot: {}", name);
        }
    }
}

// -------- Benchmark storage and statistics --------
//
// Why this section is more than "run it 10 times and average":
// timing noise on a shared machine is almost always one-sided. Something else
// (another process, the WSL2 host, a frequency dip) can only make a run
// SLOWER than the true cost, never faster. So the plain mean is dragged up by
// the bad runs, and the minimum is the most stable estimate of true cost.
// Each bench therefore reports mean +/- std (kept so old history rows stay
// comparable), plus min and median, plus a list of outlier runs. Outliers are
// REPORTED, never dropped: they never change avg/std_dev/min/median.
//
// Protocol per bench: `CELLA_BENCH_WARMUP` untimed runs (default 1: the first
// run pays for cold caches, page faults and CPU frequency ramp-up), then
// `CELLA_BENCH_RUNS` timed runs (default 10).
static BENCH_DATA: OnceLock<Mutex<Vec<(String, u128)>>> = OnceLock::new();
/// 1-minute load average per bench, keyed by full name: the max of the samples
/// taken before its warm-up and after its last timed run.
static BENCH_LOADS: OnceLock<Mutex<HashMap<String, f64>>> = OnceLock::new();
/// Load average sampled once when the first bench of the process started.
static SUITE_START_LOAD: OnceLock<Option<f64>> = OnceLock::new();

/// A 1-minute load average above this means the machine was busy enough that
/// timings are suspect. The dev box has 16 logical CPUs; 4 is "a quarter busy".
const LOAD_WARN: f64 = 4.0;
/// A bench with more than this fraction of outlier runs is called untrustworthy.
const OUTLIER_WARN_FRACTION: f64 = 0.20;

fn bench_runs() -> usize {
    std::env::var("CELLA_BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10)
}

/// Untimed runs executed before the timed ones. Override: `CELLA_BENCH_WARMUP`.
fn bench_warmup() -> usize {
    std::env::var("CELLA_BENCH_WARMUP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}

fn bench_print_enabled() -> bool {
    std::env::var("CELLA_BENCH").ok().as_deref() == Some("1")
}

fn bench_store() -> &'static Mutex<Vec<(String, u128)>> {
    BENCH_DATA.get_or_init(|| Mutex::new(Vec::new()))
}

fn bench_loads() -> &'static Mutex<HashMap<String, f64>> {
    BENCH_LOADS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Parses the first field (the 1-minute average) of `/proc/loadavg` text.
fn parse_loadavg(text: &str) -> Option<f64> {
    text.split_whitespace().next()?.parse().ok()
}

/// Current 1-minute load average, or `None` where `/proc/loadavg` does not
/// exist (Windows, macOS). Never fails the bench; the guard just goes quiet.
fn read_load1() -> Option<f64> {
    parse_loadavg(&fs::read_to_string("/proc/loadavg").ok()?)
}

fn load_text(load: Option<f64>) -> String {
    match load {
        Some(l) if l > LOAD_WARN => format!("{:.2} (HIGH, > {})", l, LOAD_WARN),
        Some(l) => format!("{:.2}", l),
        None => "n/a".to_string(),
    }
}

/// Reads and prints the load once per process, the first time a bench runs.
fn suite_start_load() -> Option<f64> {
    *SUITE_START_LOAD.get_or_init(|| {
        let l = read_load1();
        println!("[bench] load average (1 min) at suite start: {}", load_text(l));
        if l.is_some_and(|v| v > LOAD_WARN) {
            println!(
                "[bench] WARNING: machine is busy (load > {}); timings below are NOT trustworthy. Wait for it to settle.",
                LOAD_WARN
            );
        }
        l
    })
}

fn record_bench(name: &str, nanos: u128) {
    let suffix = format!("_t{}", thread_count());
    let full = format!("{}{}", name, suffix);
    let mut v = bench_store().lock().unwrap();
    v.push((full.clone(), nanos));
    if bench_print_enabled() {
        println!(
            "[bench] {:>28}: {:.6} ms",
            full,
            nanos as f64 / 1_000_000f64
        );
    }
}

// -------- Robust statistics --------

/// Median of an already-sorted slice (0.0 when empty).
fn median_sorted(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    match n {
        0 => 0.0,
        _ if n % 2 == 1 => sorted[n / 2],
        _ => (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0,
    }
}

/// Everything the harness derives from one bench's timed runs (all in ms).
#[derive(Clone, Debug, PartialEq)]
struct RunSummary {
    avg: f64,
    std_dev: f64,
    min: f64,
    median: f64,
    /// Flagged runs, in the order they were measured.
    outliers: Vec<f64>,
}

/// Fewer runs than this and "outlier" has no meaning; nothing is flagged.
const MIN_RUNS_FOR_OUTLIERS: usize = 5;

/// Flags runs outside `median +/- max(3 * 1.4826 * MAD, 2 % of median)`.
///
/// MAD ("median absolute deviation") is the median of `|x - median|`: the
/// typical distance of a run from the middle run, but computed with medians so
/// a couple of wild runs cannot inflate it. Multiplying by 1.4826 makes it
/// comparable to a standard deviation for bell-curve data, so "3 x" reads like
/// "3 sigma". It was picked over the Tukey/IQR fence (quartiles +/- 1.5 IQR)
/// because with only ~10 runs the quartiles are interpolated between a handful
/// of points and a single slow run already shifts them; MAD tolerates up to
/// half the runs being bad. The 2 % floor stops a very quiet bench (MAD near
/// zero) from flagging a run that is 1 % off, which is real but harmless.
fn find_outliers(times: &[f64]) -> Vec<f64> {
    if times.len() < MIN_RUNS_FOR_OUTLIERS {
        return Vec::new();
    }
    let mut sorted = times.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = median_sorted(&sorted);
    let mut dev: Vec<f64> = times.iter().map(|t| (t - med).abs()).collect();
    dev.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mad = median_sorted(&dev);
    let half_width = (3.0 * 1.4826 * mad).max(0.02 * med);
    times
        .iter()
        .copied()
        .filter(|t| (t - med).abs() > half_width)
        .collect()
}

fn summarize_runs(times: &[f64]) -> RunSummary {
    let n = times.len() as f64;
    if times.is_empty() {
        return RunSummary {
            avg: 0.0,
            std_dev: 0.0,
            min: 0.0,
            median: 0.0,
            outliers: Vec::new(),
        };
    }
    // avg/std_dev are over ALL timed runs (population std), exactly as before
    // this harness grew min/median, so the baseline history stays comparable.
    let avg = times.iter().sum::<f64>() / n;
    let variance = if n > 1.0 {
        times.iter().map(|&t| (t - avg) * (t - avg)).sum::<f64>() / n
    } else {
        0.0
    };
    let mut sorted = times.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    RunSummary {
        avg,
        std_dev: variance.sqrt(),
        min: sorted[0],
        median: median_sorted(&sorted),
        outliers: find_outliers(times),
    }
}

fn format_outliers(vals: &[f64]) -> String {
    vals.iter()
        .map(|v| format!("{:.3}", v))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Shared timing loop for 1D and 2D grids: untimed warm-up runs, then timed
/// runs, then (with `CELLA_BENCH=1`) one robust-stats line. `check` runs once,
/// on the first execution of the scenario (warm-up if any), outside the timer:
/// it does the ASCII dump and the FNV snapshot assertion.
fn run_benchmark_loop<G: Clone>(
    name: &str,
    g_initial: &G,
    steps: usize,
    step: fn(&mut G),
    check: impl Fn(&G),
) {
    suite_start_load();
    // Sampled before the warm-up; combined with the end sample below (max).
    let load_before = read_load1();
    let mut checked = false;
    for _ in 0..bench_warmup() {
        let mut g = g_initial.clone();
        for _ in 0..steps {
            step(&mut g);
        }
        if !checked {
            check(&g);
            checked = true;
        }
    }
    let mut times_ms = Vec::new();
    for _ in 0..bench_runs() {
        let mut g = g_initial.clone();
        let t0 = Instant::now();
        for _ in 0..steps {
            step(&mut g);
        }
        let elapsed = t0.elapsed().as_nanos();
        record_bench(name, elapsed);
        times_ms.push(elapsed as f64 / 1_000_000f64);
        if !checked {
            check(&g);
            checked = true;
        }
    }
    let full = format!("{}_t{}", name, thread_count());
    // Recorded load is the MAX of the start (before warm-up) and end samples,
    // so a busy spell at either end is visible. Stored as `load1` in the JSON.
    let load = match (load_before, read_load1()) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    if let Some(l) = load {
        bench_loads().lock().unwrap().insert(full.clone(), l);
    }
    if bench_print_enabled() && !times_ms.is_empty() {
        let s = summarize_runs(&times_ms);
        println!(
            "[bench] {:>28}: min {:.6} ms, median {:.6} ms, mean {:.6} ms (+/-{:.6}), outliers {}{}, load {}",
            full,
            s.min,
            s.median,
            s.avg,
            s.std_dev,
            s.outliers.len(),
            if s.outliers.is_empty() {
                String::new()
            } else {
                format!(" [{}] ms", format_outliers(&s.outliers))
            },
            load_text(load)
        );
    }
}

fn run_benchmark_2d(name: &str, g_initial: &Grid2D, steps: usize) {
    run_benchmark_loop(name, g_initial, steps, Grid2D::step, |g| {
        if ascii_enabled() {
            print_ascii_2d(&format!("{}: final", name), g);
        }
        assert_snapshot(name, hash_grid2d_state(g));
    });
}

fn run_benchmark_1d(name: &str, g_initial: &Grid1D, steps: usize) {
    run_benchmark_loop(name, g_initial, steps, Grid1D::step, |g| {
        if ascii_enabled() {
            print_ascii_1d(&format!("{}: final", name), g);
        }
        assert_snapshot(name, hash_grid1d_state(g));
    });
}

// -------- Optional ASCII rendering helpers --------
fn ascii_enabled() -> bool {
    std::env::var("CELLA_ASCII")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn ascii_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("ascii")
}

// -------- Config export helpers --------
fn configs_export_enabled() -> bool {
    std::env::var("CELLA_EXPORT_CONFIGS")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn configs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(Path::new("."))
        .join("configs")
}

fn export_config_2d(name: &str, g: &Grid2D) {
    use cella_lib::config::{CellaConfig, Config2D};
    let total = g.width * g.height;
    let initial: Vec<String> = (0..total)
        .map(|i| g.cell_type(i).as_str().to_string())
        .collect();
    let cfg = CellaConfig::D2(Config2D {
        ensemble: None,
        evolve: None,
        seed: 0,
        colors: Default::default(),
        width: g.width,
        height: g.height,
        history_limit: g.history_limit,
        initial,
        rule: g.rule.clone(),
        model: g.model.clone(),
        snapshot: None,
    });
    let dir = configs_dir();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join(format!("{}.json", name));
    match cfg.to_file_pretty(&path) {
        Ok(_) => println!("exported config {} => {}", name, path.display()),
        Err(e) => eprintln!("failed to export config {}: {}", name, e),
    }
}

fn export_config_1d(name: &str, g: &Grid1D) {
    use cella_lib::config::{CellaConfig, Config1D};
    let initial: Vec<String> = (0..g.width)
        .map(|i| g.cell_type(i).as_str().to_string())
        .collect();
    let cfg = CellaConfig::D1(Config1D {
        colors: Default::default(),
        seed: 0,
        ensemble: None,
        evolve: None,
        width: g.width,
        history_limit: g.history_limit,
        initial,
        rule: g.rule.clone(),
        snapshot: None,
    });
    let dir = configs_dir();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join(format!("{}.json", name));
    match cfg.to_file_pretty(&path) {
        Ok(_) => println!("exported config {} => {}", name, path.display()),
        Err(e) => eprintln!("failed to export config {}: {}", name, e),
    }
}

fn ascii_base_and_truncate(label: &str) -> (String, bool) {
    if let Some(idx) = label.find(':') {
        let base = label[..idx].trim().to_string();
        let phase = label[idx + 1..].trim().to_ascii_lowercase();
        let truncate = phase.starts_with("initial");
        (base, truncate)
    } else {
        (label.trim().to_string(), false)
    }
}

fn ascii_open_for(label: &str) -> std::io::Result<std::fs::File> {
    let (base, truncate) = ascii_base_and_truncate(label);
    let dir = ascii_dir();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join(format!("{}.txt", base));
    let mut opts = fs::OpenOptions::new();
    opts.create(true).write(true);
    if truncate {
        opts.truncate(true);
    } else {
        opts.append(true);
    }
    opts.open(path)
}

fn ascii_symbols_map(names: &mut Vec<String>) -> std::collections::BTreeMap<String, char> {
    names.sort();
    names.dedup();
    let symbol_pool: Vec<char> = "!@#$%^&*()abcdefghijklmnopqrstuvwxyz".chars().collect();
    let mut map: std::collections::BTreeMap<String, char> = std::collections::BTreeMap::new();
    for (i, n) in names.iter().enumerate() {
        let ch = *symbol_pool.get(i).unwrap_or(&'?');
        map.insert(n.clone(), ch);
    }
    map
}

fn print_ascii_1d(label: &str, g: &Grid1D) {
    use std::collections::BTreeMap;
    let mut names: Vec<String> = (0..g.width)
        .map(|i| g.cell_type(i).as_str().to_string())
        .filter(|n| n != INACTIVE)
        .collect();
    let map: BTreeMap<String, char> = ascii_symbols_map(&mut names);
    let mut line = String::with_capacity(g.width);
    for i in 0..g.width {
        let ty = g.cell_type(i).as_str();
        if ty == INACTIVE {
            line.push('.');
        } else {
            line.push(*map.get(&ty.to_string()).unwrap_or(&'?'));
        }
    }
    if let Ok(mut f) = ascii_open_for(label) {
        let _ = writeln!(f, "[ascii] {} (1D w={})", label, g.width);
        let _ = writeln!(f, "{}", line);
        let _ = writeln!(f);
    }
}

fn print_ascii_2d(label: &str, g: &Grid2D) {
    use std::collections::BTreeMap;
    let total = g.width * g.height;
    let mut names: Vec<String> = (0..total)
        .map(|i| g.cell_type(i).as_str().to_string())
        .filter(|n| n != INACTIVE)
        .collect();
    let map: BTreeMap<String, char> = ascii_symbols_map(&mut names);
    if let Ok(mut f) = ascii_open_for(label) {
        let _ = writeln!(f, "[ascii] {} (2D {}x{})", label, g.width, g.height);
        for y in 0..g.height {
            let mut line = String::with_capacity(g.width);
            for x in 0..g.width {
                let i = y * g.width + x;
                let ty = g.cell_type(i).as_str();
                if ty == INACTIVE {
                    line.push('.');
                } else {
                    line.push(*map.get(&ty.to_string()).unwrap_or(&'?'));
                }
            }
            let _ = writeln!(f, "{}", line);
        }
        let _ = writeln!(f);
    }
}

fn stress_2d_life_like_moore() {
    let alive = CellType::from("Alive");
    let inactive = CellType::inactive();
    let rule = Rule2D {
        subrules: vec![
            // Overpopulation: Alive with >=4 neighbors becomes Inactive
            Rule2DSubrule::new(
                alive.clone(),
                alive.clone(),
                4,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                inactive.clone(),
                None,
                None,
            ),
            // Survival: Alive stays Alive with >=2 neighbors
            Rule2DSubrule::new(
                alive.clone(),
                alive.clone(),
                2,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                alive.clone(),
                None,
                None,
            ),
            // Birth: Inactive becomes Alive with ==3 neighbors
            Rule2DSubrule::new(
                inactive.clone(),
                alive.clone(),
                3,
                CountOp::Eq,
                1,
                Neighborhood2D::Moore,
                alive.clone(),
                None,
                None,
            ),
        ],
    };
    let (w, h, hist) = (50usize, 30usize, 5usize);
    let mut init = vec![CellType::inactive(); w * h];
    // seed: glider-like shape
    let mut set = |x: usize, y: usize| init[y * w + x] = alive.clone();
    set(1, 0);
    set(2, 1);
    set(0, 2);
    set(1, 2);
    set(2, 2);
    let g = Grid2D::new(w, h, hist, init, rule);
    if ascii_enabled() {
        print_ascii_2d("2d_life_like_moore: initial", &g);
    }
    if configs_export_enabled() {
        export_config_2d("2d_life_like_moore", &g);
    }
    run_benchmark_2d("2d_life_like_moore", &g, 300);
}

fn stress_2d_von_neumann_threshold() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    // TODO need to make this rule more interesting
    let rule = Rule2D {
        subrules: vec![
            Rule2DSubrule::new(
                a.clone(),
                b.clone(),
                2,
                CountOp::Gt,
                2,
                Neighborhood2D::VonNeumann,
                b.clone(),
                None,
                None,
            ),
            Rule2DSubrule::new(
                b.clone(),
                b.clone(),
                1,
                CountOp::Gt,
                1,
                Neighborhood2D::VonNeumann,
                b.clone(),
                None,
                None,
            ),
        ],
    };
    let (w, h, hist) = (64usize, 32usize, 3usize);
    let mut init = vec![a.clone(); w * h];
    // random-ish seed (deterministic pattern)
    for y in 0..h {
        for x in 0..w {
            if (x ^ y) % 7 == 0 {
                init[y * w + x] = b.clone();
            }
        }
    }
    let g = Grid2D::new(w, h, hist, init, rule);
    if ascii_enabled() {
        print_ascii_2d("2d_vonneumann_threshold: initial", &g);
    }
    if configs_export_enabled() {
        export_config_2d("2d_vonneumann_threshold", &g);
    }
    run_benchmark_2d("2d_vonneumann_threshold", &g, 200);
}

fn stress_2d_straightline_threshold() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    // TODO make rule more interesting
    let rule = Rule2D {
        subrules: vec![
            Rule2DSubrule::new(
                a.clone(),
                b.clone(),
                2,
                CountOp::Gt,
                3,
                Neighborhood2D::StraightLine,
                b.clone(),
                None,
                None,
            ),
            Rule2DSubrule::new(
                b.clone(),
                b.clone(),
                1,
                CountOp::Gt,
                1,
                Neighborhood2D::StraightLine,
                b.clone(),
                None,
                None,
            ),
        ],
    };
    let (w, h, hist) = (64usize, 32usize, 3usize);
    let mut init = vec![a.clone(); w * h];
    for y in 0..h {
        for x in 0..w {
            if (x * 13 + y * 7) % 17 == 0 {
                init[y * w + x] = b.clone();
            }
        }
    }
    let g = Grid2D::new(w, h, hist, init, rule);
    if ascii_enabled() {
        print_ascii_2d("2d_straightline_threshold: initial", &g);
    }
    if configs_export_enabled() {
        export_config_2d("2d_straightline_threshold", &g);
    }
    run_benchmark_2d("2d_straightline_threshold", &g, 200);
}

fn stress_2d_langton_diagonals() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    // TODO need to make this rule more interesting
    let rule = Rule2D {
        subrules: vec![Rule2DSubrule::new(
            a.clone(),
            b.clone(),
            3,
            CountOp::Gt,
            2,
            Neighborhood2D::Langton,
            b.clone(),
            None,
            None,
        )],
    };
    let (w, h, hist) = (48usize, 48usize, 2usize);
    let mut init = vec![a.clone(); w * h];
    for i in 0..w.min(h) {
        init[i * w + i] = b.clone();
    }
    let g = Grid2D::new(w, h, hist, init, rule);
    if ascii_enabled() {
        print_ascii_2d("2d_langton_diagonals: initial", &g);
    }
    if configs_export_enabled() {
        export_config_2d("2d_langton_diagonals", &g);
    }
    run_benchmark_2d("2d_langton_diagonals", &g, 180);
}

fn stress_2d_knight_neighborhood() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    // Conway-style birth/survival using Knight neighborhood (range=1 = 8 classic L-move squares)
    let rule = Rule2D {
        subrules: vec![
            // Overpopulation: A with >4 B knight-neighbors becomes B
            Rule2DSubrule::new(
                a.clone(),
                b.clone(),
                4,
                CountOp::Gt,
                1,
                Neighborhood2D::Knight,
                b.clone(),
                None,
                None,
            ),
            // Survival: A with 2..=4 B knight-neighbors stays A
            Rule2DSubrule::new(
                a.clone(),
                b.clone(),
                2,
                CountOp::Gt,
                1,
                Neighborhood2D::Knight,
                a.clone(),
                None,
                Some(4),
            ),
            // Birth: B with ==3 A knight-neighbors becomes A
            Rule2DSubrule::new(
                b.clone(),
                a.clone(),
                3,
                CountOp::Eq,
                1,
                Neighborhood2D::Knight,
                a.clone(),
                None,
                None,
            ),
        ],
    };
    let (w, h, hist) = (64usize, 48usize, 3usize);
    let mut init = vec![b.clone(); w * h];
    // deterministic seed: scatter A cells in a structured pattern
    for y in 0..h {
        for x in 0..w {
            if (x * 7 + y * 11) % 13 == 0 {
                init[y * w + x] = a.clone();
            }
        }
    }
    let g = Grid2D::new(w, h, hist, init, rule);
    if ascii_enabled() {
        print_ascii_2d("2d_knight_neighborhood: initial", &g);
    }
    if configs_export_enabled() {
        export_config_2d("2d_knight_neighborhood", &g);
    }
    run_benchmark_2d("2d_knight_neighborhood", &g, 200);
}

fn stress_1d_rule30_center_seed() {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    let sub_active = Rule1DSubrule {
        current_type: x.clone(),
        criteria_type: x.clone(),
        wolfram_code: 30,
        n: 1,
        randomness: None,
        output_type: x.clone(),
    };
    let sub_inactive = Rule1DSubrule {
        current_type: inactive.clone(),
        criteria_type: x.clone(),
        wolfram_code: 30,
        n: 1,
        randomness: None,
        output_type: x.clone(),
    };
    let rule = Rule1D {
        subrules: vec![sub_active, sub_inactive],
    };
    let w = 257usize;
    let hist = 4usize;
    let mut init = vec![CellType::inactive(); w];
    init[w / 2] = x.clone();
    let g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() {
        print_ascii_1d("1d_rule30_center: initial", &g);
    }
    if configs_export_enabled() {
        export_config_1d("1d_rule30_center", &g);
    }
    run_benchmark_1d("1d_rule30_center", &g, 500);
}

fn stress_1d_n2_alternating_code() {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    let code: u128 = 0xAAAAAAAA; // alternating bits over first 32 patterns
    // TODO make rule more interesting
    let sub_active = Rule1DSubrule {
        current_type: x.clone(),
        criteria_type: x.clone(),
        wolfram_code: code,
        n: 2,
        randomness: None,
        output_type: x.clone(),
    };
    let sub_inactive = Rule1DSubrule {
        current_type: inactive.clone(),
        criteria_type: x.clone(),
        wolfram_code: code,
        n: 2,
        randomness: None,
        output_type: x.clone(),
    };
    let rule = Rule1D {
        subrules: vec![sub_active, sub_inactive],
    };
    let w = 301usize;
    let hist = 3usize;
    let mut init = vec![inactive.clone(); w];
    init[w / 2] = x.clone();
    let g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() {
        print_ascii_1d("1d_n2_alt: initial", &g);
    }
    if configs_export_enabled() {
        export_config_1d("1d_n2_alt", &g);
    }
    run_benchmark_1d("1d_n2_alt", &g, 400);
}

fn stress_1d_n3_custom_code() {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    // n=3 -> 2^(2*3+1)=2^7=128 patterns; pick a code with some structure
    let code: u128 = 0xF0F0_F0F0_F0F0_F0F0;
    // TODO make rule more interesting
    let sub_active = Rule1DSubrule {
        current_type: x.clone(),
        criteria_type: x.clone(),
        wolfram_code: code,
        n: 3,
        randomness: None,
        output_type: x.clone(),
    };
    let sub_inactive = Rule1DSubrule {
        current_type: inactive.clone(),
        criteria_type: x.clone(),
        wolfram_code: code,
        n: 3,
        randomness: None,
        output_type: x.clone(),
    };
    let rule = Rule1D {
        subrules: vec![sub_active, sub_inactive],
    };
    let w = 257usize;
    let hist = 2usize;
    let mut init = vec![inactive.clone(); w];
    init[w / 2] = x.clone();
    let g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() {
        print_ascii_1d("1d_n3_custom: initial", &g);
    }
    if configs_export_enabled() {
        export_config_1d("1d_n3_custom", &g);
    }
    run_benchmark_1d("1d_n3_custom", &g, 350);
}

// -------- Larger stress tests to exercise multithreading --------

fn stress_1d_three_state_cycle() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    let any = 0xFFu128;
    let rule = Rule1D {
        subrules: vec![
            Rule1DSubrule {
                current_type: a.clone(),
                criteria_type: a.clone(),
                wolfram_code: any,
                n: 1,
                randomness: None,
                output_type: b.clone(),
            },
            Rule1DSubrule {
                current_type: b.clone(),
                criteria_type: b.clone(),
                wolfram_code: any,
                n: 1,
                randomness: None,
                output_type: c.clone(),
            },
            Rule1DSubrule {
                current_type: c.clone(),
                criteria_type: c.clone(),
                wolfram_code: any,
                n: 1,
                randomness: None,
                output_type: a.clone(),
            },
        ],
    };
    let w = 1024usize;
    let hist = 3usize;
    let init = (0..w)
        .map(|i| match i % 3 {
            0 => a.clone(),
            1 => b.clone(),
            _ => c.clone(),
        })
        .collect::<Vec<_>>();
    let g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() {
        print_ascii_1d("1d_three_state_cycle: initial", &g);
    }
    if configs_export_enabled() {
        export_config_1d("1d_three_state_cycle", &g);
    }
    run_benchmark_1d("1d_three_state_cycle", &g, 800);
}

fn stress_2d_three_state_cycle() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    let rule = Rule2D {
        subrules: vec![
            Rule2DSubrule::new(
                a.clone(),
                b.clone(),
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                b.clone(),
                None,
                None,
            ),
            Rule2DSubrule::new(
                b.clone(),
                c.clone(),
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                c.clone(),
                None,
                None,
            ),
            Rule2DSubrule::new(
                c.clone(),
                a.clone(),
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                a.clone(),
                None,
                None,
            ),
        ],
    };
    let (w, h, hist) = (192usize, 128usize, 3usize);
    let mut init = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let idx = (x + y) % 3;
            init.push(match idx {
                0 => a.clone(),
                1 => b.clone(),
                _ => c.clone(),
            });
        }
    }
    let g = Grid2D::new(w, h, hist, init, rule);
    if ascii_enabled() {
        print_ascii_2d("2d_three_state_cycle: initial", &g);
    }
    if configs_export_enabled() {
        export_config_2d("2d_three_state_cycle", &g);
    }
    run_benchmark_2d("2d_three_state_cycle", &g, 240);
}

// -------- Larger stress tests to exercise multithreading --------

fn stress_2d_large_moore_256() {
    let alive = CellType::from("Alive");
    let inactive = CellType::inactive();
    let rule = Rule2D {
        subrules: vec![
            // Overpopulation: Alive with >=4 neighbors becomes Inactive
            Rule2DSubrule::new(
                alive.clone(),
                alive.clone(),
                4,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                inactive.clone(),
                None,
                None,
            ),
            // Survival: Alive stays Alive with >=2 neighbors
            Rule2DSubrule::new(
                alive.clone(),
                alive.clone(),
                2,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                alive.clone(),
                None,
                None,
            ),
            // Birth: Inactive becomes Alive with ==3 neighbors
            Rule2DSubrule::new(
                inactive.clone(),
                alive.clone(),
                3,
                CountOp::Eq,
                1,
                Neighborhood2D::Moore,
                alive.clone(),
                None,
                None,
            ),
        ],
    };
    let (w, h, hist) = (256usize, 256usize, 4usize);
    let mut init = vec![inactive.clone(); w * h];
    // Seed a few glider-like patterns along the diagonal
    for k in (0..w.min(h)).step_by(32) {
        let set = |x: usize, y: usize, v: &mut Vec<CellType>| v[y * w + x] = alive.clone();
        if k + 2 < w && k + 2 < h {
            set(k + 1, k + 0, &mut init);
            set(k + 2, k + 1, &mut init);
            set(k + 0, k + 2, &mut init);
            set(k + 1, k + 2, &mut init);
            set(k + 2, k + 2, &mut init);
        }
    }
    let g = Grid2D::new(w, h, hist, init, rule);
    if ascii_enabled() {
        print_ascii_2d("2d_large_moore_256: initial", &g);
    }
    if configs_export_enabled() {
        export_config_2d("2d_large_moore_256", &g);
    }
    run_benchmark_2d("2d_large_moore_256", &g, 200);
}

fn stress_2d_large_vn_256() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    // TODO make rule more interesting
    let rule = Rule2D {
        subrules: vec![
            Rule2DSubrule::new(
                a.clone(),
                b.clone(),
                2,
                CountOp::Gt,
                2,
                Neighborhood2D::VonNeumann,
                b.clone(),
                None,
                None,
            ),
            Rule2DSubrule::new(
                b.clone(),
                b.clone(),
                1,
                CountOp::Gt,
                1,
                Neighborhood2D::VonNeumann,
                b.clone(),
                None,
                None,
            ),
        ],
    };
    let (w, h, hist) = (256usize, 256usize, 3usize);
    let mut init = vec![a.clone(); w * h];
    for y in 0..h {
        for x in 0..w {
            if (x * 3 + y * 5) % 11 == 0 {
                init[y * w + x] = b.clone();
            }
        }
    }
    let g = Grid2D::new(w, h, hist, init, rule);
    if ascii_enabled() {
        print_ascii_2d("2d_large_vn_256: initial", &g);
    }
    if configs_export_enabled() {
        export_config_2d("2d_large_vn_256", &g);
    }
    run_benchmark_2d("2d_large_vn_256", &g, 160);
}

fn stress_1d_large_rule30_2049() {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    let sub_active = Rule1DSubrule {
        current_type: x.clone(),
        criteria_type: x.clone(),
        wolfram_code: 30,
        n: 1,
        randomness: None,
        output_type: x.clone(),
    };
    let sub_inactive = Rule1DSubrule {
        current_type: inactive.clone(),
        criteria_type: x.clone(),
        wolfram_code: 30,
        n: 1,
        randomness: None,
        output_type: x.clone(),
    };
    let rule = Rule1D {
        subrules: vec![sub_active, sub_inactive],
    };
    let w = 2049usize;
    let hist = 4usize;
    let mut init = vec![inactive.clone(); w];
    init[w / 2] = x.clone();
    let g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() {
        print_ascii_1d("1d_large_rule30_2049: initial", &g);
    }
    if configs_export_enabled() {
        export_config_1d("1d_large_rule30_2049", &g);
    }
    run_benchmark_1d("1d_large_rule30_2049", &g, 1200);
}

// Final summary printer (likely last if run with --test-threads=1).
//
// Per bench it prints mean (+/- std), min and median, then the change against
// the stored baseline on BOTH min and mean:
// - `Δmin` is the number to trust. Noise only ever slows a run down, so the
//   fastest run is the best estimate of the true cost (the spike that rejected
//   criterion measured 0.4-2.5 % spread on min-of-10 versus 5-8 % on the mean).
// - `Δavg` is kept because every baseline row ever recorded has an avg, and
//   the history table in docs/performance.md is built from it. Baseline
//   entries written before min/median existed print `Δmin n/a` until the
//   next CELLA_UPDATE_BENCH=1 refresh.
// After the table it prints a load-average report and an outlier report so
// that suspect results are impossible to miss. Nothing is dropped: outliers
// stay inside avg/std_dev; they are only listed.
#[test]
#[ignore]
fn zzz_benchmark_summary() {
    let data = bench_store().lock().unwrap();
    if data.is_empty() {
        println!("[bench] No benchmarks recorded. Did you run with --ignored?");
        return;
    }
    // Group by name (values in ms, in measurement order)
    let mut groups: HashMap<String, Vec<f64>> = HashMap::new();
    for (name, ns) in data.iter() {
        groups
            .entry(name.clone())
            .or_default()
            .push(*ns as f64 / 1_000_000f64); // convert to ms
    }

    // Load previous results if any
    let prev = load_previous_benchmarks();
    let loads = bench_loads().lock().unwrap().clone();

    println!(
        "\n[bench] Summary ({} runs for {} tests):",
        data.len(),
        groups.len()
    );
    let pct = |new: f64, old: f64| (new - old) * 100.0 / old;
    let mut total_avg: f64 = 0.0;
    let mut current_stats: HashMap<String, BenchStats> = HashMap::new();
    // (name, outlier values, total runs)
    let mut outlier_report: Vec<(String, Vec<f64>, usize)> = Vec::new();
    let mut busy: Vec<(String, f64)> = Vec::new();

    let mut names: Vec<_> = groups.keys().cloned().collect();
    names.sort();

    for name in names {
        let times = &groups[&name];
        let s = summarize_runs(times);
        let load1 = loads.get(&name).copied();
        current_stats.insert(
            name.clone(),
            BenchStats {
                avg: s.avg,
                std_dev: s.std_dev,
                min: Some(s.min),
                median: Some(s.median),
                outliers: s.outliers.len(),
                load1,
            },
        );
        total_avg += s.avg;
        if !s.outliers.is_empty() {
            outlier_report.push((name.clone(), s.outliers.clone(), times.len()));
        }
        if let Some(l) = load1.filter(|&l| l > LOAD_WARN) {
            busy.push((name.clone(), l));
        }

        let delta = match prev.get(&name) {
            None => "(new)".to_string(),
            Some(old) => {
                let d_avg = if old.avg > 0.0 {
                    format!("{:+7.2}%", pct(s.avg, old.avg))
                } else {
                    "    n/a".to_string()
                };
                let d_min = match old.min {
                    Some(m) if m > 0.0 => format!("{:+7.2}%", pct(s.min, m)),
                    _ => "    n/a".to_string(),
                };
                format!("Δmin {} Δavg {}", d_min, d_avg)
            }
        };
        println!(
            "[bench] {:>28}: avg {:>10.4} (±{:>8.4}) min {:>10.4} med {:>10.4} ms | {} | out {}",
            name,
            s.avg,
            s.std_dev,
            s.min,
            s.median,
            delta,
            s.outliers.len()
        );
    }
    println!(
        "[bench] {:>28}: {:>12.6} ms (sum of averages)",
        "TOTAL", total_avg
    );

    // ---- Load report ----
    let start_load = SUITE_START_LOAD.get().copied().flatten();
    println!("\n[bench] Load report (1-min load average; this box has 16 logical CPUs):");
    println!("[bench]   at suite start: {}", load_text(start_load));
    if loads.is_empty() {
        println!("[bench]   per bench: n/a (no /proc/loadavg on this platform)");
    } else if busy.is_empty() {
        println!("[bench]   no bench finished with load > {}.", LOAD_WARN);
    } else {
        println!(
            "[bench]   WARNING: {} bench(es) finished with load > {}; treat their numbers as suspect:",
            busy.len(),
            LOAD_WARN
        );
        for (n, l) in &busy {
            println!("[bench]     {:>28}: load {:.2}", n, l);
        }
    }

    // ---- Outlier report ----
    println!("\n[bench] Outlier report (runs outside median ± max(3·1.4826·MAD, 2 % of median)):");
    if outlier_report.is_empty() {
        println!("[bench]   no outliers in any bench.");
    } else {
        println!(
            "[bench]   {} bench(es) had outliers (they are INCLUDED in avg/std, never dropped):",
            outlier_report.len()
        );
        for (n, vals, total) in &outlier_report {
            let frac = vals.len() as f64 / *total as f64;
            println!(
                "[bench]     {:>28}: {}/{} runs [{}] ms{}",
                n,
                vals.len(),
                total,
                format_outliers(vals),
                if frac > OUTLIER_WARN_FRACTION {
                    "  <-- more than 20 % outliers: result UNTRUSTWORTHY, re-run on a quiet machine"
                } else {
                    ""
                }
            );
        }
    }

    let update = std::env::var("CELLA_UPDATE_BENCH")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if update {
        if let Err(e) = save_current_benchmarks(&current_stats) {
            eprintln!("[bench] Failed to save benchmarks: {}", e);
        } else {
            println!(
                "[bench] Saved current timings to {}",
                benchmarks_file().display()
            );
        }
    } else {
        println!(
            "[bench] Skipping save of benchmark baselines (set CELLA_UPDATE_BENCH=1 to update {}).",
            benchmarks_file().display()
        );
    }
}

// -------- Wildfire scenarios (external model; counter-based RNG makes the
// stochastic runs snapshot-stable and thread-count-independent) --------

/// 256x256 mixed-fuel landscape with an elevation ramp, moderate wind, and a
/// centre ignition. `spotting` adds firebrand events on top. Uses the default
/// `"bernoulli"` spread rule.
fn wildfire_grid_256(spotting: bool) -> Grid2D {
    wildfire_grid_256_with_spread(spotting, "bernoulli")
}

/// Same landscape as [`wildfire_grid_256`], but the caller picks the spread
/// rule (`"bernoulli"` or `"arrival"`). Keeping one builder means the arrival
/// scenarios are guaranteed to use exactly the same fuels, wind, slope and
/// ignition as the Bernoulli ones, so their timings are comparable.
fn wildfire_grid_256_with_spread(spotting: bool, spread: &str) -> Grid2D {
    use cella_lib::wildfire::{
        FuelClass, SpottingParams, WildfireEnv, WildfireModel, WildfireParams, cell_rand,
    };
    let (w, h) = (256usize, 256usize);
    let forest = CellType::from("Forest");
    let shrub = CellType::from("Shrub");
    let mut init = vec![forest; w * h];
    for (idx, cell) in init.iter_mut().enumerate() {
        // Deterministic fuel mosaic with unburnable water/rock patches.
        match cell_rand(777, 0, idx as u64, 9) {
            v if v < 0.25 => *cell = shrub,
            v if v < 0.30 => *cell = CellType::inactive(),
            _ => {}
        }
    }
    init[(h / 2) * w + w / 2] = CellType::from("Burning");
    let mut elevation = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            elevation[y * w + x] = x as f32 * 1.5 + (y as f32 * 0.4);
        }
    }
    let params = WildfireParams {
        seed: 20260814,
        p0: 0.58,
        fuels: vec![
            FuelClass {
                name: "Forest".into(),
                veg_factor: 1.0,
            },
            FuelClass {
                name: "Shrub".into(),
                veg_factor: 0.6,
            },
        ],
        wind_speed: 8.0,
        wind_from_deg: 315.0, // blows toward 45° on the grid, as the snapshot was taken
        c1: 0.045,
        c2: 0.131,
        slope_a: 0.078,
        cell_size: 30.0,
        burn_duration: 3,
        spotting: spotting.then(|| SpottingParams {
            p_spot: 0.02,
            median_distance: 8.0,
            sigma: 0.4,
            angle_jitter_deg: 25.0,
        }),
        burning_name: None,
        burned_name: None,
        spread: spread.into(),
        arrival_jitter: 0.2,
        wind_law: "exponential".into(),
    };
    let mut g = Grid2D::new(w, h, 0, init, Rule2D { subrules: vec![] });
    g.attach_model(Box::new(WildfireModel::new(
        params,
        WildfireEnv {
            density: vec![],
            elevation,
            ..Default::default()
        },
    )))
    .expect("wildfire scenario attaches");
    g
}

/// A stochastic 1D rule (Rule 30 with a 20 % chance of skipping each
/// subrule). Randomness draws are `cell_rand(seed, step, cell, stream)`, so
/// this is snapshot-testable and must hash the same on 1, 4 and 8 threads.
fn stress_1d_randomness_512() {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    let sub_active = Rule1DSubrule {
        current_type: x,
        criteria_type: x,
        wolfram_code: 30,
        n: 1,
        randomness: Some(0.2),
        output_type: x,
    };
    let sub_inactive = Rule1DSubrule {
        current_type: inactive,
        criteria_type: x,
        wolfram_code: 30,
        n: 1,
        randomness: Some(0.2),
        output_type: x,
    };
    let rule = Rule1D {
        subrules: vec![sub_active, sub_inactive],
    };
    let w = 512usize;
    let mut init = vec![inactive; w];
    init[w / 2] = x;
    let g = Grid1D::new(w, 2, init, rule).with_seed(7);
    if ascii_enabled() {
        print_ascii_1d("1d_randomness_512: initial", &g);
    }
    if configs_export_enabled() {
        export_config_1d("1d_randomness_512", &g);
    }
    run_benchmark_1d("1d_randomness_512", &g, 500);
}

/// Life with a 5 % chance that a birth is skipped, on a seeded random soup.
/// Same determinism contract as the 1D case above.
fn stress_2d_randomness_128() {
    let alive = CellType::from("Alive");
    let inactive = CellType::inactive();
    let die_crowded = Rule2DSubrule::new(
        alive,
        alive,
        4,
        CountOp::Gt,
        1,
        Neighborhood2D::Moore,
        inactive,
        None,
        None,
    );
    let survive = Rule2DSubrule::new(
        alive,
        alive,
        2,
        CountOp::Gt,
        1,
        Neighborhood2D::Moore,
        alive,
        None,
        None,
    );
    let birth = Rule2DSubrule::new(
        inactive,
        alive,
        3,
        CountOp::Eq,
        1,
        Neighborhood2D::Moore,
        alive,
        Some(0.05),
        None,
    );
    let rule = Rule2D {
        subrules: vec![die_crowded, survive, birth],
    };
    let (w, h) = (128usize, 128usize);
    let init: Vec<CellType> = (0..w * h)
        .map(|idx| {
            if cella_lib::rng::cell_rand(4242, 0, idx as u64, 11) < 0.3 {
                alive
            } else {
                inactive
            }
        })
        .collect();
    let g = Grid2D::new(w, h, 1, init, rule).with_seed(99);
    if ascii_enabled() {
        print_ascii_2d("2d_randomness_128: initial", &g);
    }
    if configs_export_enabled() {
        export_config_2d("2d_randomness_128", &g);
    }
    run_benchmark_2d("2d_randomness_128", &g, 200);
}

fn stress_2d_wildfire() {
    let g = wildfire_grid_256(false);
    if ascii_enabled() {
        print_ascii_2d("2d_wildfire_256: initial", &g);
    }
    if configs_export_enabled() {
        export_config_2d("2d_wildfire_256", &g);
    }
    run_benchmark_2d("2d_wildfire_256", &g, 200);
}

fn stress_2d_wildfire_spotting() {
    let g = wildfire_grid_256(true);
    if ascii_enabled() {
        print_ascii_2d("2d_wildfire_spotting_256: initial", &g);
    }
    if configs_export_enabled() {
        export_config_2d("2d_wildfire_spotting_256", &g);
    }
    run_benchmark_2d("2d_wildfire_spotting_256", &g, 200);
}

/// Arrival-spread wildfire on the same 256x256 landscape. The arrival rule
/// visits every cell every step (about 20x the cost of the Bernoulli rule), so
/// these scenarios run only 30 steps: that keeps one run near 200 ms at one
/// thread instead of the ~1.2 s that 200 steps would take.
const ARRIVAL_STEPS: usize = 30;

fn stress_2d_wildfire_arrival() {
    let g = wildfire_grid_256_with_spread(false, "arrival");
    if ascii_enabled() {
        print_ascii_2d("2d_wildfire_arrival_256: initial", &g);
    }
    if configs_export_enabled() {
        export_config_2d("2d_wildfire_arrival_256", &g);
    }
    run_benchmark_2d("2d_wildfire_arrival_256", &g, ARRIVAL_STEPS);
}

fn stress_2d_wildfire_arrival_spotting() {
    let g = wildfire_grid_256_with_spread(true, "arrival");
    if ascii_enabled() {
        print_ascii_2d("2d_wildfire_arrival_spotting_256: initial", &g);
    }
    if configs_export_enabled() {
        export_config_2d("2d_wildfire_arrival_spotting_256", &g);
    }
    run_benchmark_2d("2d_wildfire_arrival_spotting_256", &g, ARRIVAL_STEPS);
}

#[test]
#[ignore]
fn stress_1d_randomness_512_t1() {
    set_thread_override(1);
    stress_1d_randomness_512();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_1d_randomness_512_t4() {
    set_thread_override(4);
    stress_1d_randomness_512();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_1d_randomness_512_t8() {
    set_thread_override(8);
    stress_1d_randomness_512();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_2d_randomness_128_t1() {
    set_thread_override(1);
    stress_2d_randomness_128();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_randomness_128_t4() {
    set_thread_override(4);
    stress_2d_randomness_128();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_randomness_128_t8() {
    set_thread_override(8);
    stress_2d_randomness_128();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_2d_wildfire_t1() {
    set_thread_override(1);
    stress_2d_wildfire();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_wildfire_t4() {
    set_thread_override(4);
    stress_2d_wildfire();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_wildfire_t8() {
    set_thread_override(8);
    stress_2d_wildfire();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_2d_wildfire_spotting_t1() {
    set_thread_override(1);
    stress_2d_wildfire_spotting();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_wildfire_spotting_t4() {
    set_thread_override(4);
    stress_2d_wildfire_spotting();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_wildfire_spotting_t8() {
    set_thread_override(8);
    stress_2d_wildfire_spotting();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_2d_wildfire_arrival_t1() {
    set_thread_override(1);
    stress_2d_wildfire_arrival();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_wildfire_arrival_t4() {
    set_thread_override(4);
    stress_2d_wildfire_arrival();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_wildfire_arrival_t8() {
    set_thread_override(8);
    stress_2d_wildfire_arrival();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_2d_wildfire_arrival_spotting_t1() {
    set_thread_override(1);
    stress_2d_wildfire_arrival_spotting();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_wildfire_arrival_spotting_t4() {
    set_thread_override(4);
    stress_2d_wildfire_arrival_spotting();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_wildfire_arrival_spotting_t8() {
    set_thread_override(8);
    stress_2d_wildfire_arrival_spotting();
    clear_thread_override();
}

// -------- Thread-count variants (1,4,8) for all long tests --------
#[test]
#[ignore]
fn stress_2d_life_like_moore_t1() {
    set_thread_override(1);
    stress_2d_life_like_moore();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_life_like_moore_t4() {
    set_thread_override(4);
    stress_2d_life_like_moore();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_life_like_moore_t8() {
    set_thread_override(8);
    stress_2d_life_like_moore();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_2d_von_neumann_threshold_t1() {
    set_thread_override(1);
    stress_2d_von_neumann_threshold();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_von_neumann_threshold_t4() {
    set_thread_override(4);
    stress_2d_von_neumann_threshold();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_von_neumann_threshold_t8() {
    set_thread_override(8);
    stress_2d_von_neumann_threshold();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_2d_straightline_threshold_t1() {
    set_thread_override(1);
    stress_2d_straightline_threshold();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_straightline_threshold_t4() {
    set_thread_override(4);
    stress_2d_straightline_threshold();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_straightline_threshold_t8() {
    set_thread_override(8);
    stress_2d_straightline_threshold();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_2d_langton_diagonals_t1() {
    set_thread_override(1);
    stress_2d_langton_diagonals();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_langton_diagonals_t4() {
    set_thread_override(4);
    stress_2d_langton_diagonals();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_langton_diagonals_t8() {
    set_thread_override(8);
    stress_2d_langton_diagonals();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_2d_knight_neighborhood_t1() {
    set_thread_override(1);
    stress_2d_knight_neighborhood();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_knight_neighborhood_t4() {
    set_thread_override(4);
    stress_2d_knight_neighborhood();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_knight_neighborhood_t8() {
    set_thread_override(8);
    stress_2d_knight_neighborhood();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_1d_rule30_center_seed_t1() {
    set_thread_override(1);
    stress_1d_rule30_center_seed();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_1d_rule30_center_seed_t4() {
    set_thread_override(4);
    stress_1d_rule30_center_seed();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_1d_rule30_center_seed_t8() {
    set_thread_override(8);
    stress_1d_rule30_center_seed();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_1d_n2_alternating_code_t1() {
    set_thread_override(1);
    stress_1d_n2_alternating_code();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_1d_n2_alternating_code_t4() {
    set_thread_override(4);
    stress_1d_n2_alternating_code();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_1d_n2_alternating_code_t8() {
    set_thread_override(8);
    stress_1d_n2_alternating_code();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_1d_n3_custom_code_t1() {
    set_thread_override(1);
    stress_1d_n3_custom_code();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_1d_n3_custom_code_t4() {
    set_thread_override(4);
    stress_1d_n3_custom_code();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_1d_n3_custom_code_t8() {
    set_thread_override(8);
    stress_1d_n3_custom_code();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_2d_large_moore_256_t1() {
    set_thread_override(1);
    stress_2d_large_moore_256();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_large_moore_256_t4() {
    set_thread_override(4);
    stress_2d_large_moore_256();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_large_moore_256_t8() {
    set_thread_override(8);
    stress_2d_large_moore_256();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_2d_large_vn_256_t1() {
    set_thread_override(1);
    stress_2d_large_vn_256();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_large_vn_256_t4() {
    set_thread_override(4);
    stress_2d_large_vn_256();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_large_vn_256_t8() {
    set_thread_override(8);
    stress_2d_large_vn_256();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_1d_large_rule30_2049_t1() {
    set_thread_override(1);
    stress_1d_large_rule30_2049();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_1d_large_rule30_2049_t4() {
    set_thread_override(4);
    stress_1d_large_rule30_2049();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_1d_large_rule30_2049_t8() {
    set_thread_override(8);
    stress_1d_large_rule30_2049();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_1d_three_state_cycle_t1() {
    set_thread_override(1);
    stress_1d_three_state_cycle();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_1d_three_state_cycle_t4() {
    set_thread_override(4);
    stress_1d_three_state_cycle();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_1d_three_state_cycle_t8() {
    set_thread_override(8);
    stress_1d_three_state_cycle();
    clear_thread_override();
}

#[test]
#[ignore]
fn stress_2d_three_state_cycle_t1() {
    set_thread_override(1);
    stress_2d_three_state_cycle();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_three_state_cycle_t4() {
    set_thread_override(4);
    stress_2d_three_state_cycle();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_three_state_cycle_t8() {
    set_thread_override(8);
    stress_2d_three_state_cycle();
    clear_thread_override();
}

// -------- DS-004 scenarios: threshold, history, many-type benches --------
//
// These fill the gaps listed in docs/performance.md section 5. They share one
// shape: a builder, then `run_benchmark_*`. Thread variants come from the small
// macro below (the older scenarios spell the same three wrappers out by hand).

/// Defines `_t1` / `_t4` / `_t8` ignored tests that run `$f` under that thread
/// override, exactly like the hand-written wrappers above.
macro_rules! thread_variants {
    ($f:ident => $t1:ident, $t4:ident, $t8:ident) => {
        #[test]
        #[ignore]
        fn $t1() {
            set_thread_override(1);
            $f();
            clear_thread_override();
        }
        #[test]
        #[ignore]
        fn $t4() {
            set_thread_override(4);
            $f();
            clear_thread_override();
        }
        #[test]
        #[ignore]
        fn $t8() {
            set_thread_override(8);
            $f();
            clear_thread_override();
        }
    };
}

/// Rule 30 on a 1D grid of `width` cells, one seed cell in the middle.
///
/// This is a two-state Wolfram rule, so `Grid1D::step` may take the packed
/// bit-parallel path, but only in a serial step: `step` first asks
/// `chunks_for_work(width * work_per_cell)` how many chunks the step deserves
/// and only uses the packed path when the answer is 1. Rule 30 here has two
/// subrules of `2n+1 = 3` visits, so `work_per_cell = 6`:
///   - width 65536  -> work 393 216 < MIN_WORK_PER_CHUNK (400 000) -> 1 chunk
///     -> PACKED at t1, t4 and t8 alike (just under the threshold).
///   - width 262144 -> work 1 572 864 -> 3 chunks at t4 and t8 -> the scalar
///     parallel path; at t1 it is still 1 chunk -> packed.
fn bench_rule30_wide(name: &str, w: usize, steps: usize) {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    let sub = |cur: CellType| Rule1DSubrule {
        current_type: cur,
        criteria_type: x,
        wolfram_code: 30,
        n: 1,
        randomness: None,
        output_type: x,
    };
    let rule = Rule1D {
        subrules: vec![sub(x), sub(inactive)],
    };
    let mut init = vec![inactive; w];
    init[w / 2] = x;
    let g = Grid1D::new(w, 4, init, rule);
    if ascii_enabled() {
        print_ascii_1d(&format!("{}: initial", name), &g);
    }
    if configs_export_enabled() {
        export_config_1d(name, &g);
    }
    run_benchmark_1d(name, &g, steps);
}

fn stress_1d_rule30_65536() {
    bench_rule30_wide("1d_rule30_65536", 65536, 200);
}
thread_variants!(stress_1d_rule30_65536 =>
    stress_1d_rule30_65536_t1, stress_1d_rule30_65536_t4, stress_1d_rule30_65536_t8);

fn stress_1d_rule30_262144() {
    bench_rule30_wide("1d_rule30_262144", 262144, 50);
}
thread_variants!(stress_1d_rule30_262144 =>
    stress_1d_rule30_262144_t1, stress_1d_rule30_262144_t4, stress_1d_rule30_262144_t8);

/// A cyclic rule over `n_types` types `C0..C{n-1}`. `Ci` becomes `C(i+1)` when
/// at least `count` of its 8 Moore neighbours are `C(i+1)` (`CountOp::Gt` means
/// "at least", inclusive); otherwise it stays `Ci` via a second, always-true
/// "stay" subrule (`Lt 8` is true for every possible count). Without the stay
/// subrules a cell with no matching subrule goes Inactive and the grid dies.
/// Not a two-type rule, so it always takes the scalar 2D path (never the
/// bit-plane fast path), and `work_per_cell = 2 * 8 * n_types` (the plan sums
/// the neighbour offsets of every subrule).
fn cyclic_rule_2d(n_types: usize, count: u32) -> Rule2D {
    let ty = |i: usize| CellType::from(format!("C{}", i % n_types).as_str());
    let advance = (0..n_types).map(|i| {
        Rule2DSubrule::new(
            ty(i),
            ty(i + 1),
            count,
            CountOp::Gt,
            1,
            Neighborhood2D::Moore,
            ty(i + 1),
            None,
            None,
        )
    });
    let stay = (0..n_types).map(|i| {
        Rule2DSubrule::new(
            ty(i),
            ty(i),
            8,
            CountOp::Lt,
            1,
            Neighborhood2D::Moore,
            ty(i),
            None,
            None,
        )
    });
    Rule2D {
        subrules: advance.chain(stay).collect(),
    }
}

/// Square grid of the three-state cycle, diagonal-stripe start.
fn cycle3_grid(side: usize, hist: usize) -> Grid2D {
    let ty = |i: usize| CellType::from(format!("C{}", i % 3).as_str());
    let mut init = Vec::with_capacity(side * side);
    for y in 0..side {
        for x in 0..side {
            init.push(ty(x + y));
        }
    }
    Grid2D::new(side, side, hist, init, cyclic_rule_2d(3, 1))
}

fn bench_cycle3(name: &str, side: usize, hist: usize, steps: usize) {
    let g = cycle3_grid(side, hist);
    if ascii_enabled() {
        print_ascii_2d(&format!("{}: initial", name), &g);
    }
    if configs_export_enabled() {
        export_config_2d(name, &g);
    }
    run_benchmark_2d(name, &g, steps);
}

// history_limit sweep. 128x128 x work_per_cell 48 = 786 432, which floors to
// 1 chunk (it would need 800 000 for 2), so every step is serial whatever the
// thread count; the only thing that changes between these three is the history
// bookkeeping.
// They are pinned to t1 so the baseline names do not depend on cella.properties.
fn stress_2d_cycle128_hist0() {
    bench_cycle3("2d_cycle128_hist0", 128, 0, 300);
}
fn stress_2d_cycle128_hist1() {
    bench_cycle3("2d_cycle128_hist1", 128, 1, 300);
}
fn stress_2d_cycle128_hist7() {
    bench_cycle3("2d_cycle128_hist7", 128, 7, 300);
}
#[test]
#[ignore]
fn stress_2d_cycle128_hist0_t1() {
    set_thread_override(1);
    stress_2d_cycle128_hist0();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_cycle128_hist1_t1() {
    set_thread_override(1);
    stress_2d_cycle128_hist1();
    clear_thread_override();
}
#[test]
#[ignore]
fn stress_2d_cycle128_hist7_t1() {
    set_thread_override(1);
    stress_2d_cycle128_hist7();
    clear_thread_override();
}

/// 12-type cyclic rule from a pseudo-random start: 24 subrules x 8 Moore
/// neighbours = work_per_cell 192, and a 12-entry per-step type counter.
/// 128x128 x 192 = 3 145 728 work -> 4 chunks at t4, 7 at t8.
fn stress_2d_cyclic12_128() {
    let w = 128usize;
    let ty = |i: usize| CellType::from(format!("C{}", i % 12).as_str());
    let mut init = Vec::with_capacity(w * w);
    for y in 0..w {
        for x in 0..w {
            // cheap deterministic integer hash -> uniform-ish type per cell
            let h = (x.wrapping_mul(73856093) ^ y.wrapping_mul(19349663)).wrapping_mul(2654435761);
            init.push(ty((h >> 7) % 12));
        }
    }
    let g = Grid2D::new(w, w, 3, init, cyclic_rule_2d(12, 2));
    if ascii_enabled() {
        print_ascii_2d("2d_cyclic12_128: initial", &g);
    }
    if configs_export_enabled() {
        export_config_2d("2d_cyclic12_128", &g);
    }
    run_benchmark_2d("2d_cyclic12_128", &g, 50);
}
thread_variants!(stress_2d_cyclic12_128 =>
    stress_2d_cyclic12_128_t1, stress_2d_cyclic12_128_t4, stress_2d_cyclic12_128_t8);

// Sizes straddling MIN_WORK_PER_CHUNK (400 000) on the three-state cycle
// (work_per_cell 48). `chunks = clamp(work / 400 000, 1, threads)` rounds DOWN,
// so a grid only gets a second chunk at 2x the threshold:
//   65x65   work   202 800  (~0.5x)  -> 1 chunk
//   92x92   work   406 272  (~1x)    -> 1 chunk (just over, still floors to 1)
//   130x130 work   811 200  (~2x)    -> 2 chunks at t4/t8
//   183x183 work 1 607 472  (~4x)    -> 4 chunks at t4/t8
// Steps are scaled so every size does about the same total cell-steps (~6 M),
// which makes the t1 column flat and the t4/t8 columns show the split's effect.
fn stress_2d_straddle_65() {
    bench_cycle3("2d_straddle_65", 65, 3, 1400);
}
fn stress_2d_straddle_92() {
    bench_cycle3("2d_straddle_92", 92, 3, 700);
}
fn stress_2d_straddle_130() {
    bench_cycle3("2d_straddle_130", 130, 3, 360);
}
fn stress_2d_straddle_183() {
    bench_cycle3("2d_straddle_183", 183, 3, 180);
}
thread_variants!(stress_2d_straddle_65 =>
    stress_2d_straddle_65_t1, stress_2d_straddle_65_t4, stress_2d_straddle_65_t8);
thread_variants!(stress_2d_straddle_92 =>
    stress_2d_straddle_92_t1, stress_2d_straddle_92_t4, stress_2d_straddle_92_t8);
thread_variants!(stress_2d_straddle_130 =>
    stress_2d_straddle_130_t1, stress_2d_straddle_130_t4, stress_2d_straddle_130_t8);
thread_variants!(stress_2d_straddle_183 =>
    stress_2d_straddle_183_t1, stress_2d_straddle_183_t4, stress_2d_straddle_183_t8);

/// One baseline entry in `tests/benchmarks_last.json`.
///
/// `avg` and `std_dev` are the original schema. The rest was added later and is
/// `#[serde(default)]`, so older files (which only have `avg`/`std_dev`) still
/// load; their missing fields read as `None` / 0 and the summary prints `n/a`
/// for the comparisons that need them.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
struct BenchStats {
    avg: f64,
    std_dev: f64,
    /// Fastest timed run, ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    min: Option<f64>,
    /// Median timed run, ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    median: Option<f64>,
    /// How many runs fell outside the MAD fence (see `find_outliers`).
    #[serde(default)]
    outliers: usize,
    /// 1-minute load average when the bench finished (Linux only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    load1: Option<f64>,
}

#[test]
fn bench_stats_serialize_deserialize_roundtrip() {
    let original = BenchStats {
        avg: 123.456,
        std_dev: 7.89,
        min: Some(120.0),
        median: Some(122.5),
        outliers: 2,
        load1: Some(0.75),
    };
    let json = serde_json::to_string(&original).expect("serialize BenchStats");
    let restored: BenchStats = serde_json::from_str(&json).expect("deserialize BenchStats");
    assert_eq!(original, restored);
}

#[test]
fn bench_stats_deserialize_known_json() {
    // Old schema: only avg + std_dev. Must keep parsing.
    let json = r#"{"avg":42.0,"std_dev":1.5}"#;
    let stats: BenchStats =
        serde_json::from_str(json).expect("deserialize BenchStats from known JSON");
    assert_eq!(stats.avg, 42.0);
    assert_eq!(stats.std_dev, 1.5);
    assert_eq!(stats.min, None);
    assert_eq!(stats.median, None);
    assert_eq!(stats.outliers, 0);
    assert_eq!(stats.load1, None);
    // A whole old-style file parses as a map too.
    let map: HashMap<String, BenchStats> =
        serde_json::from_str(r#"{"a_t1":{"avg":1.0,"std_dev":0.1}}"#).unwrap();
    assert_eq!(map["a_t1"].avg, 1.0);
}

#[test]
fn bench_stats_default_is_zero() {
    let stats = BenchStats::default();
    let json = serde_json::to_string(&stats).expect("serialize default BenchStats");
    let restored: BenchStats = serde_json::from_str(&json).expect("deserialize default BenchStats");
    assert_eq!(restored.avg, 0.0);
    assert_eq!(restored.std_dev, 0.0);
    assert!(!json.contains("min"), "absent optional fields are skipped");
}

#[test]
fn bench_summary_matches_plain_mean_std_and_adds_min_median() {
    let t = [10.0, 12.0, 11.0, 10.5, 11.5];
    let s = summarize_runs(&t);
    assert!((s.avg - 11.0).abs() < 1e-12);
    // population std: sqrt((1 + 1 + 0 + 0.25 + 0.25) / 5)
    assert!((s.std_dev - 0.5f64.sqrt()).abs() < 1e-12);
    assert_eq!(s.min, 10.0);
    assert_eq!(s.median, 11.0);
    assert!(s.outliers.is_empty());
    // even count -> mean of the two middle runs
    assert_eq!(summarize_runs(&[1.0, 2.0, 3.0, 4.0]).median, 2.5);
    // empty and single-run inputs do not panic
    assert_eq!(summarize_runs(&[]).avg, 0.0);
    assert_eq!(summarize_runs(&[7.0]).std_dev, 0.0);
}

#[test]
fn bench_outliers_are_flagged_and_still_counted_in_avg() {
    let mut t = vec![10.0, 10.1, 9.9, 10.05, 9.95, 10.0, 10.1, 9.9, 10.0];
    t.push(25.0); // one slow run
    let s = summarize_runs(&t);
    assert_eq!(s.outliers, vec![25.0]);
    // never dropped: the avg includes it
    assert!(s.avg > 11.0);
    // median/min are unaffected by it
    assert!((s.median - 10.0).abs() < 0.06);
    assert_eq!(s.min, 9.9);
}

#[test]
fn bench_outliers_need_enough_runs_and_ignore_tiny_jitter() {
    // too few runs: nothing flagged even if wild
    assert!(find_outliers(&[1.0, 1.0, 1.0, 100.0]).is_empty());
    // identical runs (MAD = 0): the 2 % floor keeps a 1 % wobble unflagged,
    // while a 10 % excursion is flagged.
    assert!(find_outliers(&[10.0, 10.0, 10.0, 10.0, 10.1]).is_empty());
    assert_eq!(find_outliers(&[10.0, 10.0, 10.0, 10.0, 11.0]), vec![11.0]);
}

#[test]
fn bench_loadavg_parsing() {
    assert_eq!(parse_loadavg("1.61 1.99 2.80 3/1258 786815\n"), Some(1.61));
    assert_eq!(parse_loadavg(""), None);
    assert_eq!(parse_loadavg("garbage 1 2"), None);
    assert!(load_text(Some(9.0)).contains("HIGH"));
    assert_eq!(load_text(None), "n/a");
}

// -------- Benchmark persistence helpers --------
fn benchmarks_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("benchmarks_last.json")
}

fn load_previous_benchmarks() -> HashMap<String, BenchStats> {
    let path = benchmarks_file();
    if let Ok(s) = fs::read_to_string(&path) {
        if let Ok(map) = serde_json::from_str::<HashMap<String, BenchStats>>(&s) {
            return map;
        }
    }
    HashMap::new()
}

fn save_current_benchmarks(
    map: &HashMap<String, BenchStats>,
) -> Result<(), Box<dyn std::error::Error>> {
    let s = serde_json::to_string_pretty(map)?;
    if let Some(parent) = benchmarks_file().parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(benchmarks_file(), s)?;
    Ok(())
}

#[test]
#[ignore]
fn stress_config_load_and_run() {
    let path = std::env::temp_dir().join(format!("cella_bench_config_{}.json", std::process::id()));

    let a = "Alive".to_string();
    let b = "Inactive".to_string();
    let rule = Rule2D {
        subrules: vec![Rule2DSubrule::new(
            CellType::from(b.as_str()),
            CellType::from(a.as_str()),
            0,
            CountOp::Gt,
            1,
            Neighborhood2D::Moore,
            CellType::from(a.as_str()),
            None,
            None,
        )],
    };

    let w = 100usize;
    let h = 100usize;
    let hist = 2usize;
    let mut initial = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            if x == w / 2 && y == h / 2 {
                initial.push(a.clone());
            } else {
                initial.push(b.clone());
            }
        }
    }

    let cfg = CellaConfig::D2(Config2D {
        ensemble: None,
        evolve: None,
        seed: 0,
        colors: Default::default(),
        width: w,
        height: h,
        history_limit: hist,
        initial,
        rule,
        model: None,
        snapshot: None,
    });

    // Save
    cfg.to_file_pretty(&path).expect("save config");

    // Load
    let t0 = Instant::now();
    let loaded = CellaConfig::from_file(&path).expect("load config");
    let load_time = t0.elapsed();
    println!("Config load time: {:?}", load_time);

    let g = loaded.build_grid2d().expect("build grid");

    // Benchmark
    run_benchmark_2d("stress_config_100x100", &g, 50);

    // Cleanup
    let _ = fs::remove_file(path);
}
