//! The deterministic dummy forecasters: persistence (the ignition/previous
//! mask, unchanged), the Circle (area-matched radial growth from a chamfer
//! distance transform), the wind-oriented Ellipse (E41, both the rear-focus
//! headline variant and the centred post-hoc control), and their *lagged*
//! variants (grown from the mask one window back instead of from the
//! ignition mask — the fair dummy competitors for a state-corrected mode).
//! Shared by the ensemble modes (which score every window against these
//! nulls alongside the ensemble itself) and by `nulls` mode ([`run_nulls`]),
//! which scores nothing but these.

use std::path::{Path, PathBuf};

use cella_lib::explore::metrics::{brier, iou};
use cella_lib::wildfire::wind_toward_grid_deg;
use serde::Deserialize;

use crate::report::{provenance, write_json};
use crate::score::NullObsScore;
use crate::{Scenario, Truth, load, mask_at};

/// Chamfer distance (3-4 mask) from the seed set; basis of the radial null.
pub(crate) fn chamfer_from(seed_mask: &[bool], w: usize, h: usize) -> Vec<u32> {
    const FAR: u32 = u32::MAX / 2;
    let mut d: Vec<u32> = seed_mask.iter().map(|&s| if s { 0 } else { FAR }).collect();
    let idx = |x: usize, y: usize| y * w + x;
    for y in 0..h {
        for x in 0..w {
            let mut best = d[idx(x, y)];
            if x > 0 {
                best = best.min(d[idx(x - 1, y)] + 3);
            }
            if y > 0 {
                best = best.min(d[idx(x, y - 1)] + 3);
                if x > 0 {
                    best = best.min(d[idx(x - 1, y - 1)] + 4);
                }
                if x + 1 < w {
                    best = best.min(d[idx(x + 1, y - 1)] + 4);
                }
            }
            d[idx(x, y)] = best;
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let mut best = d[idx(x, y)];
            if x + 1 < w {
                best = best.min(d[idx(x + 1, y)] + 3);
            }
            if y + 1 < h {
                best = best.min(d[idx(x, y + 1)] + 3);
                if x + 1 < w {
                    best = best.min(d[idx(x + 1, y + 1)] + 4);
                }
                if x > 0 {
                    best = best.min(d[idx(x - 1, y + 1)] + 4);
                }
            }
            d[idx(x, y)] = best;
        }
    }
    d
}

/// Grows `seed_mask` outward by chamfer distance to exactly `target_area`
/// true cells — the Circle null's own construction (`chamfer_from` plus a
/// distance-then-index sort), generalised to any seed instead of only the
/// fixed ignition mask, so it can be re-seeded from a moving observation
/// (the *lagged* Circle: grown from the observed mask at t_{k-1}, matched
/// to the observed area at t_k, the fair dummy competitor for a
/// state-corrected forecast that also only ever sees t_{k-1}).
pub(crate) fn radial_mask_from(seed_mask: &[bool], w: usize, h: usize, target_area: usize) -> Vec<bool> {
    let dist = chamfer_from(seed_mask, w, h);
    let mut order: Vec<usize> = (0..w * h).collect();
    order.sort_by_key(|&i| (dist[i], i));
    let mut out = vec![false; w * h];
    for &i in order.iter().take(target_area) {
        out[i] = true;
    }
    out
}

/// One hourly reading from `station_hourly.json` (E41's `ellipse_station`
/// variant). Extra fields in the file (temperature, humidity, provenance)
/// are ignored — serde only pulls out what this struct names.
#[derive(Deserialize)]
struct StationRow {
    hours: f64,
    from_deg: f64,
    speed_ms: f64,
}

/// The station file's shape: a header we don't need plus the hourly rows.
/// `pub(crate)`: read by `crate::diag`'s per-window diagnostics (E48) as
/// well as [`run_nulls`] below, via [`load_station`].
#[derive(Deserialize)]
pub(crate) struct StationLog {
    rows: Vec<StationRow>,
}

