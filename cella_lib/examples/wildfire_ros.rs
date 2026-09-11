//! Measure the model's own rate of spread so one tick can be given a real
//! duration (validation experiment E19), and the E30a arrival-time kernel
//! measurements on the flat grid and on a point ignition.
//!
//! The scenario format declares `steps_per_hour` (50 ticks/day for the
//! six-fire pack, copied from the papers) but nothing ties a tick to a
//! physical time. In a CA the front advances at some fraction of one cell
//! per tick that depends on p0, wind, and burn duration; multiplied by the
//! cell size, that is a rate of spread in metres per tick. Matching it to
//! an observed or Rothermel rate of spread fixes the tick length.
//!
//! Set-up (`speed` mode, the default): homogeneous fuel (veg_factor 1,
//! density 1, flat), a full-height burning column at x = 0..2 so the front
//! is a straight line, wind blowing toward +x (from 270°) or calm. The
//! front position is the mean x of the rightmost burning/burned cell per
//! row; speed is the slope of a linear fit over the steady part of the run.
//! Output: one JSON array on stdout.
//!
//! Usage (from cella_lib/): `cargo run --release --example wildfire_ros -- <mode>`
//!
//! Modes:
//! - `speed` (default, no argument needed): the original E19 table, unchanged.
//! - `arrival_flat`: the E30a flat-grid speed table, both spread rules,
//!   wind 0/2/5/8 m/s, p0 0.12/0.22/0.44, burn duration 5/10, 3 seeds.
//! - `illuminate`: point ignition on a 400x400 uniform grid, wind toward +x
//!   at 0/2/5/8 m/s (the flat-grid table's own four winds), elongation
//!   (E12's measure) at 2/5/10/20 % burned, both rules, 3 seeds.
//! - `lb`: length-to-breadth table on the arrival rule at 10 % size, for
//!   2/5/8 m/s, scanning `c2` under the exponential wind law and also under
//!   the rear-focus law, against Anderson (1983)'s `LB(U)`. Also prints the
//!   head:back ratio at 0.6 m/s for both laws (a closed-form number, not a
//!   simulation).

use cella_lib::wildfire::{FuelClass, WildfireEnv, WildfireModel, WildfireParams, anderson_lb};
use cella_lib::{CellType, Grid2D, Rule2D};
use std::env;

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

/// Base flat-grid params shared by every mode; callers override the fields
/// that vary (`spread`, `wind_law`, `c2`, `wind_speed`, `p0`, `burn_duration`,
/// `seed`).
fn base_params(seed: u64) -> WildfireParams {
    WildfireParams {
        seed,
        p0: 0.58,
        fuels: vec![FuelClass {
            name: "Forest".into(),
            veg_factor: 1.0,
        }],
        wind_speed: 0.0,
        wind_from_deg: 270.0, // west wind: blows toward +x
        c1: 0.045,
        c2: 0.131,
        slope_a: 0.078,
        cell_size: 30.0,
        burn_duration: 1,
        spotting: None,
        burning_name: None,
        burned_name: None,
        spread: "bernoulli".into(),
        arrival_jitter: 0.2,
        wind_law: "exponential".into(),
    }
}

