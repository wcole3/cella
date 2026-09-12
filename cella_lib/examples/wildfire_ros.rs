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
//! - `illuminate`: point ignition, both rules, both wind laws. Fix round 2:
//!   the ignition sits *upwind* on an elongated 900x300 grid (see
//!   `WIDTH`/`HEIGHT`/`IGNITE_X` below) instead of centred on a square one,
//!   after a centred 400x400 grid was found to let the fire's head hit the
//!   domain boundary at almost exactly the old 10 % checkpoint (see the
//!   module doc's own arithmetic note above `illuminate`) — every
//!   checkpoint reads cells burned as an absolute count (2,000 / 5,000 /
//!   10,000 / 20,000), not a fraction of the grid, and carries a
//!   `boundary_contact` flag.
//! - `lb`: length-to-breadth table on the arrival rule at the 10,000- and
//!   20,000-cell checkpoints (same domain), scanning `c2` under the
//!   exponential wind law and also under the rear-focus law, against
//!   Anderson (1983)'s `LB(U)`, at jitter 0.2 (3 seeds) and jitter 0 (one
//!   deterministic reading). Also prints the head:back ratio at 0.6 m/s for
//!   both laws (a closed-form number, not a simulation).

use cella_lib::wildfire::{FuelClass, WildfireEnv, WildfireModel, WildfireParams, anderson_lb};
use cella_lib::{CellType, Grid2D, Rule2D};
use std::env;

const W: usize = 240;
const H: usize = 120;

/// Domain for `illuminate`/`lb` (fix round 2): elongated and wind-aligned,
/// with the ignition placed upwind, so the fire's head has ~860 cells of
/// room before it can ever reach the far edge. At 8 m/s, rear-focus, p0 =
/// 0.12, the head's own cost is `1 / (p0 * exp(c1*v))` ~= 5.8 ticks/cell (see
/// `illuminate`'s doc comment); reaching the last cell before the edge
/// (860 cells away) would take ~5,000 ticks, and by then a length-to-
/// breadth-7 shape covers `pi * 860^2 / 7` ~= 332,000 cells — several times
/// the whole grid's own 270,000 — so no checkpoint used here (up to 20,000
/// cells) can plausibly reach that edge. The old centred 400x400 domain put
/// the boundary only 200 cells from the ignition in every direction, which
/// a length-to-breadth-7 head reached at almost exactly the 10 % checkpoint
/// -- an artifact of that domain's small size, not of the spread rule.
const WIDTH: usize = 900;
const HEIGHT: usize = 300;
const IGNITE_X: usize = 40;

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
/// tracked cells, unit-square-corrected), reimplemented here rather than
/// pulled in from `cella_lib::explore::metrics` so this example stays a
/// plain consumer of the public `Grid2D`/`WildfireModel` API — the exact
/// second-moment formula this mirrors is documented and tested in
/// `cella_lib::explore::metrics::elongation`. That shared function clamps
/// its result to `[1, 10]` (`MAX_ELONGATION`), which is the right ceiling
/// for the shapes E12/E37 look at; this copy clamps to `[1, 50]` instead
/// (fix round 2), because rear-focus at 8 m/s on the upwind-ignition domain
/// below genuinely exceeds 10 (a near-1-D spine can have arbitrarily large
/// second-moment elongation) — reporting a saturated 10.0 for every
/// checkpoint would look flat for the wrong reason. 50 is generous headroom
/// for a length-to-breadth around Anderson's own ceiling of 8 without
/// blowing up on the smallest checkpoints tested (2,000 cells is already
/// far past the few-cell regime where a straight line reads as
/// near-infinite).
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
    (l1 / l2).sqrt().clamp(1.0, 50.0)
}