/// `station_hourly.json` for a scenario directory, or `None` if the fire
/// has no station file — the same lookup [`run_nulls`] does
/// (`SMC_STATION_WIND` env override, else `dir.join("station_hourly.json")`),
/// factored out so `crate::diag`'s per-window diagnostics (E48) load it the
/// same way instead of duplicating the env-var override.
pub(crate) fn load_station(dir: &Path) -> Option<StationLog> {
    let station_path = std::env::var("SMC_STATION_WIND")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dir.join("station_hourly.json"));
    station_path.exists().then(|| load(&station_path))
}

/// The vector mean of the station wind over `[start_hours, end_hours)`, as
/// (speed_ms, grid "toward" direction in radians) — the same pair
/// [`grow_ellipse`] wants from the ERA5 schedule.
///
/// "Vector mean" means: turn every hourly reading into a little arrow
/// (length = speed, direction = where it blows toward), add the arrows,
/// then read the length and direction back off the sum. That is different
/// from averaging the speeds and the bearings separately — a wind that is
/// due north for one hour and due south the next averages to *no* wind by
/// vector mean (the arrows cancel), which is physically what happened,
/// while separately-averaged bearings would nonsensically call that
/// "due east or west, who knows". Rotating first (from the compass
/// meteorological bearing to the grid's toward-angle) and averaging after
/// gives the same answer as averaging first and rotating after, because
/// rotation is linear — so we can average directly in grid coordinates.
///
/// Falls back to the single closest row when the window contains none
/// (the campaign's station files are hourly with no gaps, so this should
/// not trigger; it exists so a thinner future log degrades instead of
/// panicking).
pub(crate) fn station_vector_mean(log: &StationLog, start_hours: f64, end_hours: f64) -> (f64, f64) {
    let in_window: Vec<&StationRow> = log
        .rows
        .iter()
        .filter(|r| r.hours >= start_hours - 1e-6 && r.hours < end_hours - 1e-6)
        .collect();
    let rows: Vec<&StationRow> = if in_window.is_empty() {
        let mid = (start_hours + end_hours) / 2.0;
        log.rows
            .iter()
            .min_by(|a, b| {
                (a.hours - mid)
                    .abs()
                    .partial_cmp(&(b.hours - mid).abs())
                    .expect("hours are finite")
            })
            .into_iter()
            .collect()
    } else {
        in_window
    };
    let (mut ux, mut uy) = (0.0, 0.0);
    for r in &rows {
        let toward = wind_toward_grid_deg(r.from_deg).to_radians();
        ux += r.speed_ms * toward.cos();
        uy += r.speed_ms * toward.sin();
    }
    let n = rows.len().max(1) as f64;
    let (mx, my) = (ux / n, uy / n);
    (mx.hypot(my), my.atan2(mx))
}

/// Anderson (1983)'s length-to-breadth ratio of a wind-driven fire ellipse,
/// from the 10 m wind speed `u` in m/s. `LB = 1` is a circle (no wind
/// effect); it grows with speed and is clamped to `[1, 8]` because the raw
/// curve is only fit over the range fires are actually observed at — past
/// that it is extrapolation, not physics.
pub(crate) fn anderson_lb(u: f64) -> f64 {
    let lb = 0.936 * (0.2566 * u).exp() + 0.461 * (-0.1548 * u).exp() - 0.397;
    lb.clamp(1.0, 8.0)
}

