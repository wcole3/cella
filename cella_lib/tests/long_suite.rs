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
// ASCII dumps: Set CELLA_ASCII=1 to write ASCII renders of the initial and
// final states for each test to text files under tests/ascii/<testname>.txt.
//
// Config export: Set CELLA_EXPORT_CONFIGS=1 to write a JSON config file for
// each long test to configs/<testname>.json at the project root. These files
// can be loaded in the GUI to inspect the test case visually.
//   On Windows PowerShell:
//     $env:CELLA_EXPORT_CONFIGS=1; cargo test -p cella_lib -- --ignored; Remove-Item Env:CELLA_EXPORT_CONFIGS

use cella_lib::*;
use cella_lib::config::{CellaConfig, Config2D};
use cella_lib::threads::{set_thread_override, clear_thread_override, thread_count};
use serde::{Serialize, Deserialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use std::collections::HashMap;
use std::io::Write;

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
    // include dims and step
    acc ^= fnv1a64(&g.width.to_le_bytes());
    acc ^= fnv1a64(&g.step.to_le_bytes());
    for c in &g.cells {
        // TODO think about this
        acc ^= fnv1a64(c.current.as_str().as_bytes());
        acc = acc.wrapping_add(c.age_in_state as u64);
    }
    acc
}

fn hash_grid2d_state(g: &Grid2D) -> u64 {
    let mut acc: u64 = 0;
    acc ^= fnv1a64(&g.width.to_le_bytes());
    acc ^= fnv1a64(&g.height.to_le_bytes());
    acc ^= fnv1a64(&g.step.to_le_bytes());
    for c in &g.cells {
        acc ^= fnv1a64(c.current.as_str().as_bytes());
        acc = acc.wrapping_add(c.age_in_state as u64);
    }
    acc
}

fn snapshots_dir() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("snapshots") }

fn assert_snapshot(name: &str, value: u64) {
    let dir = snapshots_dir();
    let path = dir.join(format!("{}.txt", name));
    let val_hex = format!("{:016x}", value);
    let update = std::env::var("CELLA_UPDATE_SNAPSHOTS").ok().map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
    if update {
        let _ = fs::create_dir_all(&dir);
        fs::write(&path, &val_hex).expect("write snapshot");
        println!("updated snapshot {} => {}", name, path.display());
        return;
    }
    match fs::read_to_string(&path) {
        Ok(s) => {
            let s = s.trim();
            assert_eq!(s, val_hex, "snapshot mismatch for {} ({}): expected {}, got {}", name, path.display(), s, val_hex);
        }
        Err(_) => {
            eprintln!("snapshot missing for {} at {}. Set CELLA_UPDATE_SNAPSHOTS=1 to create it.", name, path.display());
            panic!("missing snapshot: {}", name);
        }
    }
}

// -------- Benchmark storage --------
static BENCH_DATA: OnceLock<Mutex<Vec<(String, u128)>>> = OnceLock::new();

fn bench_runs() -> usize {
    std::env::var("CELLA_BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10)
}

fn bench_store() -> &'static Mutex<Vec<(String, u128)>> {
    BENCH_DATA.get_or_init(|| Mutex::new(Vec::new()))
}
fn record_bench(name: &str, nanos: u128) {
    let suffix = format!("_t{}", thread_count());
    let full = format!("{}{}", name, suffix);
    let mut v = bench_store().lock().unwrap();
    v.push((full.clone(), nanos));
    if std::env::var("CELLA_BENCH").ok().as_deref() == Some("1") {
        println!("[bench] {:>28}: {:.6} ms", full, nanos as f64 / 1_000_000f64);
    }
}

