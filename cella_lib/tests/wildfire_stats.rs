//! Statistical ensemble tests for the wildfire model. Tolerance-based, not
//! exact: they assert distributional properties over a seed ensemble, so they
//! are `#[ignore]`d alongside the other long-running tests.
//!
//! Run with: `cargo test -p cella_lib --release --test wildfire_stats -- --ignored`

use cella_lib::wildfire::{FuelClass, WildfireEnv, WildfireModel, WildfireParams};
use cella_lib::{CellType, Grid2D, Rule2D};

const W: usize = 48;
const H: usize = 48;
const SEEDS: u64 = 30;

fn params(seed: u64, wind_speed: f64, wind_from_deg: f64) -> WildfireParams {
    WildfireParams {
        seed,
        p0: 0.35,
        fuels: vec![FuelClass {
            name: "Forest".into(),
            veg_factor: 1.0,
        }],
        wind_speed,
        wind_from_deg,
        c1: 0.045,
        c2: 0.131,
        slope_a: 0.078,
        cell_size: 30.0,
        // Two burning steps per cell: with burn_duration 1 about 6% of runs
        // die at the spark (all eight neighbors miss), which is physical but
        // makes ensemble assertions flaky.
        burn_duration: 2,
        spotting: None,
        burning_name: None,
        burned_name: None,
        spread: "bernoulli".into(),
        arrival_jitter: 0.2,
        wind_law: "exponential".into(),
    }
}

/// Uniform forest with a centre ignition.
fn run(seed: u64, wind_speed: f64, wind_from_deg: f64, steps: usize) -> Grid2D {
    let mut init = vec![CellType::new("Forest"); W * H];
    init[(H / 2) * W + W / 2] = CellType::new("Burning");
    let mut g = Grid2D::new(W, H, 0, init, Rule2D { subrules: vec![] });
    g.attach_model(Box::new(WildfireModel::new(
        params(seed, wind_speed, wind_from_deg),
        WildfireEnv::default(),
    )))
    .unwrap();
    for _ in 0..steps {
        g.step();
    }
    g
}

fn burned_fraction(g: &Grid2D) -> f64 {
    let burned = CellType::new("BurnedOut");
    let burning = CellType::new("Burning");
    let n = (0..W * H)
        .filter(|&i| g.cell_type(i) == burned || g.cell_type(i) == burning)
        .count();
    n as f64 / (W * H) as f64
}

/// Centroid of fire-affected cells, in cell coordinates.
fn fire_centroid(g: &Grid2D) -> (f64, f64) {
    let burned = CellType::new("BurnedOut");
    let burning = CellType::new("Burning");
    let (mut sx, mut sy, mut n) = (0.0f64, 0.0f64, 0u32);
    for y in 0..H {
        for x in 0..W {
            let t = g.cell_type(y * W + x);
            if t == burned || t == burning {
                sx += x as f64;
                sy += y as f64;
                n += 1;
            }
        }
    }
    (sx / f64::from(n), sy / f64::from(n))
}

#[test]
#[ignore]
fn ensemble_burned_fraction_within_band() {
    // No wind, flat, 15 steps from a centre spark. The front advances at most
    // one cell per step, so the burn is geometrically capped at roughly a
    // radius-16 disc (~2.4% to ~35% of the 48x48 grid depending on how well
    // the fire percolates). The band is deliberately loose (tolerance-based
    // test, not exact): it catches gross regressions — the fire dying at the
    // spark, or spreading impossibly fast — not small parameter drift.
    let steps = 15;
    let mut fractions: Vec<f64> = Vec::new();
    for seed in 0..SEEDS {
        fractions.push(burned_fraction(&run(seed, 0.0, 0.0, steps)));
    }
    let mean = fractions.iter().sum::<f64>() / fractions.len() as f64;
    assert!(
        (0.02..=0.40).contains(&mean),
        "ensemble mean burned fraction {mean:.3} outside sanity band; fractions: {fractions:?}"
    );
    // The fire must spread beyond its ignition point in nearly every run; an
    // occasional instant die-out is legitimate stochastic behaviour.
    let spread = fractions
        .iter()
        .filter(|&&f| f > 1.5 / (W * H) as f64)
        .count();
    assert!(
        spread as f64 >= 0.9 * SEEDS as f64,
        "fire failed to spread in {} of {SEEDS} runs",
        SEEDS as usize - spread
    );
    // ...and stochasticity must produce run-to-run variation.
    let min = fractions.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = fractions.iter().cloned().fold(0.0f64, f64::max);
    assert!(max > min, "ensemble is degenerate: all runs identical");
}

#[test]
#[ignore]
fn ensemble_centroid_is_displaced_downwind() {
    // Strong easterly wind (blowing toward +x): the ensemble-mean fire
    // centroid must sit east of the ignition point, and further east than the
    // no-wind ensemble's centroid.
    let steps = 20;
    let ignition_x = (W / 2) as f64;
    let mut wind_dx = 0.0;
    let mut calm_dx = 0.0;
    for seed in 0..SEEDS {
        wind_dx += fire_centroid(&run(seed, 10.0, 270.0, steps)).0 - ignition_x;
        calm_dx += fire_centroid(&run(seed, 0.0, 0.0, steps)).0 - ignition_x;
    }
    wind_dx /= SEEDS as f64;
    calm_dx /= SEEDS as f64;
    assert!(
        wind_dx > calm_dx + 0.5,
        "wind must push the burn centroid downwind: wind dx {wind_dx:.2} vs calm dx {calm_dx:.2}"
    );
    assert!(
        wind_dx > 0.5,
        "downwind displacement expected, got {wind_dx:.2}"
    );
    assert!(
        calm_dx.abs() < 2.0,
        "calm ensemble should stay roughly centred, got {calm_dx:.2}"
    );
}