/// The travel-time cost of one Dijkstra step `(dx, dy)` (grid cells; the
/// 8-connected offsets, so `1` or `√2`) under a wind blowing *toward*
/// `wind_toward_rad` (0 = +x, turning toward +y — the grid convention
/// [`wind_toward_grid_deg`] produces), with ellipse length-to-breadth
/// ratio `lb`.
///
/// **What an ellipse template is.** A calm fire spreads the same speed in
/// every direction: a circle. A wind-driven fire spreads fastest with the
/// wind, slowest against it, in between on the flanks — an ellipse. Put
/// the ignition point at the ellipse's rear focus (not its centre) and the
/// three textbook rates fall out of two numbers, semi-major axis `a` and
/// semi-minor axis `b` (`b = 1` here; only the *shape* of the ellipse
/// matters, its overall size is decided by the area match, not by `a`/`b`
/// in physical units), with `c = √(a² − b²)` the distance from the centre
/// to a focus:
///
/// - **head rate** (straight downwind, θ = 0): `a + c` — the tip of the
///   ellipse is `a + c` from the focus, the far side of the centre.
/// - **back rate** (straight upwind, θ = 180°): `a − c` — the near tip is
///   only `a − c` from the focus, because the focus sits *inside* that
///   short end, near the fire's own start.
/// - **flank rate** (crosswind, θ = 90°): `b² / a` — the ellipse's
///   half-width as seen from the focus.
///
/// Worked example: wind gives `LB = a = 3` (`b = 1`), so
/// `c = √(9 − 1) ≈ 2.83`. Head rate `a + c ≈ 5.83`, back rate
/// `a − c ≈ 0.17`, flank rate `b²/a ≈ 0.33`: the fire runs about 34× faster
/// downwind than upwind, and about 17× faster downwind than sideways — the
/// long, narrow lobe a real wind-driven fire draws. The general formula
/// used below, `r(θ) = b² / (a − c·cos θ)`, reduces to exactly these three
/// numbers at θ = 0°, 180°, 90° and interpolates between them.
///
/// Growing outward at the *reciprocal* rate — cost, not speed — turns the
/// three rates into three travel times, and Dijkstra over that cost field
/// grows a mask whose boundary is (approximately, on a discrete grid) that
/// same ellipse.
fn ellipse_step_cost(dx: f64, dy: f64, wind_toward_rad: f64, lb: f64) -> f64 {
    let norm = (dx * dx + dy * dy).sqrt();
    let (wx, wy) = (wind_toward_rad.cos(), wind_toward_rad.sin());
    let cos_theta = (dx * wx + dy * wy) / norm;
    let (a, b) = (lb, 1.0f64);
    let c = (a * a - b * b).sqrt();
    let r = b * b / (a - c * cos_theta);
    norm / r
}

/// Post-hoc control for E41 (added after seeing the rear-focus results,
/// not pre-registered — see the TEST_PLAN v1.8 addendum). The rear-focus
/// template above bakes in a front/back skew — `(a + c) / (a − c)` — from
/// LB alone, before any stretch is even visible: at `LB = 1.1` (about
/// Ferguson's peak) that ratio is already ≈ 2.4, just from *which way*
/// round the ellipse is pointed. This control removes that skew: the
/// ignition sits at the ellipse's *centre*, so head and back rates are
/// equal (`a`) and only the flank rate (`1`) differs — `LB` alone can no
/// longer encode "front vs. back", only "long axis vs. short axis". The
/// rate is `r(θ) = a·b / √(b²·cos²θ + a²·sin²θ)` (`a = lb`, `b = 1`):
/// head (θ = 0) and back (θ = 180°) both equal `a`, flank (θ = 90°)
/// equals `b`. Comparing this to [`ellipse_step_cost`]'s result on the
/// same fire separates two different explanations for the same IoU gain:
/// this control winning too means the *stretch* (long vs. short axis)
/// carries signal; only the rear-focus version winning means it was the
/// *sign* (front vs. back) all along, which a symmetric LB template can
/// give away almost for free.
fn centred_ellipse_step_cost(dx: f64, dy: f64, wind_toward_rad: f64, lb: f64) -> f64 {
    let norm = (dx * dx + dy * dy).sqrt();
    let (wx, wy) = (wind_toward_rad.cos(), wind_toward_rad.sin());
    let cos_theta = (dx * wx + dy * wy) / norm;
    let (a, b) = (lb, 1.0f64);
    let r = a * b / (b * b * cos_theta * cos_theta + a * a * (1.0 - cos_theta * cos_theta)).sqrt();
    norm / r
}