fn run_benchmark_2d(name: &str, g_initial: &Grid2D, steps: usize) {
    for i in 0..bench_runs() {
        let mut g = g_initial.clone();
        let t0 = Instant::now();
        for _ in 0..steps { g.step(); }
        let elapsed = t0.elapsed().as_nanos();
        record_bench(name, elapsed);
        if i == 0 {
            if ascii_enabled() { print_ascii_2d(&format!("{}: final", name), &g); }
            let hash = hash_grid2d_state(&g);
            assert_snapshot(name, hash);
        }
    }
}

fn run_benchmark_1d(name: &str, g_initial: &Grid1D, steps: usize) {
    for i in 0..bench_runs() {
        let mut g = g_initial.clone();
        let t0 = Instant::now();
        for _ in 0..steps { g.step(); }
        let elapsed = t0.elapsed().as_nanos();
        record_bench(name, elapsed);
        if i == 0 {
            if ascii_enabled() { print_ascii_1d(&format!("{}: final", name), &g); }
            let hash = hash_grid1d_state(&g);
            assert_snapshot(name, hash);
        }
    }
}

// -------- Optional ASCII rendering helpers --------
fn ascii_enabled() -> bool {
    std::env::var("CELLA_ASCII").ok().map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false)
}

fn ascii_dir() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("ascii") }

// -------- Config export helpers --------
fn configs_export_enabled() -> bool {
    std::env::var("CELLA_EXPORT_CONFIGS").ok().map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false)
}

fn configs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap_or(Path::new(".")).join("configs")
}

