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
//
// The snapshots are deterministic hashes of the final grid state after a large
// number of steps. If engine behavior changes (intentionally or not), the hash
// will differ, prompting a snapshot update.
//
// Benchmarks: Each test records elapsed wall time. A final ignored test
// `zzz_benchmark_summary` prints a summary of all recorded times. You can also
// set CELLA_BENCH=1 to print per-test timings immediately.

use cella_lib::*;
use std::fs;
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

fn hash_rule_serde_1d(rule: &Rule1D) -> u64 {
    let s = serde_json::to_string(rule).unwrap();
    fnv1a64(s.as_bytes())
}

fn hash_rule_serde_2d(rule: &Rule2D) -> u64 {
    let s = serde_json::to_string(rule).unwrap();
    fnv1a64(s.as_bytes())
}

fn hash_grid1d_state(g: &Grid1D) -> u64 {
    let mut acc: u64 = 0;
    // include dims and step
    acc ^= fnv1a64(&g.width.to_le_bytes());
    acc ^= fnv1a64(&g.step.to_le_bytes());
    acc ^= hash_rule_serde_1d(&g.rule);
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
    acc ^= hash_rule_serde_2d(&g.rule);
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
    let mut v = bench_store().lock().unwrap();
    v.push((name.to_string(), ms));
    if std::env::var("CELLA_BENCH").ok().as_deref() == Some("1") {
        println!("[bench] {:>28}: {} ms", name, ms);
    }
}

#[test]
#[ignore]
fn stress_2d_life_like_moore() {
    let alive = CellType("Alive".into());
    let inactive = CellType::inactive();
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: alive.clone(), criteria_type: alive.clone(), threshold: 2, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
        Rule2DSubrule { current_type: inactive.clone(), criteria_type: alive.clone(), threshold: 3, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
    ]};
    let (w,h,hist) = (50usize, 30usize, 5usize);
    let mut init = vec![CellType::inactive(); w*h];
    // seed: glider-like shape
    let mut set = |x: usize, y: usize| init[y*w + x] = alive.clone();
    set(1,0); set(2,1); set(0,2); set(1,2); set(2,2);
    let mut g = Grid2D::new(w,h,hist,init,rule);
    let t0 = Instant::now();
    for _ in 0..300 { g.step(); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("2d_life_like_moore", elapsed);
    let hash = hash_grid2d_state(&g);
    assert_snapshot("2d_life_like_moore", hash);
}

#[test]
#[ignore]
fn stress_2d_von_neumann_threshold() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), threshold: 2, range: 2, neighborhood: Neighborhood2D::VonNeumann, randomness: None, output_type: b.clone() },
        Rule2DSubrule { current_type: b.clone(), criteria_type: b.clone(), threshold: 1, range: 1, neighborhood: Neighborhood2D::VonNeumann, randomness: None, output_type: b.clone() },
    ]};
    let (w,h,hist) = (64usize, 32usize, 3usize);
    let mut init = vec![a.clone(); w*h];
    // random-ish seed (deterministic pattern)
    for y in 0..h { for x in 0..w { if (x ^ y) % 7 == 0 { init[y*w + x] = b.clone(); } } }
    let mut g = Grid2D::new(w,h,hist,init,rule);
    let t0 = Instant::now();
    for _ in 0..200 { g.step(); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("2d_vonneumann_threshold", elapsed);
    let hash = hash_grid2d_state(&g);
    assert_snapshot("2d_vonneumann_threshold", hash);
}

#[test]
#[ignore]
fn stress_2d_langdon_diagonals() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), threshold: 3, range: 2, neighborhood: Neighborhood2D::Langdon, randomness: None, output_type: b.clone() },
    ]};
    let (w,h,hist) = (48usize, 48usize, 2usize);
    let mut init = vec![a.clone(); w*h];
    for i in 0..w.min(h) { init[i*w + i] = b.clone(); }
    let mut g = Grid2D::new(w,h,hist,init,rule);
    let t0 = Instant::now();
    for _ in 0..180 { g.step(); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("2d_langdon_diagonals", elapsed);
    let hash = hash_grid2d_state(&g);
    assert_snapshot("2d_langdon_diagonals", hash);
}

#[test]
#[ignore]
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
    let t0 = Instant::now();
    for _ in 0..500 { g.step(); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("1d_rule30_center", elapsed);
    let hash = hash_grid1d_state(&g);
    assert_snapshot("1d_rule30_center", hash);
}

#[test]
#[ignore]
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
    let t0 = Instant::now();
    for _ in 0..400 { g.step(); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("1d_n2_alt", elapsed);
    let hash = hash_grid1d_state(&g);
    assert_snapshot("1d_n2_alt", hash);
}

#[test]
#[ignore]
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
    let t0 = Instant::now();
    for _ in 0..350 { g.step(); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("1d_n3_custom", elapsed);
    let hash = hash_grid1d_state(&g);
    assert_snapshot("1d_n3_custom", hash);
}

// -------- Larger stress tests to exercise multithreading --------

#[test]
#[ignore]
fn stress_2d_large_moore_256() {
    let alive = CellType("Alive".into());
    let inactive = CellType::inactive();
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: alive.clone(), criteria_type: alive.clone(), threshold: 2, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
        Rule2DSubrule { current_type: inactive.clone(), criteria_type: alive.clone(), threshold: 3, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
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
    let t0 = Instant::now();
    for _ in 0..200 { g.step(); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("2d_large_moore_256", elapsed);
    let hash = hash_grid2d_state(&g);
    assert_snapshot("2d_large_moore_256", hash);
}

#[test]
#[ignore]
fn stress_2d_large_vn_256() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), threshold: 2, range: 2, neighborhood: Neighborhood2D::VonNeumann, randomness: None, output_type: b.clone() },
        Rule2DSubrule { current_type: b.clone(), criteria_type: b.clone(), threshold: 1, range: 1, neighborhood: Neighborhood2D::VonNeumann, randomness: None, output_type: b.clone() },
    ]};
    let (w,h,hist) = (256usize, 256usize, 3usize);
    let mut init = vec![a.clone(); w*h];
    for y in 0..h { for x in 0..w { if (x*3 + y*5) % 11 == 0 { init[y*w + x] = b.clone(); } } }
    let mut g = Grid2D::new(w,h,hist,init,rule);
    let t0 = Instant::now();
    for _ in 0..160 { g.step(); }
    let elapsed = t0.elapsed().as_millis();
    record_bench("2d_large_vn_256", elapsed);
    let hash = hash_grid2d_state(&g);
    assert_snapshot("2d_large_vn_256", hash);
}

#[test]
#[ignore]
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
    let t0 = Instant::now();
    for _ in 0..1200 { g.step(); }
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
    println!("\n[bench] Summary ({} entries):", data.len());
    let mut total: u128 = 0;
    let mut entries = data.clone();
    entries.sort_by(|a,b| a.0.cmp(&b.0));
    for (name, ms) in entries {
        println!("[bench] {:>28}: {} ms", name, ms);
        total += ms;
    }
    println!("[bench] {:>28}: {} ms (sum)", "TOTAL", total);
}