fn speed(p0: f64, dur: u32, wind: f64, seed: u64, spread: &str, wind_law: &str) -> (f64, f64) {
    let forest = CellType::new("Forest");
    let b = CellType::new("Burning");
    let mut cells = vec![forest; W * H];
    for y in 0..H {
        for x in 0..2 {
            cells[y * W + x] = b;
        }
    }
    let mut params = base_params(seed);
    params.p0 = p0;
    params.wind_speed = wind;
    params.burn_duration = dur;
    params.spread = spread.into();
    params.wind_law = wind_law.into();
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

/// E12's elongation measure (√(λ1/λ2) of the second-moment matrix of the
/// tracked cells, unit-square-corrected, clamped to [1, 10]), reimplemented
/// here rather than pulled in from `cella_lib::explore::metrics` so this
/// example stays a plain consumer of the public `Grid2D`/`WildfireModel` API
/// — the exact formula this mirrors is documented and tested in
/// `cella_lib::explore::metrics::elongation`.
fn elongation(cells: &[CellType], w: usize, _h: usize, tracked: impl Fn(CellType) -> bool) -> f64 {
    let (mut n, mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
    for (i, &c) in cells.iter().enumerate() {
        if tracked(c) {
            let (x, y) = ((i % w) as f64, (i / w) as f64);
            n += 1.0;
            sx += x;
            sy += y;
            sxx += x * x;
            syy += y * y;
            sxy += x * y;
        }
    }
    if n < 2.0 {
        return 1.0;
    }
    let (mx, my) = (sx / n, sy / n);
    let cxx = sxx / n - mx * mx + 1.0 / 12.0;
    let cyy = syy / n - my * my + 1.0 / 12.0;
    let cxy = sxy / n - mx * my;
    let tr = cxx + cyy;
    let det: f64 = (cxx * cyy - cxy * cxy).max(0.0);
    let disc: f64 = (tr * tr / 4.0 - det).max(0.0).sqrt();
    let (l1, l2) = (tr / 2.0 + disc, (tr / 2.0 - disc).max(1e-12));
    (l1 / l2).sqrt().clamp(1.0, 10.0)
}

/// One point-ignition run: a `size`x`size` uniform grid, a 3x3 ignition at
/// the centre, wind `wind_speed` toward +x. Steps until the burned fraction
/// (Burning + BurnedOut share of all cells) crosses each of `checkpoints`
/// (fractions in (0, 1], ascending), recording `(fraction, steps, elongation)`
/// at each crossing. Stops early (and returns whatever was recorded) if the
/// fire dies out or `max_steps` is reached.
#[allow(clippy::too_many_arguments)]
fn illuminate(
    size: usize,
    wind_speed: f64,
    p0: f64,
    burn_duration: u32,
    seed: u64,
    spread: &str,
    wind_law: &str,
    c2: f64,
    checkpoints: &[f64],
    max_steps: u64,
) -> Vec<(f64, u64, f64)> {
    let forest = CellType::new("Forest");
    let b = CellType::new("Burning");
    let burned = CellType::new("BurnedOut");
    let mut cells = vec![forest; size * size];
    let (cx, cy) = (size / 2, size / 2);
    for dy in 0..3usize {
        for dx in 0..3usize {
            let (x, y) = (cx + dx - 1, cy + dy - 1);
            cells[y * size + x] = b;
        }
    }
    let mut params = base_params(seed);
    params.p0 = p0;
    params.wind_speed = wind_speed;
    params.wind_from_deg = 270.0; // west wind: blows toward +x
    params.burn_duration = burn_duration;
    params.spread = spread.into();
    params.wind_law = wind_law.into();
    params.c2 = c2;
    let mut g = Grid2D::new(size, size, 0, cells, Rule2D { subrules: vec![] });
    g.attach_model(Box::new(WildfireModel::new(params, WildfireEnv::default())))
        .expect("attach");
    let total = (size * size) as f64;
    let mut next_checkpoint = 0usize;
    let mut out = Vec::new();
    let mut t = 0u64;
    while t < max_steps && next_checkpoint < checkpoints.len() {
        g.step();
        t += 1;
        let cells_now = g.cells();
        let burned_n = cells_now.iter().filter(|&&c| c == b || c == burned).count() as f64;
        let frac = burned_n / total;
        if frac >= checkpoints[next_checkpoint] {
            let e = elongation(cells_now, size, size, |c| c == b || c == burned);
            out.push((checkpoints[next_checkpoint], t, e));
            next_checkpoint += 1;
        }
        if t.is_multiple_of(200) && !cells_now.contains(&b) {
            if env::var("WF_DEBUG").is_ok() {
                eprintln!("  [debug] died at t={t}, frac={frac:.5}, next_checkpoint={next_checkpoint}");
            }
            break; // fire died before reaching every checkpoint.
        }
    }
    if env::var("WF_DEBUG").is_ok() && next_checkpoint < checkpoints.len() {
        let frac = g.cells().iter().filter(|&&c| c == b || c == burned).count() as f64 / total;
        eprintln!(
            "  [debug] stopped at t={t}/{max_steps}, frac={frac:.5}, next_checkpoint={next_checkpoint}, still_burning={}",
            g.cells().contains(&b)
        );
    }
    out
}

fn run_speed_table() {
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
                    let (s, _) = speed(p0, dur, wind, seed, "bernoulli", "exponential");
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

/// E30a measurement 1: the E19 flat-grid speed table for both spread rules,
/// on the combinations the task brief pre-registered.
/// Opening line for a metadata-wrapped report: `{"binary_git": ...,
/// "binary_built_utc": ..., "results": [`. `CELLA_GIT_SHA`/`CELLA_BUILT_UTC`
/// come from `cella_lib/build.rs` (the same fields `wildfire_smc`'s reports
/// carry), so a results file can always be traced back to the exact commit
/// and build that produced it.
fn print_report_open() {
    println!(
        "{{\"binary_git\":{:?},\"binary_built_utc\":{:?},\"results\":[",
        env!("CELLA_GIT_SHA"),
        env!("CELLA_BUILT_UTC")
    );
}

fn print_report_close() {
    println!("\n]}}");
}

fn run_arrival_flat() {
    let p0s = [0.12, 0.22, 0.44];
    let durs = [5u32, 10];
    let winds = [0.0, 2.0, 5.0, 8.0];
    let rules = ["bernoulli", "arrival"];
    print_report_open();
    let mut first = true;
    for &spread in &rules {
        for &dur in &durs {
            for &wind in &winds {
                for &p0 in &p0s {
                    let mut v = Vec::new();
                    for seed in 0..3u64 {
                        let (s, _) = speed(p0, dur, wind, seed, spread, "exponential");
                        v.push(s);
                    }
                    let mean = v.iter().sum::<f64>() / v.len() as f64;
                    if !first {
                        println!(",");
                    }
                    first = false;
                    print!(
                        "{{\"spread\":\"{spread}\",\"p0\":{p0},\"dur\":{dur},\"wind_ms\":{wind},\"cells_per_step\":{mean:.4},\"m_per_step\":{:.2},\"seeds\":{:?}}}",
                        mean * 30.0,
                        v
                    );
                    eprintln!(
                        "{spread:9} p0 {p0:.2} dur {dur:2} wind {wind:.0}: {mean:.3} cells/step = {:.1} m/step",
                        mean * 30.0
                    );
                }
            }
        }
    }
    print_report_close();
}

/// E30a measurement 2: point-ignition elongation vs. size, both rules.
fn run_illuminate() {
    let size = env::var("WF_ILLUMINATE_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(400usize);
    let seeds: u64 = env::var("WF_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3u64);
    let checkpoints = [0.02, 0.05, 0.10, 0.20];
    // p0 = 0.44 (the top of this same task's pre-registered flat-grid trio),
    // burn_duration = 5, chosen (before looking at any illumination result)
    // so a lone downwind neighbor self-sustains under the arrival rule with
    // a comfortable margin: p0 * dir_downwind(8 m/s) * burn_duration = 0.44
    // * 1.43 * 5 ~= 3.15, well clear of 1. Lower values in the same trio
    // (0.12, 0.22) sit close enough to the margin = 1 boundary that the
    // default arrival_jitter (sigma 0.2) can push an individual cell below
    // it, permanently starving that path (heat stops accumulating once its
    // only supporting neighbor has burned out) — confirmed with WF_DEBUG=1.
    let p0: f64 = env::var("WF_P0").ok().and_then(|s| s.parse().ok()).unwrap_or(0.44);
    let burn_duration: u32 = env::var("WF_BURN_DUR")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    // Same four winds as the flat-grid speed table, so the figure (elongation
    // vs. size, both rules, four winds) can be built straight from this mode.
    // WF_WIND_LAW overrides the law for a supplementary check (the addendum's
    // "under Bernoulli, rear_focus still collapses with size" claim) without
    // touching the pre-registered exponential-law default.
    let wind_law = env::var("WF_WIND_LAW").unwrap_or_else(|_| "exponential".to_string());
    let winds = [0.0, 2.0, 5.0, 8.0];
    let max_steps = 20_000u64;
    print_report_open();
    let mut first = true;
    for &wind_speed in &winds {
        for &spread in &["bernoulli", "arrival"] {
            for seed in 0..seeds {
                let hits = illuminate(
                    size,
                    wind_speed,
                    p0,
                    burn_duration,
                    seed,
                    spread,
                    &wind_law,
                    0.131,
                    &checkpoints,
                    max_steps,
                );
                for (frac, steps, e) in hits {
                    if !first {
                        println!(",");
                    }
                    first = false;
                    print!(
                        "{{\"spread\":\"{spread}\",\"wind_ms\":{wind_speed},\"seed\":{seed},\"size_frac\":{frac},\"steps\":{steps},\"elongation\":{e:.4}}}"
                    );
                    eprintln!(
                        "wind {wind_speed:.0} {spread:9} seed {seed} size {:.0}%: {steps} steps, elongation {e:.3}",
                        frac * 100.0
                    );
                }
            }
        }
    }
    print_report_close();
}

/// E30a measurement 3: length-to-breadth table on the arrival rule at 10 %
/// size, c2 scan under the exponential law plus the rear-focus law, and the
/// closed-form head:back ratio at 0.6 m/s for both laws (no simulation
/// needed for that number: it is `dir[head] / dir[back]` from the wind law
/// directly).
fn run_lb() {
    let size = env::var("WF_ILLUMINATE_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(400usize);
    let seeds: u64 = env::var("WF_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3u64);
    // p0 as in `run_illuminate`. burn_duration is much longer here: at
    // rear_focus / wind 8 the crosswind direction factor is ~0.0146, so a
    // single upstream neighbor only contributes p0 * 0.0146 * burn_duration
    // heat before burning out. At burn_duration = 5 that is ~0.03 — far
    // below 1, so the fire can only ever advance as a one-cell-wide spine
    // (no organic sideways growth), which starves out once it reaches the
    // grid edge, long before 10 % of the domain is burned (confirmed by
    // running it: it dies at <0.1 % burned regardless of grid size, since a
    // fixed-width spine's area *share* of an N x N grid shrinks as N grows).
    // burn_duration = 500 gives that same worst-case direction a margin of
    // 0.44 * 0.0146 * 500 ~= 3.2 (safe against the arrival_jitter default's
    // draw-to-draw spread) so the fire can actually thicken into a 2-D shape
    // whose length-to-breadth ratio is measurable at all.
    let p0 = 0.44;
    let burn_duration: u32 = env::var("WF_BURN_DUR")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);
    let winds = [2.0, 5.0, 8.0];
    let c2s = [0.131, 0.2, 0.3, 0.45];
    let checkpoints = [0.10];
    // A strongly wind-anisotropic setting (high c2, high wind, or
    // rear_focus) can grow a needle-thin fire that runs to the grid edge
    // before it has thickened enough to cover 10 % of the domain; a larger
    // step budget than `illuminate` gives the slow lateral direction more
    // chance to catch up before giving up on that cell.
    let max_steps = 100_000u64;

    // Closed-form head:back at 0.6 m/s, both laws, default c2 for the
    // exponential law (Alexandridis c2 = 0.131).
    let v = 0.6f64;
    let exp_head_back = (2.0 * 0.131 * v).exp();
    let a = anderson_lb(v);
    let c = (a * a - 1.0).max(0.0).sqrt();
    let rf_head_back = (a + c).powi(2);
    eprintln!(
        "head:back at {v} m/s: exponential (c2=0.131) = {exp_head_back:.3}, rear_focus = {rf_head_back:.3} (LB={a:.3})"
    );

    // JSON has no NaN, so a checkpoint a seed never reached (the needle-thin
    // failure mode described above) is `null`, not a number.
    let json_opt = |v: Option<f64>| match v {
        Some(x) => format!("{x:.4}"),
        None => "null".to_string(),
    };

    print_report_open();
    let mut first = true;
    let mut emit = |law: &str, c2: f64, wind: f64, lbs: &[Option<f64>]| {
        if !first {
            println!(",");
        }
        first = false;
        let anderson = anderson_lb(wind);
        let reached: Vec<f64> = lbs.iter().filter_map(|&v| v).collect();
        let mean = if reached.is_empty() {
            None
        } else {
            Some(reached.iter().sum::<f64>() / reached.len() as f64)
        };
        let seeds_json: Vec<String> = lbs.iter().map(|&v| json_opt(v)).collect();
        print!(
            "{{\"wind_law\":\"{law}\",\"c2\":{c2},\"wind_ms\":{wind},\"lb_mean\":{},\"lb_seeds\":[{}],\"reached_seeds\":{},\"total_seeds\":{},\"anderson_lb\":{anderson:.4},\"head_back_0_6ms\":{}}}",
            json_opt(mean),
            seeds_json.join(","),
            reached.len(),
            lbs.len(),
            if law == "rear_focus" { rf_head_back } else { exp_head_back }
        );
        eprintln!(
            "{law:12} c2={c2:.3} wind={wind:.0}: LB={} ({}/{} seeds reached 10%; Anderson {anderson:.3})",
            mean.map(|m| format!("{m:.3}")).unwrap_or_else(|| "n/a".into()),
            reached.len(),
            lbs.len(),
        );
    };

    for &wind in &winds {
        for &c2 in &c2s {
            let mut lbs = Vec::new();
            for seed in 0..seeds {
                let hits = illuminate(
                    size,
                    wind,
                    p0,
                    burn_duration,
                    seed,
                    "arrival",
                    "exponential",
                    c2,
                    &checkpoints,
                    max_steps,
                );
                lbs.push(hits.first().map(|&(_, _, e)| e));
            }
            emit("exponential", c2, wind, &lbs);
        }
        // Rear-focus: c2 is unused by that law; report once per wind.
        let mut lbs = Vec::new();
        for seed in 0..seeds {
            let hits = illuminate(
                size,
                wind,
                p0,
                burn_duration,
                seed,
                "arrival",
                "rear_focus",
                0.131,
                &checkpoints,
                max_steps,
            );
            lbs.push(hits.first().map(|&(_, _, e)| e));
        }
        emit("rear_focus", 0.0, wind, &lbs);
    }
    print_report_close();
}

fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| "speed".to_string());
    match mode.as_str() {
        "arrival_flat" => run_arrival_flat(),
        "illuminate" => run_illuminate(),
        "lb" => run_lb(),
        _ => run_speed_table(),
    }
}