fn export_config_2d(name: &str, g: &Grid2D) {
    use cella_lib::config::{CellaConfig, Config2D};
    let initial: Vec<String> = g.cells.iter().map(|c| c.current.as_str().to_string()).collect();
    let cfg = CellaConfig::D2(Config2D {
        width: g.width,
        height: g.height,
        history_limit: g.history_limit,
        initial,
        rule: g.rule.clone(),
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
    let initial: Vec<String> = g.cells.iter().map(|c| c.current.as_str().to_string()).collect();
    let cfg = CellaConfig::D1(Config1D {
        width: g.width,
        history_limit: g.history_limit,
        initial,
        rule: g.rule.clone(),
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
        let phase = label[idx+1..].trim().to_ascii_lowercase();
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
    if truncate { opts.truncate(true); } else { opts.append(true); }
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
    let mut names: Vec<String> = g
        .cells
        .iter()
        .map(|c| c.current.as_str().to_string())
        .filter(|n| n != INACTIVE)
        .collect();
    let map: BTreeMap<String, char> = ascii_symbols_map(&mut names);
    let mut line = String::with_capacity(g.width);
    for i in 0..g.width {
        let ty = g.cells[i].current.as_str();
        if ty == INACTIVE { line.push('.'); }
        else { line.push(*map.get(&ty.to_string()).unwrap_or(&'?')); }
    }
    if let Ok(mut f) = ascii_open_for(label) {
        let _ = writeln!(f, "[ascii] {} (1D w={})", label, g.width);
        let _ = writeln!(f, "{}", line);
        let _ = writeln!(f);
    }
}

fn print_ascii_2d(label: &str, g: &Grid2D) {
    use std::collections::BTreeMap;
    let mut names: Vec<String> = g
        .cells
        .iter()
        .map(|c| c.current.as_str().to_string())
        .filter(|n| n != INACTIVE)
        .collect();
    let map: BTreeMap<String, char> = ascii_symbols_map(&mut names);
    if let Ok(mut f) = ascii_open_for(label) {
        let _ = writeln!(f, "[ascii] {} (2D {}x{})", label, g.width, g.height);
        for y in 0..g.height {
            let mut line = String::with_capacity(g.width);
            for x in 0..g.width {
                let i = y * g.width + x;
                let ty = g.cells[i].current.as_str();
                if ty == INACTIVE { line.push('.'); }
                else { line.push(*map.get(&ty.to_string()).unwrap_or(&'?')); }
            }
            let _ = writeln!(f, "{}", line);
        }
        let _ = writeln!(f);
    }
}

fn stress_2d_life_like_moore() {
    let alive = CellType::from("Alive");
    let inactive = CellType::inactive();
    let rule = Rule2D { subrules: vec![
        // Overpopulation: Alive with >=4 neighbors becomes Inactive
        Rule2DSubrule { current_type: alive.clone(), criteria_type: alive.clone(), count: 4, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: inactive.clone() },
        // Survival: Alive stays Alive with >=2 neighbors
        Rule2DSubrule { current_type: alive.clone(), criteria_type: alive.clone(), count: 2, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
        // Birth: Inactive becomes Alive with ==3 neighbors
        Rule2DSubrule { current_type: inactive.clone(), criteria_type: alive.clone(), count: 3, op: CountOp::Eq, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
    ]};
    let (w,h,hist) = (50usize, 30usize, 5usize);
    let mut init = vec![CellType::inactive(); w*h];
    // seed: glider-like shape
    let mut set = |x: usize, y: usize| init[y*w + x] = alive.clone();
    set(1,0); set(2,1); set(0,2); set(1,2); set(2,2);
    let g = Grid2D::new(w,h,hist,init,rule);
    if ascii_enabled() { print_ascii_2d("2d_life_like_moore: initial", &g); }
    if configs_export_enabled() { export_config_2d("2d_life_like_moore", &g); }
    run_benchmark_2d("2d_life_like_moore", &g, 300);
}

fn stress_2d_von_neumann_threshold() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    // TODO need to make this rule more interesting
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: None, range: 2, neighborhood: Neighborhood2D::VonNeumann, randomness: None, output_type: b.clone() },
        Rule2DSubrule { current_type: b.clone(), criteria_type: b.clone(), count: 1, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::VonNeumann, randomness: None, output_type: b.clone() },
    ]};
    let (w,h,hist) = (64usize, 32usize, 3usize);
    let mut init = vec![a.clone(); w*h];
    // random-ish seed (deterministic pattern)
    for y in 0..h { for x in 0..w { if (x ^ y) % 7 == 0 { init[y*w + x] = b.clone(); } } }
    let g = Grid2D::new(w,h,hist,init,rule);
    if ascii_enabled() { print_ascii_2d("2d_vonneumann_threshold: initial", &g); }
    if configs_export_enabled() { export_config_2d("2d_vonneumann_threshold", &g); }
    run_benchmark_2d("2d_vonneumann_threshold", &g, 200);
}

fn stress_2d_straightline_threshold() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    // TODO make rule more interesting
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: None, range: 3, neighborhood: Neighborhood2D::StraightLine, randomness: None, output_type: b.clone() },
        Rule2DSubrule { current_type: b.clone(), criteria_type: b.clone(), count: 1, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::StraightLine, randomness: None, output_type: b.clone() },
    ]};
    let (w,h,hist) = (64usize, 32usize, 3usize);
    let mut init = vec![a.clone(); w*h];
    for y in 0..h { for x in 0..w { if (x * 13 + y * 7) % 17 == 0 { init[y*w + x] = b.clone(); } } }
    let g = Grid2D::new(w,h,hist,init,rule);
    if ascii_enabled() { print_ascii_2d("2d_straightline_threshold: initial", &g); }
    if configs_export_enabled() { export_config_2d("2d_straightline_threshold", &g); }
    run_benchmark_2d("2d_straightline_threshold", &g, 200);
}

fn stress_2d_langton_diagonals() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    // TODO need to make this rule more interesting
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 3, op: CountOp::Gt, limit: None, range: 2, neighborhood: Neighborhood2D::Langton, randomness: None, output_type: b.clone() },
    ]};
    let (w,h,hist) = (48usize, 48usize, 2usize);
    let mut init = vec![a.clone(); w*h];
    for i in 0..w.min(h) { init[i*w + i] = b.clone(); }
    let g = Grid2D::new(w,h,hist,init,rule);
    if ascii_enabled() { print_ascii_2d("2d_langton_diagonals: initial", &g); }
    if configs_export_enabled() { export_config_2d("2d_langton_diagonals", &g); }
    run_benchmark_2d("2d_langton_diagonals", &g, 180);
}