/// True if any cell in `cells` matching `tracked` sits within 2 cells of any
/// edge of a `w`x`h` grid. Fix round 2: a checkpoint reached while the fire's
/// own extent is touching the boundary is not a free measurement of the
/// spread rule's shape — the boundary itself starts shaping it — so every
/// checkpoint carries this flag and a caller can discard/flag contaminated
/// rows instead of silently reporting them as if the domain were infinite.
fn boundary_contact(cells: &[CellType], w: usize, h: usize, tracked: impl Fn(CellType) -> bool) -> bool {
    const MARGIN: usize = 2;
    for (i, &c) in cells.iter().enumerate() {
        if !tracked(c) {
            continue;
        }
        let (x, y) = (i % w, i / w);
        if x < MARGIN || x + MARGIN >= w || y < MARGIN || y + MARGIN >= h {
            return true;
        }
    }
    false
}

/// One point-ignition run: a `width`x`height` uniform grid, a 3x3 ignition
/// at `(ignite_x, height / 2)` (upwind, not centred — see `WIDTH`/`HEIGHT`/
/// `IGNITE_X`'s doc comment), wind `wind_speed` toward +x. Steps until the
/// burned cell COUNT (Burning + BurnedOut) crosses each of `checkpoints`
/// (absolute cell counts, ascending), recording `(count, steps, elongation,
/// front_x, boundary_contact)` at each crossing, where `front_x` is the
/// largest x-coordinate among burned/burning cells (so
/// `front_x / steps` is a direct, no-fitting estimate of the head's own
/// speed in cells/tick, comparable to the closed form `1 / cost_head` =
/// `p_base * dir[head]` = `p0 * exp(c1 * wind_speed)` — identical under
/// either wind law, since both give the head direction factor `exp(c1*v)`
/// exactly). Stops early (and returns whatever was recorded) if the fire
/// dies out or `max_steps` is reached.
#[allow(clippy::too_many_arguments)]
fn illuminate(
    width: usize,
    height: usize,
    ignite_x: usize,
    wind_speed: f64,
    p0: f64,
    burn_duration: u32,
    seed: u64,
    spread: &str,
    wind_law: &str,
    c2: f64,
    arrival_jitter: f64,
    checkpoints: &[usize],
    max_steps: u64,
) -> Vec<(usize, u64, f64, usize, bool)> {
    let forest = CellType::new("Forest");
    let b = CellType::new("Burning");
    let burned = CellType::new("BurnedOut");
    let mut cells = vec![forest; width * height];
    let cy = height / 2;
    for dy in 0..3usize {
        for dx in 0..3usize {
            let (x, y) = (ignite_x + dx - 1, cy + dy - 1);
            cells[y * width + x] = b;
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
    params.arrival_jitter = arrival_jitter;
    let mut g = Grid2D::new(width, height, 0, cells, Rule2D { subrules: vec![] });
    g.attach_model(Box::new(WildfireModel::new(params, WildfireEnv::default())))
        .expect("attach");
    let mut next_checkpoint = 0usize;
    let mut out = Vec::new();
    let mut t = 0u64;
    while t < max_steps && next_checkpoint < checkpoints.len() {
        g.step();
        t += 1;
        let cells_now = g.cells();
        let is_fire = |c: &CellType| *c == b || *c == burned;
        let n = cells_now.iter().filter(|c| is_fire(c)).count();
        if n >= checkpoints[next_checkpoint] {
            let e = elongation(cells_now, width, height, |c| c == b || c == burned);
            let front_x = (0..cells_now.len())
                .filter(|&i| is_fire(&cells_now[i]))
                .map(|i| i % width)
                .max()
                .unwrap_or(ignite_x);
            let contact = boundary_contact(cells_now, width, height, |c| c == b || c == burned);
            out.push((checkpoints[next_checkpoint], t, e, front_x, contact));
            next_checkpoint += 1;
        }
        if t.is_multiple_of(200) && !cells_now.contains(&b) {
            if env::var("WF_DEBUG").is_ok() {
                eprintln!("  [debug] died at t={t}, n={n}, next_checkpoint={next_checkpoint}");
            }
            break; // fire died before reaching every checkpoint.
        }
    }
    if env::var("WF_DEBUG").is_ok() && next_checkpoint < checkpoints.len() {
        let n = g.cells().iter().filter(|&&c| c == b || c == burned).count();
        eprintln!(
            "  [debug] stopped at t={t}/{max_steps}, n={n}, next_checkpoint={next_checkpoint}, still_burning={}",
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

/// E30a measurement 2 (fix round 2): point-ignition elongation vs. size, on
/// the upwind-ignition/elongated domain, both rules, both wind laws.
fn run_illuminate() {
    let seeds: u64 = env::var("WF_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(3u64);
    let checkpoints = [2000usize, 5000, 10000, 20000];
    let burn_duration: u32 = env::var("WF_BURN_DUR").ok().and_then(|s| s.parse().ok()).unwrap_or(5);
    let p0s: Vec<f64> = env::var("WF_P0")
        .ok()
        .map(|s| vec![s.parse().expect("WF_P0")])
        .unwrap_or_else(|| vec![0.12, 0.22]);
    let winds = [0.0, 2.0, 5.0, 8.0];
    let laws = ["exponential", "rear_focus"];
    let max_steps: u64 = env::var("WF_MAX_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(20_000);
    print_report_open();
    let mut first = true;
    let mut emit = |p0: f64, wind_speed: f64, spread: &str, law: &str, seed: Option<u64>, jitter: f64, hits: &[(usize, u64, f64, usize, bool)]| {
        for &(n, steps, e, front_x, contact) in hits {
            if !first {
                println!(",");
            }
            first = false;
            let seed_json = seed.map(|s| s.to_string()).unwrap_or_else(|| "null".to_string());
            print!(
                "{{\"p0\":{p0},\"spread\":\"{spread}\",\"wind_law\":\"{law}\",\"wind_ms\":{wind_speed},\"seed\":{seed_json},\"jitter\":{jitter},\"cells\":{n},\"steps\":{steps},\"elongation\":{e:.4},\"front_x\":{front_x},\"boundary_contact\":{contact}}}"
            );
            eprintln!(
                "p0={p0} wind={wind_speed:.0} {spread:9} {law:11} seed={seed_json} jit={jitter} n={n:5}: {steps:5} steps, elongation {e:.3}, front_x {front_x}, boundary {contact}"
            );
        }
    };
    for &p0 in &p0s {
        for &wind_speed in &winds {
            for &spread in &["bernoulli", "arrival"] {
                for &law in &laws {
                    for seed in 0..seeds {
                        let hits = illuminate(
                            WIDTH, HEIGHT, IGNITE_X, wind_speed, p0, burn_duration, seed, spread, law, 0.131, 0.2,
                            &checkpoints, max_steps,
                        );
                        emit(p0, wind_speed, spread, law, Some(seed), 0.2, &hits);
                    }
                }
            }
            // One deterministic jitter-0 reading per law, at every wind, for
            // both rules — cheap (arrival) or free (Bernoulli has no jitter
            // to silence, so this is just seed 0 again, kept for a uniform
            // table shape).
            for &spread in &["bernoulli", "arrival"] {
                for &law in &laws {
                    let hits = illuminate(
                        WIDTH, HEIGHT, IGNITE_X, wind_speed, p0, burn_duration, 0, spread, law, 0.131, 0.0,
                        &checkpoints, max_steps,
                    );
                    emit(p0, wind_speed, spread, law, None, 0.0, &hits);
                }
            }
        }
    }
    print_report_close();
}

/// E30a measurement 3 (fix round 2): length-to-breadth table on the arrival
/// rule at the 10,000- and 20,000-cell checkpoints on the same upwind-
/// ignition domain, c2 scan under the exponential law plus the rear-focus
/// law, and the closed-form head:back ratio at 0.6 m/s for both laws (no
/// simulation needed for that number: it is `dir[head] / dir[back]` from the
/// wind law directly).
fn run_lb() {
    let seeds: u64 = env::var("WF_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(3u64);
    let burn_duration: u32 = env::var("WF_BURN_DUR").ok().and_then(|s| s.parse().ok()).unwrap_or(5);
    let p0s: Vec<f64> = env::var("WF_P0")
        .ok()
        .map(|s| vec![s.parse().expect("WF_P0")])
        .unwrap_or_else(|| vec![0.12, 0.22]);
    let winds = [2.0, 5.0, 8.0];
    let c2s = [0.131, 0.2, 0.3, 0.45];
    let checkpoints = [10_000usize, 20_000];
    let max_steps: u64 = env::var("WF_MAX_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(20_000);

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

    let json_opt = |v: Option<f64>| match v {
        Some(x) => format!("{x:.4}"),
        None => "null".to_string(),
    };

    print_report_open();
    let mut first = true;
    let mut emit = |p0: f64,
                    law: &str,
                    c2: f64,
                    wind: f64,
                    cells_target: usize,
                    lbs: &[Option<(f64, usize, bool)>],
                    jitter0: Option<(f64, usize, bool)>,
                    template_lb: Option<f64>| {
        if !first {
            println!(",");
        }
        first = false;
        let anderson = anderson_lb(wind);
        let reached: Vec<(f64, usize, bool)> = lbs.iter().filter_map(|&v| v).collect();
        let mean = if reached.is_empty() {
            None
        } else {
            Some(reached.iter().map(|&(e, _, _)| e).sum::<f64>() / reached.len() as f64)
        };
        let any_contact = reached.iter().any(|&(_, _, c)| c) || jitter0.is_some_and(|(_, _, c)| c);
        print!(
            "{{\"p0\":{p0},\"wind_law\":\"{law}\",\"c2\":{c2},\"wind_ms\":{wind},\"cells\":{cells_target},\"lb_mean\":{},\"lb_jitter0\":{},\"boundary_contact\":{any_contact},\"reached_seeds\":{},\"total_seeds\":{},\"anderson_lb\":{anderson:.4},\"head_back_0_6ms\":{},\"template_lb\":{}}}",
            json_opt(mean),
            json_opt(jitter0.map(|(e, _, _)| e)),
            reached.len(),
            lbs.len(),
            if law == "rear_focus" { rf_head_back } else { exp_head_back },
            json_opt(template_lb)
        );
        eprintln!(
            "p0={p0} {law:12} c2={c2:.3} wind={wind:.0} n={cells_target:6}: LB={} jitter0={} contact={any_contact} ({}/{} seeds; Anderson {anderson:.3})",
            mean.map(|m| format!("{m:.3}")).unwrap_or_else(|| "n/a".into()),
            jitter0.map(|(e, _, _)| format!("{e:.3}")).unwrap_or_else(|| "n/a".into()),
            reached.len(),
            lbs.len(),
        );
    };

    for &p0 in &p0s {
        for &wind in &winds {
            let mut law_configs: Vec<(&str, f64)> = c2s.iter().map(|&c2| ("exponential", c2)).collect();
            law_configs.push(("rear_focus", 0.0));
            for (law, c2) in law_configs {
                // 3-seed default-jitter pass and 1 deterministic jitter-0
                // pass, each producing both checkpoints (10k, 20k cells) in
                // one run.
                let mut per_checkpoint: Vec<Vec<Option<(f64, usize, bool)>>> =
                    vec![Vec::new(); checkpoints.len()];
                for seed in 0..seeds {
                    let hits = illuminate(
                        WIDTH, HEIGHT, IGNITE_X, wind, p0, burn_duration, seed, "arrival", law, c2, 0.2,
                        &checkpoints, max_steps,
                    );
                    for (i, _) in checkpoints.iter().enumerate() {
                        let v = hits
                            .iter()
                            .find(|&&(n, _, _, _, _)| n == checkpoints[i])
                            .map(|&(_, _, e, fx, c)| (e, fx, c));
                        per_checkpoint[i].push(v);
                    }
                }
                let jitter0_hits = illuminate(
                    WIDTH, HEIGHT, IGNITE_X, wind, p0, burn_duration, 0, "arrival", law, c2, 0.0,
                    &checkpoints, max_steps,
                );
                for (i, &cp) in checkpoints.iter().enumerate() {
                    let j0 = jitter0_hits
                        .iter()
                        .find(|&&(n, _, _, _, _)| n == cp)
                        .map(|&(_, _, e, fx, c)| (e, fx, c));
                    emit(p0, law, c2, wind, cp, &per_checkpoint[i], j0, None);
                }
            }
        }
    }
    // Fix round 3: two extra rear-focus template points at jitter 0, deliberately
    // solved for round Anderson LB targets rather than round wind speeds, so the
    // LB table reads template-LB 1.2 / 1.5 / 2.0 / 3.2 / 7.0 -> measured (the
    // 1.5/3.2/7.0 points already exist at wind 2/5/8 above). Controller fix round
    // 3 estimated ~0.85 and ~2.9 m/s for LB 1.2 and 2.0; solving anderson_lb(v) =
    // target with bisection gives 0.9690 and 3.1771 m/s -- used here instead, so
    // the reported "template LB" is exact, not approximate.
    let p0 = p0s[0];
    for &(target_lb, wind) in &[(1.2f64, 0.9690f64), (2.0, 3.1771)] {
        let jitter0_hits = illuminate(
            WIDTH, HEIGHT, IGNITE_X, wind, p0, burn_duration, 0, "arrival", "rear_focus", 0.0, 0.0,
            &checkpoints, max_steps,
        );
        for &cp in &checkpoints {
            let j0 = jitter0_hits
                .iter()
                .find(|&&(n, _, _, _, _)| n == cp)
                .map(|&(_, _, e, fx, c)| (e, fx, c));
            emit(p0, "rear_focus", 0.0, wind, cp, &[], j0, Some(target_lb));
            eprintln!(
                "  [template LB {target_lb}] wind={wind:.4} n={cp}: {}",
                j0.map(|(e, _, c)| format!("measured {e:.4}{}", if c { " (boundary contact)" } else { "" }))
                    .unwrap_or_else(|| "did not reach checkpoint".into())
            );
        }
    }
    print_report_close();
}

/// Fix round 2: measured head speed (cells/tick along +x, from `front_x`)
/// against the closed form `p0 * exp(c1 * wind_speed)` (identical under
/// either wind law), one row per wind, arrival rule, jitter 0 (deterministic,
/// isolates the rate from per-cell noise).
fn run_head_speed() {
    let p0: f64 = env::var("WF_P0").ok().and_then(|s| s.parse().ok()).unwrap_or(0.12);
    let burn_duration: u32 = env::var("WF_BURN_DUR").ok().and_then(|s| s.parse().ok()).unwrap_or(5);
    let winds = [0.0, 2.0, 5.0, 8.0];
    let checkpoints = [5000usize, 20000];
    let max_steps: u64 = env::var("WF_MAX_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(20_000);
    print_report_open();
    let mut first = true;
    for &wind in &winds {
        for &law in &["exponential", "rear_focus"] {
            let hits = illuminate(
                WIDTH, HEIGHT, IGNITE_X, wind, p0, burn_duration, 0, "arrival", law, 0.131, 0.0,
                &checkpoints, max_steps,
            );
            if hits.len() < 2 {
                eprintln!("wind={wind:.0} {law}: did not reach both checkpoints, skipping speed fit");
                continue;
            }
            let (n0, t0, _, x0, c0) = hits[0];
            let (n1, t1, _, x1, c1) = hits[1];
            let measured = (x1 as f64 - x0 as f64) / (t1 as f64 - t0 as f64);
            let closed_form = p0 * (0.045 * wind).exp();
            if !first {
                println!(",");
            }
            first = false;
            print!(
                "{{\"p0\":{p0},\"wind_law\":\"{law}\",\"wind_ms\":{wind},\"measured_cells_per_tick\":{measured:.4},\"closed_form_cells_per_tick\":{closed_form:.4},\"n0\":{n0},\"n1\":{n1},\"boundary_contact\":{}}}",
                c0 || c1
            );
            eprintln!(
                "wind={wind:.0} {law:11}: measured {measured:.4} cells/tick, closed form {closed_form:.4} (ratio {:.3})",
                measured / closed_form
            );
        }
    }
    print_report_close();
}

fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| "speed".to_string());
    match mode.as_str() {
        "arrival_flat" => run_arrival_flat(),
        "illuminate" => run_illuminate(),
        "lb" => run_lb(),
        "head_speed" => run_head_speed(),
        _ => run_speed_table(),
    }
}