/// Grows `mask` outward to `target_area` true cells by Dijkstra over
/// `step_cost(dx, dy) -> cost` (a step's grid offset to its travel-time
/// cost): the accepted set, stopped as soon as its size reaches
/// `target_area`, supersedes the starting mask (every starting cell is at
/// cost 0, so it is always accepted first). Shared by [`grow_ellipse`] and
/// [`grow_ellipse_centred`], which differ only in `step_cost`.
///
/// Deterministic: ties in accumulated cost break on cell index, the lowest
/// index first (`BinaryHeap<Reverse<(cost_bits, idx)>>` — `f64::to_bits`
/// preserves numeric order for the non-negative, finite costs here).
///
/// `target_area` is assumed to be at least the current mask's area — the
/// observed burned area this is matched to only grows between
/// observations — so every seed cell is always among the accepted set;
/// debug builds check this rather than silently returning a mask smaller
/// than `mask` itself.
fn dijkstra_grow(
    mask: &[bool],
    w: usize,
    h: usize,
    target_area: usize,
    step_cost: impl Fn(f64, f64) -> f64,
) -> Vec<bool> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    debug_assert!(
        target_area >= mask.iter().filter(|&&b| b).count(),
        "target_area must not shrink below the current mask (observed area is monotone)"
    );
    const OFFSETS: [(i32, i32); 8] = [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ];
    let mut visited = vec![false; mask.len()];
    let mut accepted = 0usize;
    let mut heap: BinaryHeap<Reverse<(u64, usize)>> = BinaryHeap::new();
    for (i, &m) in mask.iter().enumerate() {
        if m {
            heap.push(Reverse((0.0f64.to_bits(), i)));
        }
    }
    while let Some(Reverse((cost_bits, i))) = heap.pop() {
        if visited[i] {
            continue;
        }
        visited[i] = true;
        accepted += 1;
        if accepted >= target_area {
            break;
        }
        let cost = f64::from_bits(cost_bits);
        let (x, y) = (i % w, i / w);
        for &(dx, dy) in &OFFSETS {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                continue;
            }
            let ni = (ny as usize) * w + nx as usize;
            if visited[ni] {
                continue;
            }
            let step = step_cost(f64::from(dx), f64::from(dy));
            heap.push(Reverse(((cost + step).to_bits(), ni)));
        }
    }
    visited
}

/// Grows `mask` by the wind-oriented, rear-focus Ellipse (E41's headline
/// null). `wind_toward_rad`/`lb` set the direction and shape of
/// [`ellipse_step_cost`]; see its docs for what the ellipse template means.
pub(crate) fn grow_ellipse(
    mask: &[bool],
    w: usize,
    h: usize,
    wind_toward_rad: f64,
    lb: f64,
    target_area: usize,
) -> Vec<bool> {
    dijkstra_grow(mask, w, h, target_area, |dx, dy| {
        ellipse_step_cost(dx, dy, wind_toward_rad, lb)
    })
}

/// Grows `mask` by the centred-ellipse control ([`centred_ellipse_step_cost`]
/// — post-hoc, see its docs). Same signature as [`grow_ellipse`].
fn grow_ellipse_centred(
    mask: &[bool],
    w: usize,
    h: usize,
    wind_toward_rad: f64,
    lb: f64,
    target_area: usize,
) -> Vec<bool> {
    dijkstra_grow(mask, w, h, target_area, |dx, dy| {
        centred_ellipse_step_cost(dx, dy, wind_toward_rad, lb)
    })
}