fn stress_2d_knight_neighborhood() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    // Conway-style birth/survival using Knight neighborhood (range=1 = 8 classic L-move squares)
    let rule = Rule2D { subrules: vec![
        // Overpopulation: A with >4 B knight-neighbors becomes B
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 4, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Knight, randomness: None, output_type: b.clone() },
        // Survival: A with 2..=4 B knight-neighbors stays A
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: Some(4), range: 1, neighborhood: Neighborhood2D::Knight, randomness: None, output_type: a.clone() },
        // Birth: B with ==3 A knight-neighbors becomes A
        Rule2DSubrule { current_type: b.clone(), criteria_type: a.clone(), count: 3, op: CountOp::Eq, limit: None, range: 1, neighborhood: Neighborhood2D::Knight, randomness: None, output_type: a.clone() },
    ]};
    let (w, h, hist) = (64usize, 48usize, 3usize);
    let mut init = vec![b.clone(); w * h];
    // deterministic seed: scatter A cells in a structured pattern
    for y in 0..h { for x in 0..w { if (x * 7 + y * 11) % 13 == 0 { init[y * w + x] = a.clone(); } } }
    let g = Grid2D::new(w, h, hist, init, rule);
    if ascii_enabled() { print_ascii_2d("2d_knight_neighborhood: initial", &g); }
    if configs_export_enabled() { export_config_2d("2d_knight_neighborhood", &g); }
    run_benchmark_2d("2d_knight_neighborhood", &g, 200);
}

fn stress_1d_rule30_center_seed() {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    let sub_active = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() };
    let sub_inactive = Rule1DSubrule { current_type: inactive.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub_active, sub_inactive] };
    let w = 257usize; let hist = 4usize;
    let mut init = vec![CellType::inactive(); w];
    init[w/2] = x.clone();
    let g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() { print_ascii_1d("1d_rule30_center: initial", &g); }
    if configs_export_enabled() { export_config_1d("1d_rule30_center", &g); }
    run_benchmark_1d("1d_rule30_center", &g, 500);
}

fn stress_1d_n2_alternating_code() {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    let code: u128 = 0xAAAAAAAA; // alternating bits over first 32 patterns
    // TODO make rule more interesting
    let sub_active = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: code, n: 2, randomness: None, output_type: x.clone() };
    let sub_inactive = Rule1DSubrule { current_type: inactive.clone(), criteria_type: x.clone(), wolfram_code: code, n: 2, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub_active, sub_inactive] };
    let w = 301usize; let hist = 3usize;
    let mut init = vec![inactive.clone(); w];
    init[w/2] = x.clone();
    let g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() { print_ascii_1d("1d_n2_alt: initial", &g); }
    if configs_export_enabled() { export_config_1d("1d_n2_alt", &g); }
    run_benchmark_1d("1d_n2_alt", &g, 400);
}

fn stress_1d_n3_custom_code() {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    // n=3 -> 2^(2*3+1)=2^7=128 patterns; pick a code with some structure
    let code: u128 = 0xF0F0_F0F0_F0F0_F0F0;
    // TODO make rule more interesting
    let sub_active = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: code, n: 3, randomness: None, output_type: x.clone() };
    let sub_inactive = Rule1DSubrule { current_type: inactive.clone(), criteria_type: x.clone(), wolfram_code: code, n: 3, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub_active, sub_inactive] };
    let w = 257usize; let hist = 2usize;
    let mut init = vec![inactive.clone(); w];
    init[w/2] = x.clone();
    let g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() { print_ascii_1d("1d_n3_custom: initial", &g); }
    if configs_export_enabled() { export_config_1d("1d_n3_custom", &g); }
    run_benchmark_1d("1d_n3_custom", &g, 350);
}

