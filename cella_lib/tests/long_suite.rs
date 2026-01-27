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

use cella_lib::*;
use cella_lib::threads::{set_thread_override, clear_thread_override, thread_count};
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
        acc ^= fnv1a64(c.current.0.as_bytes());
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
        acc ^= fnv1a64(c.current.0.as_bytes());
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
fn bench_store() -> &'static Mutex<Vec<(String, u128)>> {
    BENCH_DATA.get_or_init(|| Mutex::new(Vec::new()))
}
fn record_bench(name: &str, ms: u128) {
    let suffix = format!("_t{}", thread_count());
    let full = format!("{}{}", name, suffix);
    let mut v = bench_store().lock().unwrap();
    v.push((full.clone(), ms));
    if std::env::var("CELLA_BENCH").ok().as_deref() == Some("1") {
        println!("[bench] {:>28}: {} ms", full, ms);
    }
}

// -------- Optional ASCII rendering helpers --------
fn ascii_enabled() -> bool {
    std::env::var("CELLA_ASCII").ok().map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false)
}

fn ascii_dir() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("ascii") }

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
        .map(|c| c.current.0.clone())
        .filter(|n| n != INACTIVE)
        .collect();
    let map: BTreeMap<String, char> = ascii_symbols_map(&mut names);
    let mut line = String::with_capacity(g.width);
    for i in 0..g.width {
        let ty = &g.cells[i].current.0;
        if ty == INACTIVE { line.push('.'); }
        else { line.push(*map.get(ty).unwrap_or(&'?')); }
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
        .map(|c| c.current.0.clone())
        .filter(|n| n != INACTIVE)
        .collect();
    let map: BTreeMap<String, char> = ascii_symbols_map(&mut names);
    if let Ok(mut f) = ascii_open_for(label) {
        let _ = writeln!(f, "[ascii] {} (2D {}x{})", label, g.width, g.height);
        for y in 0..g.height {
            let mut line = String::with_capacity(g.width);
            for x in 0..g.width {
                let i = y * g.width + x;
                let ty = &g.cells[i].current.0;
                if ty == INACTIVE { line.push('.'); }
                else { line.push(*map.get(ty).unwrap_or(&'?')); }
            }
            let _ = writeln!(f, "{}", line);
        }
        let _ = writeln!(f);
    }
}

fn stress_2d_life_like_moore() {
    let alive = CellType("Alive".into());
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
    let mut g = Grid2D::new(w,h,hist,init,rule);
    if ascii_enabled() { print_ascii_2d("2d_life_like_moore: initial", &g); }
    let t0 = Instant::now();
    for _ in 0..300 { g.step(); }
    if ascii_enabled() { print_ascii_2d("2d_life_like_moore: final", &g); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("2d_life_like_moore", elapsed);
    let hash = hash_grid2d_state(&g);
    assert_snapshot("2d_life_like_moore", hash);
}

fn stress_2d_von_neumann_threshold() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: None, range: 2, neighborhood: Neighborhood2D::VonNeumann, randomness: None, output_type: b.clone() },
        Rule2DSubrule { current_type: b.clone(), criteria_type: b.clone(), count: 1, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::VonNeumann, randomness: None, output_type: b.clone() },
    ]};
    let (w,h,hist) = (64usize, 32usize, 3usize);
    let mut init = vec![a.clone(); w*h];
    // random-ish seed (deterministic pattern)
    for y in 0..h { for x in 0..w { if (x ^ y) % 7 == 0 { init[y*w + x] = b.clone(); } } }
    let mut g = Grid2D::new(w,h,hist,init,rule);
    if ascii_enabled() { print_ascii_2d("2d_vonneumann_threshold: initial", &g); }
    let t0 = Instant::now();
    for _ in 0..200 { g.step(); }
    if ascii_enabled() { print_ascii_2d("2d_vonneumann_threshold: final", &g); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("2d_vonneumann_threshold", elapsed);
    let hash = hash_grid2d_state(&g);
    assert_snapshot("2d_vonneumann_threshold", hash);
}