/// The `nulls` mode report (E41): persistence, the Circle and the three
/// Ellipse variants, scored at every observation, with no ensemble at all
/// — cheap enough to run for every fire in a few seconds.
#[derive(serde::Serialize)]
struct NullsReport {
    scenario: String,
    mode: String,
    /// Which build produced this file, so two runs can be told apart:
    /// the short git commit hash `wildfire_smc` was compiled from
    /// (`"unknown"` if `git` wasn't available at build time).
    binary_git: String,
    /// Which build produced this file, so two runs can be told apart:
    /// the UTC timestamp `wildfire_smc` was compiled at.
    binary_built_utc: String,
    /// Whether `station_hourly.json` was found for this scenario; when
    /// `false` every `ellipse_station_*` field above and below is `None`.
    station_available: bool,
    wall_time_secs: f64,
    scores: Vec<NullObsScore>,
    mean_persistence_iou: f64,
    final_persistence_iou: f64,
    mean_radial_iou: f64,
    final_radial_iou: f64,
    mean_ellipse_era5_iou: f64,
    final_ellipse_era5_iou: f64,
    mean_ellipse_station_iou: Option<f64>,
    final_ellipse_station_iou: Option<f64>,
    mean_ellipse_era5x3_iou: f64,
    final_ellipse_era5x3_iou: f64,
    /// Post-hoc control (not pre-registered), same wind/LB as `ellipse_era5`
    /// but centred — see `NullObsScore.ellipse_era5_centred_iou`.
    mean_ellipse_era5_centred_iou: f64,
    final_ellipse_era5_centred_iou: f64,
    mean_brier_persistence: f64,
    mean_brier_radial: f64,
    mean_brier_ellipse_era5: f64,
    mean_brier_ellipse_station: Option<f64>,
    mean_brier_ellipse_era5x3: f64,
    mean_brier_ellipse_era5_centred: f64,
}