// -------- Larger stress tests to exercise multithreading --------

fn stress_1d_three_state_cycle() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    let any = 0xFFu128;
    let rule = Rule1D { subrules: vec![
        Rule1DSubrule { current_type: a.clone(), criteria_type: a.clone(), wolfram_code: any, n: 1, randomness: None, output_type: b.clone() },
        Rule1DSubrule { current_type: b.clone(), criteria_type: b.clone(), wolfram_code: any, n: 1, randomness: None, output_type: c.clone() },
        Rule1DSubrule { current_type: c.clone(), criteria_type: c.clone(), wolfram_code: any, n: 1, randomness: None, output_type: a.clone() },
    ]};
    let w = 1024usize; let hist = 3usize;
    let init = (0..w).map(|i| match i % 3 { 0 => a.clone(), 1 => b.clone(), _ => c.clone() }).collect::<Vec<_>>();
    let g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() { print_ascii_1d("1d_three_state_cycle: initial", &g); }
    if configs_export_enabled() { export_config_1d("1d_three_state_cycle", &g); }
    run_benchmark_1d("1d_three_state_cycle", &g, 800);
}

fn stress_2d_three_state_cycle() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 0, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() },
        Rule2DSubrule { current_type: b.clone(), criteria_type: c.clone(), count: 0, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: c.clone() },
        Rule2DSubrule { current_type: c.clone(), criteria_type: a.clone(), count: 0, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: a.clone() },
    ]};
    let (w,h,hist) = (192usize, 128usize, 3usize);
    let mut init = Vec::with_capacity(w*h);
    for y in 0..h { for x in 0..w { let idx = (x + y) % 3; init.push(match idx { 0 => a.clone(), 1 => b.clone(), _ => c.clone() }); } }
    let g = Grid2D::new(w,h,hist,init,rule);
    if ascii_enabled() { print_ascii_2d("2d_three_state_cycle: initial", &g); }
    if configs_export_enabled() { export_config_2d("2d_three_state_cycle", &g); }
    run_benchmark_2d("2d_three_state_cycle", &g, 240);
}

// -------- Larger stress tests to exercise multithreading --------

fn stress_2d_large_moore_256() {
    let alive = CellType::from("Alive");
    let inactive = CellType::inactive();
    let rule = Rule2D { subrules: vec![
        // Overpopulation: Alive with >=4 neighbors becomes Inactive
        Rule2DSubrule { current_type: alive.clone(), criteria_type: alive.clone(), count: 4, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: inactive.clone() },
        // Survival: Alive stays Alive with >=2 neighbors
        Rule2DSubrule { current_type: alive.clone(), criteria_type: alive.clone(), count: 2, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
        // Birth: Inactive becomes Alive with ==3 neighbors
        Rule2DSubrule { current_type: inactive.clone(), criteria_type: alive.clone(), count: 3, op: CountOp::Eq, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
    ]};
    let (w,h,hist) = (256usize, 256usize, 4usize);
    let mut init = vec![inactive.clone(); w*h];
    // Seed a few glider-like patterns along the diagonal
    for k in (0..w.min(h)).step_by(32) {
        let set = |x: usize, y: usize, v: &mut Vec<CellType>| v[y*w + x] = alive.clone();
        if k+2 < w && k+2 < h {
            set(k+1, k+0, &mut init);
            set(k+2, k+1, &mut init);
            set(k+0, k+2, &mut init);
            set(k+1, k+2, &mut init);
            set(k+2, k+2, &mut init);
        }
    }
    let g = Grid2D::new(w,h,hist,init,rule);
    if ascii_enabled() { print_ascii_2d("2d_large_moore_256: initial", &g); }
    if configs_export_enabled() { export_config_2d("2d_large_moore_256", &g); }
    run_benchmark_2d("2d_large_moore_256", &g, 200);
}

fn stress_2d_large_vn_256() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    // TODO make rule more interesting
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: None, range: 2, neighborhood: Neighborhood2D::VonNeumann, randomness: None, output_type: b.clone() },
        Rule2DSubrule { current_type: b.clone(), criteria_type: b.clone(), count: 1, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::VonNeumann, randomness: None, output_type: b.clone() },
    ]};
    let (w,h,hist) = (256usize, 256usize, 3usize);
    let mut init = vec![a.clone(); w*h];
    for y in 0..h { for x in 0..w { if (x*3 + y*5) % 11 == 0 { init[y*w + x] = b.clone(); } } }
    let g = Grid2D::new(w,h,hist,init,rule);
    if ascii_enabled() { print_ascii_2d("2d_large_vn_256: initial", &g); }
    if configs_export_enabled() { export_config_2d("2d_large_vn_256", &g); }
    run_benchmark_2d("2d_large_vn_256", &g, 160);
}