fn stress_2d_straightline_threshold() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: None, range: 3, neighborhood: Neighborhood2D::StraightLine, randomness: None, output_type: b.clone() },
        Rule2DSubrule { current_type: b.clone(), criteria_type: b.clone(), count: 1, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::StraightLine, randomness: None, output_type: b.clone() },
    ]};
    let (w,h,hist) = (64usize, 32usize, 3usize);
    let mut init = vec![a.clone(); w*h];
    for y in 0..h { for x in 0..w { if (x * 13 + y * 7) % 17 == 0 { init[y*w + x] = b.clone(); } } }
    let mut g = Grid2D::new(w,h,hist,init,rule);
    if ascii_enabled() { print_ascii_2d("2d_straightline_threshold: initial", &g); }
    let t0 = Instant::now();
    for _ in 0..200 { g.step(); }
    if ascii_enabled() { print_ascii_2d("2d_straightline_threshold: final", &g); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("2d_straightline_threshold", elapsed);
    let hash = hash_grid2d_state(&g);
    assert_snapshot("2d_straightline_threshold", hash);
}

fn stress_2d_langdon_diagonals() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 3, op: CountOp::Gt, limit: None, range: 2, neighborhood: Neighborhood2D::Langdon, randomness: None, output_type: b.clone() },
    ]};
    let (w,h,hist) = (48usize, 48usize, 2usize);
    let mut init = vec![a.clone(); w*h];
    for i in 0..w.min(h) { init[i*w + i] = b.clone(); }
    let mut g = Grid2D::new(w,h,hist,init,rule);
    if ascii_enabled() { print_ascii_2d("2d_langdon_diagonals: initial", &g); }
    let t0 = Instant::now();
    for _ in 0..180 { g.step(); }
    if ascii_enabled() { print_ascii_2d("2d_langdon_diagonals: final", &g); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("2d_langdon_diagonals", elapsed);
    let hash = hash_grid2d_state(&g);
    assert_snapshot("2d_langdon_diagonals", hash);
}

fn stress_1d_rule30_center_seed() {
    let x = CellType("X".into());
    let inactive = CellType::inactive();
    let sub_active = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() };
    let sub_inactive = Rule1DSubrule { current_type: inactive.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub_active, sub_inactive] };
    let w = 257usize; let hist = 4usize;
    let mut init = vec![CellType::inactive(); w];
    init[w/2] = x.clone();
    let mut g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() { print_ascii_1d("1d_rule30_center: initial", &g); }
    let t0 = Instant::now();
    for _ in 0..500 { g.step(); }
    if ascii_enabled() { print_ascii_1d("1d_rule30_center: final", &g); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("1d_rule30_center", elapsed);
    let hash = hash_grid1d_state(&g);
    assert_snapshot("1d_rule30_center", hash);
}

fn stress_1d_n2_alternating_code() {
    let x = CellType("X".into());
    let inactive = CellType::inactive();
    let code: u128 = 0xAAAAAAAA; // alternating bits over first 32 patterns
    let sub_active = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: code, n: 2, randomness: None, output_type: x.clone() };
    let sub_inactive = Rule1DSubrule { current_type: inactive.clone(), criteria_type: x.clone(), wolfram_code: code, n: 2, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub_active, sub_inactive] };
    let w = 301usize; let hist = 3usize;
    let mut init = vec![inactive.clone(); w];
    init[w/2] = x.clone();
    let mut g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() { print_ascii_1d("1d_n2_alt: initial", &g); }
    let t0 = Instant::now();
    for _ in 0..400 { g.step(); }
    if ascii_enabled() { print_ascii_1d("1d_n2_alt: final", &g); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("1d_n2_alt", elapsed);
    let hash = hash_grid1d_state(&g);
    assert_snapshot("1d_n2_alt", hash);
}

