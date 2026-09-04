//! Measure the model's own rate of spread so one tick can be given a real
//! duration (validation experiment E19).
//!
//! The scenario format declares `steps_per_hour` (50 ticks/day for the
//! six-fire pack, copied from the papers) but nothing ties a tick to a
//! physical time. In a CA the front advances at some fraction of one cell
//! per tick that depends on p0, wind, and burn duration; multiplied by the
//! cell size, that is a rate of spread in metres per tick. Matching it to
//! an observed or Rothermel rate of spread fixes the tick length.
//!
//! Set-up: homogeneous fuel (veg_factor 1, density 1, flat), a full-height
//! burning column at x = 0..2 so the front is a straight line, wind blowing
//! toward +x (from 270°) or calm. The front position is the mean x of the
//! rightmost burning/burned cell per row; speed is the slope of a linear
//! fit over the steady part of the run. Output: one JSON line per case.
//!
//! Usage (from cella_lib/): cargo run --release --example wildfire_ros

use cella_lib::{CellType, FuelClass, Grid2D, Rule2D, WildfireEnv, WildfireModel, WildfireParams};

const W: usize = 240;
const H: usize = 120;

fn front_x(g: &Grid2D, burning: CellType, burned: CellType) -> f64 {
    let mut sum = 0.0;
    for y in 0..H {
        let mut fx = 0usize;
        for x in 0..W {
            let t = g.cell_type(y * W + x);
            if t == burning || t == burned {
                fx = x;
            }
        }
        sum += fx as f64;
    }
    sum / H as f64
}

fn speed(p0: f64, dur: u32, wind: f64, seed: u64) -> (f64, f64) {
    let forest = CellType::new("Forest");
    let b = CellType::new("Burning");
    let mut cells = vec![forest; W * H];
    for y in 0..H {
        for x in 0..2 {
            cells[y * W + x] = b;
        }
    }
    let params = WildfireParams {
        seed,
        p0,
        fuels: vec![FuelClass { name: "Forest".into(), veg_factor: 1.0 }],
        wind_speed: wind,
        wind_from_deg: 270.0,
        c1: 0.045,
        c2: 0.131,
        slope_a: 0.078,
        cell_size: 30.0,
        burn_duration: dur,
        spotting: None,
        burning_name: None,
        burned_name: None,
    };
    let mut g = Grid2D::new(W, H, 0, cells, Rule2D { subrules: vec![] });
    g.attach_model(Box::new(WildfireModel::new(params, WildfireEnv::default())))
        .expect("attach");
    let burned = CellType::new("BurnedOut");
    // Skip a warm-up, then fit x(t) until the front nears the far edge.
    let mut xs = Vec::new();
    let mut ts = Vec::new();
    let mut t = 0u64;
    let max_steps = 20_000u64;
    let mut died = false;
    while t < max_steps {
        g.step();
        t += 1;
        let fx = front_x(&g, b, burned);
        if t >= 20 {
            xs.push(fx);
            ts.push(t as f64);
        }
        if fx > (W - 10) as f64 {
            break;
        }
        // Dead fire: nothing burning any more.
        if t.is_multiple_of(50) && (0..W * H).all(|i| g.cell_type(i) != b) {
            died = true;
            break;
        }
    }
    if xs.len() < 5 || died {
        return (0.0, t as f64);
    }
    let n = xs.len() as f64;
    let mt = ts.iter().sum::<f64>() / n;
    let mx = xs.iter().sum::<f64>() / n;
    let cov: f64 = ts.iter().zip(&xs).map(|(a, b)| (a - mt) * (b - mx)).sum();
    let var: f64 = ts.iter().map(|a| (a - mt) * (a - mt)).sum();
    (cov / var, t as f64)
}

fn main() {
    let p0s = [0.05, 0.08, 0.10, 0.12, 0.16, 0.22, 0.30, 0.44, 0.58];
    let durs = [5u32, 10];
    let winds = [0.0, 2.0, 5.0, 8.0];
    println!("[");
    let mut first = true;
    for &dur in &durs {
        for &wind in &winds {
            for &p0 in &p0s {
                let mut v = Vec::new();
                for seed in 0..3u64 {
                    let (s, _) = speed(p0, dur, wind, seed);
                    v.push(s);
                }
                let mean = v.iter().sum::<f64>() / v.len() as f64;
                if !first {
                    println!(",");
                }
                first = false;
                print!(
                    "{{\"p0\":{p0},\"dur\":{dur},\"wind_ms\":{wind},\"cells_per_step\":{mean:.4},\"m_per_step\":{:.2},\"seeds\":{:?}}}",
                    mean * 30.0,
                    v
                );
                eprintln!("p0 {p0:.2} dur {dur:2} wind {wind:.0}: {mean:.3} cells/step = {:.1} m/step", mean * 30.0);
            }
        }
    }
    println!("\n]");
}