fn stress_1d_large_rule30_2049() {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    let sub_active = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() };
    let sub_inactive = Rule1DSubrule { current_type: inactive.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub_active, sub_inactive] };
    let w = 2049usize; let hist = 4usize;
    let mut init = vec![inactive.clone(); w];
    init[w/2] = x.clone();
    let g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() { print_ascii_1d("1d_large_rule30_2049: initial", &g); }
    if configs_export_enabled() { export_config_1d("1d_large_rule30_2049", &g); }
    run_benchmark_1d("1d_large_rule30_2049", &g, 1200);
}

// Final summary printer (likely last if run with --test-threads=1)
#[test]
#[ignore]
fn zzz_benchmark_summary() {
    let data = bench_store().lock().unwrap();
    if data.is_empty() {
        println!("[bench] No benchmarks recorded. Did you run with --ignored?");
        return;
    }
    // Group by name
    let mut groups: HashMap<String, Vec<f64>> = HashMap::new();
    for (name, ns) in data.iter() {
        groups.entry(name.clone()).or_default().push(*ns as f64 / 1_000_000f64); // convert to ms
    }

    // Load previous results if any
    let prev = load_previous_benchmarks();

    println!("\n[bench] Summary ({} runs for {} tests):", data.len(), groups.len());
    let mut total_avg: f64 = 0.0;
    let mut current_stats: HashMap<String, BenchStats> = HashMap::new();
    
    let mut names: Vec<_> = groups.keys().cloned().collect();
    names.sort();

    for name in names {
        let times = &groups[&name];
        let n = times.len() as f64;
        let sum: f64 = times.iter().sum();
        let avg = sum / n;
        
        let variance = if n > 1.0 {
            times.iter().map(|&t| {
                let diff = t - avg;
                diff * diff
            }).sum::<f64>() / n
        } else {
            0.0
        };
        let std_dev = variance.sqrt();
        let stats = BenchStats { avg, std_dev };
        current_stats.insert(name.clone(), stats.clone());
        total_avg += avg;

        if let Some(old) = prev.get(&name) {
            if old.avg > 0.0 {
                let diff = avg - old.avg;
                let pct = (diff * 100.0) / old.avg;
                let sign = if diff >= 0.0 { "+" } else { "" };
                println!("[bench] {:>28}: {:7.6} ms (±{:5.6} ms) (Δ {}{:7.6} ms, {:+.2}%)",
                         name, avg, std_dev, sign, diff, pct);
            } else {
                println!("[bench] {:>28}: {:7.6} ms (±{:5.6} ms) (Δ n/a)", name, avg, std_dev);
            }
        } else {
            println!("[bench] {:>28}: {:7.6} ms (±{:5.6} ms) (new)", name, avg, std_dev);
        }
    }
    println!("[bench] {:>28}: {:7.6} ms (sum of averages)", "TOTAL", total_avg);

    let update = std::env::var("CELLA_UPDATE_BENCH").ok().map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
    if update {
        if let Err(e) = save_current_benchmarks(&current_stats) {
            eprintln!("[bench] Failed to save benchmarks: {}", e);
        } else {
            println!("[bench] Saved current timings to {}", benchmarks_file().display());
        }
    } else {
        println!("[bench] Skipping save of benchmark baselines (set CELLA_UPDATE_BENCH=1 to update {}).", benchmarks_file().display());
    }
}