fn stress_1d_n3_custom_code() {
    let x = CellType("X".into());
    let inactive = CellType::inactive();
    // n=3 -> 2^(2*3+1)=2^7=128 patterns; pick a code with some structure
    let code: u128 = 0xF0F0_F0F0_F0F0_F0F0;
    let sub_active = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: code, n: 3, randomness: None, output_type: x.clone() };
    let sub_inactive = Rule1DSubrule { current_type: inactive.clone(), criteria_type: x.clone(), wolfram_code: code, n: 3, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub_active, sub_inactive] };
    let w = 257usize; let hist = 2usize;
    let mut init = vec![inactive.clone(); w];
    init[w/2] = x.clone();
    let mut g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() { print_ascii_1d("1d_n3_custom: initial", &g); }
    let t0 = Instant::now();
    for _ in 0..350 { g.step(); }
    if ascii_enabled() { print_ascii_1d("1d_n3_custom: final", &g); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("1d_n3_custom", elapsed);
    let hash = hash_grid1d_state(&g);
    assert_snapshot("1d_n3_custom", hash);
}

// -------- Larger stress tests to exercise multithreading --------

fn stress_1d_three_state_cycle() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let c = CellType("C".into());
    let any = 0xFFu128;
    let rule = Rule1D { subrules: vec![
        Rule1DSubrule { current_type: a.clone(), criteria_type: a.clone(), wolfram_code: any, n: 1, randomness: None, output_type: b.clone() },
        Rule1DSubrule { current_type: b.clone(), criteria_type: b.clone(), wolfram_code: any, n: 1, randomness: None, output_type: c.clone() },
        Rule1DSubrule { current_type: c.clone(), criteria_type: c.clone(), wolfram_code: any, n: 1, randomness: None, output_type: a.clone() },
    ]};
    let w = 1024usize; let hist = 3usize;
    let init = (0..w).map(|i| match i % 3 { 0 => a.clone(), 1 => b.clone(), _ => c.clone() }).collect::<Vec<_>>();
    let mut g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() { print_ascii_1d("1d_three_state_cycle: initial", &g); }
    let t0 = Instant::now();
    for _ in 0..800 { g.step(); }
    if ascii_enabled() { print_ascii_1d("1d_three_state_cycle: final", &g); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("1d_three_state_cycle", elapsed);
    let hash = hash_grid1d_state(&g);
    assert_snapshot("1d_three_state_cycle", hash);
}

fn stress_2d_three_state_cycle() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let c = CellType("C".into());
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 0, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() },
        Rule2DSubrule { current_type: b.clone(), criteria_type: c.clone(), count: 0, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: c.clone() },
        Rule2DSubrule { current_type: c.clone(), criteria_type: a.clone(), count: 0, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: a.clone() },
    ]};
    let (w,h,hist) = (192usize, 128usize, 3usize);
    let mut init = Vec::with_capacity(w*h);
    for y in 0..h { for x in 0..w { let idx = (x + y) % 3; init.push(match idx { 0 => a.clone(), 1 => b.clone(), _ => c.clone() }); } }
    let mut g = Grid2D::new(w,h,hist,init,rule);
    if ascii_enabled() { print_ascii_2d("2d_three_state_cycle: initial", &g); }
    let t0 = Instant::now();
    for _ in 0..240 { g.step(); }
    if ascii_enabled() { print_ascii_2d("2d_three_state_cycle: final", &g); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("2d_three_state_cycle", elapsed);
    let hash = hash_grid2d_state(&g);
    assert_snapshot("2d_three_state_cycle", hash);
}

// -------- Larger stress tests to exercise multithreading --------

fn stress_2d_large_moore_256() {
    let alive = CellType("Alive".into());
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
    let mut g = Grid2D::new(w,h,hist,init,rule);
    if ascii_enabled() { print_ascii_2d("2d_large_moore_256: initial", &g); }
    let t0 = Instant::now();
    for _ in 0..200 { g.step(); }
    if ascii_enabled() { print_ascii_2d("2d_large_moore_256: final", &g); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("2d_large_moore_256", elapsed);
    let hash = hash_grid2d_state(&g);
    assert_snapshot("2d_large_moore_256", hash);
}

fn stress_2d_large_vn_256() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: None, range: 2, neighborhood: Neighborhood2D::VonNeumann, randomness: None, output_type: b.clone() },
        Rule2DSubrule { current_type: b.clone(), criteria_type: b.clone(), count: 1, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::VonNeumann, randomness: None, output_type: b.clone() },
    ]};
    let (w,h,hist) = (256usize, 256usize, 3usize);
    let mut init = vec![a.clone(); w*h];
    for y in 0..h { for x in 0..w { if (x*3 + y*5) % 11 == 0 { init[y*w + x] = b.clone(); } } }
    let mut g = Grid2D::new(w,h,hist,init,rule);
    if ascii_enabled() { print_ascii_2d("2d_large_vn_256: initial", &g); }
    let t0 = Instant::now();
    for _ in 0..160 { g.step(); }
    if ascii_enabled() { print_ascii_2d("2d_large_vn_256: final", &g); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("2d_large_vn_256", elapsed);
    let hash = hash_grid2d_state(&g);
    assert_snapshot("2d_large_vn_256", hash);
}

