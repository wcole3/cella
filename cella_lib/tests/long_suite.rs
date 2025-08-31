// Long-running stress tests for cella_lib (ignored by default).
//
// How to use:
// - List ignored tests: cargo test -p cella_lib -- --ignored --list
// - Run all ignored tests: cargo test -p cella_lib -- --ignored
// - Update snapshots (writes tests/snapshots/*.txt):
//     CELLA_UPDATE_SNAPSHOTS=1 cargo test -p cella_lib -- --ignored
//   On Windows PowerShell:
//     $env:CELLA_UPDATE_SNAPSHOTS=1; cargo test -p cella_lib -- --ignored; Remove-Item Env:CELLA_UPDATE_SNAPSHOTS
//
// The snapshots are deterministic hashes of the final grid state after a large
// number of steps. If engine behavior changes (intentionally or not), the hash
// will differ, prompting a snapshot update.

use cella_lib::*;
use std::fs;
use std::path::{Path, PathBuf};

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
    for _ in 0..300 { g.step(); }
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
    for _ in 0..200 { g.step(); }
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
    for _ in 0..180 { g.step(); }
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
    for _ in 0..500 { g.step(); }
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
    for _ in 0..400 { g.step(); }
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
    for _ in 0..350 { g.step(); }
    let hash = hash_grid1d_state(&g);
    assert_snapshot("1d_n3_custom", hash);
}