// -------- Thread-count variants (1,4,8) for all long tests --------
#[test]
#[ignore]
fn stress_2d_life_like_moore_t1() { set_thread_override(1); stress_2d_life_like_moore(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_life_like_moore_t4() { set_thread_override(4); stress_2d_life_like_moore(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_life_like_moore_t8() { set_thread_override(8); stress_2d_life_like_moore(); clear_thread_override(); }

#[test]
#[ignore]
fn stress_2d_von_neumann_threshold_t1() { set_thread_override(1); stress_2d_von_neumann_threshold(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_von_neumann_threshold_t4() { set_thread_override(4); stress_2d_von_neumann_threshold(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_von_neumann_threshold_t8() { set_thread_override(8); stress_2d_von_neumann_threshold(); clear_thread_override(); }

#[test]
#[ignore]
fn stress_2d_straightline_threshold_t1() { set_thread_override(1); stress_2d_straightline_threshold(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_straightline_threshold_t4() { set_thread_override(4); stress_2d_straightline_threshold(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_straightline_threshold_t8() { set_thread_override(8); stress_2d_straightline_threshold(); clear_thread_override(); }

#[test]
#[ignore]
fn stress_2d_langton_diagonals_t1() { set_thread_override(1); stress_2d_langton_diagonals(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_langton_diagonals_t4() { set_thread_override(4); stress_2d_langton_diagonals(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_langton_diagonals_t8() { set_thread_override(8); stress_2d_langton_diagonals(); clear_thread_override(); }

#[test]
#[ignore]
fn stress_2d_knight_neighborhood_t1() { set_thread_override(1); stress_2d_knight_neighborhood(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_knight_neighborhood_t4() { set_thread_override(4); stress_2d_knight_neighborhood(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_knight_neighborhood_t8() { set_thread_override(8); stress_2d_knight_neighborhood(); clear_thread_override(); }

#[test]
#[ignore]
fn stress_1d_rule30_center_seed_t1() { set_thread_override(1); stress_1d_rule30_center_seed(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_1d_rule30_center_seed_t4() { set_thread_override(4); stress_1d_rule30_center_seed(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_1d_rule30_center_seed_t8() { set_thread_override(8); stress_1d_rule30_center_seed(); clear_thread_override(); }

#[test]
#[ignore]
fn stress_1d_n2_alternating_code_t1() { set_thread_override(1); stress_1d_n2_alternating_code(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_1d_n2_alternating_code_t4() { set_thread_override(4); stress_1d_n2_alternating_code(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_1d_n2_alternating_code_t8() { set_thread_override(8); stress_1d_n2_alternating_code(); clear_thread_override(); }

#[test]
#[ignore]
fn stress_1d_n3_custom_code_t1() { set_thread_override(1); stress_1d_n3_custom_code(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_1d_n3_custom_code_t4() { set_thread_override(4); stress_1d_n3_custom_code(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_1d_n3_custom_code_t8() { set_thread_override(8); stress_1d_n3_custom_code(); clear_thread_override(); }

#[test]
#[ignore]
fn stress_2d_large_moore_256_t1() { set_thread_override(1); stress_2d_large_moore_256(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_large_moore_256_t4() { set_thread_override(4); stress_2d_large_moore_256(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_large_moore_256_t8() { set_thread_override(8); stress_2d_large_moore_256(); clear_thread_override(); }

#[test]
#[ignore]
fn stress_2d_large_vn_256_t1() { set_thread_override(1); stress_2d_large_vn_256(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_large_vn_256_t4() { set_thread_override(4); stress_2d_large_vn_256(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_large_vn_256_t8() { set_thread_override(8); stress_2d_large_vn_256(); clear_thread_override(); }

#[test]
#[ignore]
fn stress_1d_large_rule30_2049_t1() { set_thread_override(1); stress_1d_large_rule30_2049(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_1d_large_rule30_2049_t4() { set_thread_override(4); stress_1d_large_rule30_2049(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_1d_large_rule30_2049_t8() { set_thread_override(8); stress_1d_large_rule30_2049(); clear_thread_override(); }

#[test]
#[ignore]
fn stress_1d_three_state_cycle_t1() { set_thread_override(1); stress_1d_three_state_cycle(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_1d_three_state_cycle_t4() { set_thread_override(4); stress_1d_three_state_cycle(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_1d_three_state_cycle_t8() { set_thread_override(8); stress_1d_three_state_cycle(); clear_thread_override(); }

#[test]
#[ignore]
fn stress_2d_three_state_cycle_t1() { set_thread_override(1); stress_2d_three_state_cycle(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_three_state_cycle_t4() { set_thread_override(4); stress_2d_three_state_cycle(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_three_state_cycle_t8() { set_thread_override(8); stress_2d_three_state_cycle(); clear_thread_override(); }


#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
struct BenchStats {
    avg: f64,
    std_dev: f64,
}

#[test]
fn bench_stats_serialize_deserialize_roundtrip() {
    let original = BenchStats { avg: 123.456, std_dev: 7.89 };
    let json = serde_json::to_string(&original).expect("serialize BenchStats");
    let restored: BenchStats = serde_json::from_str(&json).expect("deserialize BenchStats");
    assert_eq!(original, restored);
}

#[test]
fn bench_stats_deserialize_known_json() {
    let json = r#"{"avg":42.0,"std_dev":1.5}"#;
    let stats: BenchStats = serde_json::from_str(json).expect("deserialize BenchStats from known JSON");
    assert_eq!(stats.avg, 42.0);
    assert_eq!(stats.std_dev, 1.5);
}

#[test]
fn bench_stats_default_is_zero() {
    let stats = BenchStats::default();
    let json = serde_json::to_string(&stats).expect("serialize default BenchStats");
    let restored: BenchStats = serde_json::from_str(&json).expect("deserialize default BenchStats");
    assert_eq!(restored.avg, 0.0);
    assert_eq!(restored.std_dev, 0.0);
}

// -------- Benchmark persistence helpers --------
fn benchmarks_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("benchmarks_last.json")
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

fn save_current_benchmarks(map: &HashMap<String, BenchStats>) -> Result<(), Box<dyn std::error::Error>> {
    let s = serde_json::to_string_pretty(map)?;
    if let Some(parent) = benchmarks_file().parent() { let _ = fs::create_dir_all(parent); }
    fs::write(benchmarks_file(), s)?;
    Ok(())
}

#[test]
#[ignore]
fn stress_config_load_and_run() {
    let path = std::env::temp_dir().join(format!("cella_bench_config_{}.json", std::process::id()));
    
    let a = "Alive".to_string();
    let b = "Inactive".to_string();
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule {
            current_type: CellType::from(b.as_str()),
            criteria_type: CellType::from(a.as_str()),
            count: 0,
            op: CountOp::Gt,
            limit: None,
            range: 1,
            neighborhood: Neighborhood2D::Moore,
            randomness: None,
            output_type: CellType::from(a.as_str()) 
        }
    ]};
    
    let w = 100usize;
    let h = 100usize;
    let hist = 2usize;
    let mut initial = Vec::with_capacity(w*h);
    for y in 0..h {
        for x in 0..w {
            if x == w/2 && y == h/2 { initial.push(a.clone()); }
            else { initial.push(b.clone()); }
        }
    }
    
    let cfg = CellaConfig::D2(Config2D {
        width: w,
        height: h,
        history_limit: hist,
        initial,
        rule,
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