fn stress_1d_large_rule30_2049() {
    let x = CellType("X".into());
    let inactive = CellType::inactive();
    let sub_active = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() };
    let sub_inactive = Rule1DSubrule { current_type: inactive.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub_active, sub_inactive] };
    let w = 2049usize; let hist = 4usize;
    let mut init = vec![inactive.clone(); w];
    init[w/2] = x.clone();
    let mut g = Grid1D::new(w, hist, init, rule);
    if ascii_enabled() { print_ascii_1d("1d_large_rule30_2049: initial", &g); }
    let t0 = Instant::now();
    for _ in 0..1200 { g.step(); }
    if ascii_enabled() { print_ascii_1d("1d_large_rule30_2049: final", &g); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("1d_large_rule30_2049", elapsed);
    let hash = hash_grid1d_state(&g);
    assert_snapshot("1d_large_rule30_2049", hash);
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
    // Build current map
    let mut current: HashMap<String, u128> = HashMap::new();
    for (name, ms) in data.iter() { current.insert(name.clone(), *ms); }
    // Load previous results if any
    let prev = load_previous_benchmarks();

    println!("\n[bench] Summary ({} entries):", data.len());
    let mut total: u128 = 0;
    let mut entries = data.clone();
    entries.sort_by(|a,b| a.0.cmp(&b.0));
    for (name, ms) in entries {
        total += ms;
        if let Some(old) = prev.get(&name) {
            if *old > 0 {
                let diff = ms as i128 - *old as i128;
                let pct = (diff as f64) * 100.0 / (*old as f64);
                let sign = if diff >= 0 { "+" } else { "" };
                println!("[bench] {:>28}: {} ms (Δ {}{} ms, {:+.2}%)", name, ms, sign, diff, pct);
            } else {
                println!("[bench] {:>28}: {} ms (Δ n/a)", name, ms);
            }
        } else {
            println!("[bench] {:>28}: {} ms (new)", name, ms);
        }
    }
    println!("[bench] {:>28}: {} ms (sum)", "TOTAL", total);
    let update = std::env::var("CELLA_UPDATE_BENCH").ok().map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
    if update {
        if let Err(e) = save_current_benchmarks(&current) {
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
fn stress_2d_langdon_diagonals_t1() { set_thread_override(1); stress_2d_langdon_diagonals(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_langdon_diagonals_t4() { set_thread_override(4); stress_2d_langdon_diagonals(); clear_thread_override(); }
#[test]
#[ignore]
fn stress_2d_langdon_diagonals_t8() { set_thread_override(8); stress_2d_langdon_diagonals(); clear_thread_override(); }

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


// -------- Benchmark persistence helpers --------
fn benchmarks_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("benchmarks_last.json")
}

fn load_previous_benchmarks() -> HashMap<String, u128> {
    let path = benchmarks_file();
    if let Ok(s) = fs::read_to_string(&path) {
        if let Ok(map) = serde_json::from_str::<HashMap<String, u128>>(&s) {
            return map;
        }
    }
    HashMap::new()
}

fn save_current_benchmarks(map: &HashMap<String, u128>) -> Result<(), Box<dyn std::error::Error>> {
    let s = serde_json::to_string_pretty(map)?;
    if let Some(parent) = benchmarks_file().parent() { let _ = fs::create_dir_all(parent); }
    fs::write(benchmarks_file(), s)?;
    Ok(())
}