/// `nulls` mode (validation E41): no ensemble, just the deterministic
/// dummy forecasters — persistence, the Circle and the three Ellipse
/// variants — scored at every observation. This is the cheap, honest
/// first check of whether the *wind inputs we have* (ERA5 daily, or the
/// station log) carry any shape signal at all, before spending any more
/// effort on the fire model's kernel.
///
/// The Circle and the Ellipse both start from the ignition mask
/// (`truth.json`'s t = 0 perimeter — the same set `config.json`'s initial
/// state was built from, so this needs no `CellaConfig` or simulation) and
/// grow window by window. The wind for a window is the scenario's ERA5
/// entry that starts it (`ellipse_era5`, `ellipse_era5x3` — same entry,
/// speed × 3), or the vector mean of the station log's hourly rows that
/// fall inside it (`ellipse_station`, `None` when there is no station
/// file for this fire).
pub(crate) fn run_nulls(sc: &Scenario, truth: &Truth, dir: &Path, rot: f64, out: &Path) {
    let start = std::time::Instant::now();
    let (w, h) = (sc.grid.width, sc.grid.height);
    let total = w * h;
    let ignition = mask_at(&truth.arrival_hours, 0.0);
    let dist = chamfer_from(&ignition, w, h);
    let mut order: Vec<usize> = (0..total).collect();
    order.sort_by_key(|&i| (dist[i], i));

    let station: Option<StationLog> = load_station(dir);
    let station_available = station.is_some();

    let mut ellipse_era5 = ignition.clone();
    let mut ellipse_station = ignition.clone();
    let mut ellipse_era5x3 = ignition.clone();
    let mut ellipse_era5_centred = ignition.clone();

    let mut scores: Vec<NullObsScore> = Vec::new();
    let mut obs_idx = 1usize;
    for win in sc.wind.windows(2) {
        let (cur, next) = (&win[0], &win[1]);
        if obs_idx >= truth.observed_at.len()
            || (truth.observed_at[obs_idx] - next.hours).abs() >= 1e-6
        {
            continue;
        }
        let t = next.hours;
        let obs = mask_at(&truth.arrival_hours, t);
        let obs_n = obs.iter().filter(|&&b| b).count();

        let mut radial = vec![false; total];
        for &i in order.iter().take(obs_n) {
            radial[i] = true;
        }

        let toward_era5 = wind_toward_grid_deg(cur.from_deg + rot).to_radians();
        ellipse_era5 = grow_ellipse(
            &ellipse_era5,
            w,
            h,
            toward_era5,
            anderson_lb(cur.speed_ms),
            obs_n,
        );
        ellipse_era5x3 = grow_ellipse(
            &ellipse_era5x3,
            w,
            h,
            toward_era5,
            anderson_lb(cur.speed_ms * 3.0),
            obs_n,
        );
        // Post-hoc control (not pre-registered): same wind and LB as
        // ellipse_era5, but centred — see centred_ellipse_step_cost's docs.
        ellipse_era5_centred = grow_ellipse_centred(
            &ellipse_era5_centred,
            w,
            h,
            toward_era5,
            anderson_lb(cur.speed_ms),
            obs_n,
        );
        let (ellipse_station_iou, brier_ellipse_station) = match &station {
            Some(log) => {
                let (speed, toward) = station_vector_mean(log, cur.hours, next.hours);
                ellipse_station =
                    grow_ellipse(&ellipse_station, w, h, toward, anderson_lb(speed), obs_n);
                let prob: Vec<f32> = ellipse_station.iter().map(|&b| b as u8 as f32).collect();
                (Some(iou(&ellipse_station, &obs)), Some(brier(&prob, &obs)))
            }
            None => (None, None),
        };

        let prob_of = |m: &[bool]| -> Vec<f32> { m.iter().map(|&b| b as u8 as f32).collect() };
        scores.push(NullObsScore {
            hours: t,
            obs_burned: obs_n as u64,
            persistence_iou: iou(&ignition, &obs),
            brier_persistence: brier(&prob_of(&ignition), &obs),
            radial_iou: iou(&radial, &obs),
            brier_radial: brier(&prob_of(&radial), &obs),
            ellipse_era5_iou: iou(&ellipse_era5, &obs),
            brier_ellipse_era5: brier(&prob_of(&ellipse_era5), &obs),
            ellipse_station_iou,
            brier_ellipse_station,
            ellipse_era5x3_iou: iou(&ellipse_era5x3, &obs),
            brier_ellipse_era5x3: brier(&prob_of(&ellipse_era5x3), &obs),
            ellipse_era5_centred_iou: iou(&ellipse_era5_centred, &obs),
            brier_ellipse_era5_centred: brier(&prob_of(&ellipse_era5_centred), &obs),
        });
        eprintln!(
            "  t={t:5.0}h obs {obs_n:7} | persistence {:.3} radial {:.3} | ellipse era5 {:.3} station {} era5x3 {:.3} era5_centred {:.3}",
            iou(&ignition, &obs),
            iou(&radial, &obs),
            scores.last().unwrap().ellipse_era5_iou,
            scores
                .last()
                .unwrap()
                .ellipse_station_iou
                .map_or("  n/a".to_string(), |v| format!("{v:.3}")),
            scores.last().unwrap().ellipse_era5x3_iou,
            scores.last().unwrap().ellipse_era5_centred_iou,
        );
        obs_idx += 1;
    }

    let n = scores.len() as f64;
    let mean = |f: &dyn Fn(&NullObsScore) -> f64| scores.iter().map(f).sum::<f64>() / n;
    let mean_opt = |f: &dyn Fn(&NullObsScore) -> Option<f64>| -> Option<f64> {
        if station_available {
            Some(scores.iter().filter_map(f).sum::<f64>() / n)
        } else {
            None
        }
    };
    let (binary_git, binary_built_utc) = provenance();
    let report = NullsReport {
        scenario: sc.id.clone(),
        mode: "nulls".to_string(),
        binary_git,
        binary_built_utc,
        station_available,
        wall_time_secs: start.elapsed().as_secs_f64(),
        mean_persistence_iou: mean(&|s| s.persistence_iou),
        final_persistence_iou: scores.last().map_or(0.0, |s| s.persistence_iou),
        mean_radial_iou: mean(&|s| s.radial_iou),
        final_radial_iou: scores.last().map_or(0.0, |s| s.radial_iou),
        mean_ellipse_era5_iou: mean(&|s| s.ellipse_era5_iou),
        final_ellipse_era5_iou: scores.last().map_or(0.0, |s| s.ellipse_era5_iou),
        mean_ellipse_station_iou: mean_opt(&|s| s.ellipse_station_iou),
        final_ellipse_station_iou: scores.last().and_then(|s| s.ellipse_station_iou),
        mean_ellipse_era5x3_iou: mean(&|s| s.ellipse_era5x3_iou),
        final_ellipse_era5x3_iou: scores.last().map_or(0.0, |s| s.ellipse_era5x3_iou),
        mean_ellipse_era5_centred_iou: mean(&|s| s.ellipse_era5_centred_iou),
        final_ellipse_era5_centred_iou: scores.last().map_or(0.0, |s| s.ellipse_era5_centred_iou),
        mean_brier_persistence: mean(&|s| s.brier_persistence),
        mean_brier_radial: mean(&|s| s.brier_radial),
        mean_brier_ellipse_era5: mean(&|s| s.brier_ellipse_era5),
        mean_brier_ellipse_station: mean_opt(&|s| s.brier_ellipse_station),
        mean_brier_ellipse_era5x3: mean(&|s| s.brier_ellipse_era5x3),
        mean_brier_ellipse_era5_centred: mean(&|s| s.brier_ellipse_era5_centred),
        scores,
    };
    write_json(out, &report);
    eprintln!(
        "{}: nulls in {:.1}s | mean IoU persistence {:.3} radial {:.3} ellipse_era5 {:.3} ellipse_era5x3 {:.3} ellipse_era5_centred {:.3} ellipse_station {} -> {}",
        sc.id,
        report.wall_time_secs,
        report.mean_persistence_iou,
        report.mean_radial_iou,
        report.mean_ellipse_era5_iou,
        report.mean_ellipse_era5x3_iou,
        report.mean_ellipse_era5_centred_iou,
        report
            .mean_ellipse_station_iou
            .map_or("n/a".to_string(), |v| format!("{v:.3}")),
        out.display()
    );
}

#[cfg(test)]
mod ellipse_null_tests {
    use super::*;

    fn seed_mask(w: usize, h: usize, cx: usize, cy: usize) -> Vec<bool> {
        let mut m = vec![false; w * h];
        m[cy * w + cx] = true;
        m
    }

    #[test]
    fn anderson_lb_is_one_at_zero_wind_and_clamped_above_eight() {
        // The Anderson curve is fit to equal 1.0 (a circle) at U = 0 by
        // construction: 0.936 + 0.461 - 0.397 = 1.0 exactly.
        assert!((anderson_lb(0.0) - 1.0).abs() < 1e-9);
        // Way past the fit range, the clamp keeps the ellipse from
        // running away to a sliver.
        assert!((anderson_lb(100.0) - 8.0).abs() < 1e-9);
        assert!(anderson_lb(5.0) > 1.0 && anderson_lb(5.0) < 8.0);
    }

    #[test]
    fn calm_wind_reproduces_the_chamfer_disc() {
        // LB = 1 makes ellipse_step_cost isotropic (r(theta) = 1 for every
        // theta), so growing by Dijkstra should trace out the same disc
        // the chamfer-distance Circle null does.
        let (w, h) = (200, 200);
        let seed = seed_mask(w, h, 100, 100);
        let target = 3000; // ~radius 31 cells, well clear of the grid edge.
        let dist = chamfer_from(&seed, w, h);
        let mut order: Vec<usize> = (0..w * h).collect();
        order.sort_by_key(|&i| (dist[i], i));
        let mut circle = vec![false; w * h];
        for &i in order.iter().take(target) {
            circle[i] = true;
        }
        let ellipse = grow_ellipse(&seed, w, h, 0.0, 1.0, target);
        let score = iou(&ellipse, &circle);
        assert!(score >= 0.95, "calm-wind IoU with the chamfer disc {score}");
    }

    #[test]
    fn west_wind_stretches_the_grown_set_toward_positive_x() {
        // wind_toward_grid_deg(270) = 0: a west wind blows toward +x. With
        // LB = 3 the ellipse should read as clearly elongated along +x: at
        // least twice as far right of the seed as it reaches up or down.
        let (w, h) = (200, 200);
        let (cx, cy) = (60, 100);
        let seed = seed_mask(w, h, cx, cy);
        let toward = wind_toward_grid_deg(270.0).to_radians();
        let grown = grow_ellipse(&seed, w, h, toward, 3.0, 4000);

        let mut x_extent_right = 0i64;
        let mut y_extent = 0i64;
        for (i, &b) in grown.iter().enumerate() {
            if !b {
                continue;
            }
            let (x, y) = (i % w, i / w);
            x_extent_right = x_extent_right.max(x as i64 - cx as i64);
            y_extent = y_extent.max((y as i64 - cy as i64).abs());
        }
        assert!(
            x_extent_right as f64 >= 2.0 * y_extent as f64,
            "x-extent right {x_extent_right} should be >= 2x the y-extent {y_extent}"
        );
    }

    #[test]
    fn centred_ellipse_has_no_front_back_skew() {
        // The rear-focus ellipse (grow_ellipse) is asymmetric front to
        // back by construction; the centred control must not be. Same
        // wind toward +x, same LB = 3: left extent and right extent
        // should come out within 10% of each other.
        let (w, h) = (200, 200);
        let (cx, cy) = (100, 100);
        let seed = seed_mask(w, h, cx, cy);
        let grown = grow_ellipse_centred(&seed, w, h, 0.0, 3.0, 4000);

        let (mut left, mut right) = (0i64, 0i64);
        for (i, &b) in grown.iter().enumerate() {
            if !b {
                continue;
            }
            let x = (i % w) as i64;
            right = right.max(x - cx as i64);
            left = left.max(cx as i64 - x);
        }
        let (left, right) = (left as f64, right as f64);
        assert!(
            (left - right).abs() <= 0.10 * right.max(left),
            "left extent {left} should be within 10% of right extent {right}"
        );
    }

    #[test]
    fn grow_ellipse_matches_the_target_area_exactly() {
        let (w, h) = (120, 120);
        let seed = seed_mask(w, h, 60, 60);
        for &target in &[1usize, 500, 5000] {
            let grown = grow_ellipse(&seed, w, h, 1.2, 2.5, target);
            let count = grown.iter().filter(|&&b| b).count();
            assert_eq!(count, target.max(1), "target area {target}");
        }
    }

    /// Lagged persistence on a toy pair of nested masks (real arrival-time
    /// masks are always nested: burned cells stay burned): IoU of a subset
    /// against its superset reduces to |subset| / |superset|, since the
    /// intersection is the subset itself and the union is the superset.
    #[test]
    fn lagged_persistence_iou_is_the_nested_area_ratio() {
        let (w, h) = (20, 20);
        let total = w * h;
        // A_{k-1}: the first 30 cells burned. A_k: the first 70 (a superset).
        let a_prev = {
            let mut m = vec![false; total];
            m[..30].fill(true);
            m
        };
        let a_now = {
            let mut m = vec![false; total];
            m[..70].fill(true);
            m
        };
        let lagged_persistence_iou = iou(&a_prev, &a_now);
        assert!((lagged_persistence_iou - 30.0 / 70.0).abs() < 1e-12);

        // Degenerate case: no growth at all (A_{k-1} == A_k) is a perfect
        // lagged forecast, IoU 1.0 -- the ratio formula agrees (70/70).
        assert!((iou(&a_now, &a_now) - 1.0).abs() < 1e-12);
    }

    /// The lagged Circle's defining property: whatever seed it grows from,
    /// the result has exactly the target area -- including a seed that is
    /// itself already bigger than the target (can't happen with a truly
    /// monotone observed series, but the function must not panic or
    /// silently shrink below the seed either way, matching how
    /// `dijkstra_grow`'s own debug assertion treats the Circle/Ellipse).
    #[test]
    fn lagged_circle_area_equals_the_target() {
        let (w, h) = (40, 40);
        let seed = seed_mask(w, h, 20, 20);
        for &target in &[1usize, 50, 800, 1600] {
            let grown = radial_mask_from(&seed, w, h, target);
            assert_eq!(grown.iter().filter(|&&b| b).count(), target);
        }
        // A non-trivial seed (not just one cell) works the same way.
        let mut wide_seed = vec![false; w * h];
        wide_seed[..40].fill(true);
        let grown = radial_mask_from(&wide_seed, w, h, 200);
        assert_eq!(grown.iter().filter(|&&b| b).count(), 200);
        // Every seed cell is included (it's at distance 0, so it's always
        // among the closest `target` cells).
        assert!(wide_seed.iter().zip(&grown).all(|(&s, &g)| !s || g));
    }
}
