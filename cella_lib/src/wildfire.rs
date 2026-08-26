//! Alexandridis-style stochastic wildfire model, the first in-tree
//! [`ExternalModel`].
//!
//! Cell states are ordinary interned [`CellType`]s: any number of fuel classes
//! (each with a flammability factor), a `Burning` state, an absorbing
//! `BurnedOut` state, and the engine's `Inactive` background acting as
//! unburnable terrain (water, roads, firebreaks). A fuel cell ignites
//! stochastically based on its burning neighbors:
//!
//! ```text
//! p(neighbor j) = p_base[cell] * dir_factor[j] * slope[cell, j]
//! p_ignite      = 1 - Π_j (1 - p(j))            (inclusion–exclusion)
//! ```
//!
//! where `p_base = p0 * veg_factor(class) * density[cell]`,
//! `dir_factor[j] = exp(c1·V) · exp(V·c2·(cos θ_j − 1)) / dist_j` is the
//! Alexandridis wind factor with a `1/√2` diagonal-distance correction, and
//! `slope[cell, j] = exp(a · θ_s)` with `θ_s` the slope angle in degrees from
//! the neighbor up to the cell (Alexandridis et al. 2008: p_h = 0.58,
//! c1 = 0.045, c2 = 0.131, a = 0.078).
//!
//! Everything expensive is precomputed: the slope table once at attach
//! (elevation is static), the eight wind factors once per chunk (wind is
//! uniform per step), leaving two multiplies per burning neighbor and no
//! transcendentals in the per-cell loop.
//!
//! Randomness is a stateless counter-based hash of
//! `(seed, step, cell index, stream)` — see [`cell_rand`] — so results are
//! exactly reproducible for a fixed seed and independent of thread and chunk
//! count. Spotting (long-range ignition by firebrands) samples a lognormal
//! landing distance along the wind vector (Sardoy et al.) and is delivered
//! through [`ModelEvent`]s.

use serde::{Deserialize, Serialize};

use crate::external::{ChunkCtx, ExternalModel, GridView, ModelError, ModelEvent};
use crate::rules::{Neighborhood2D, neighborhood_offsets};
use crate::types::CellType;

/// RNG stream for the ignition draw.
const STREAM_IGNITE: u64 = 0;
/// RNG stream for the spot-trigger draw.
const STREAM_SPOT: u64 = 1;
/// RNG streams for the Box–Muller pair of the lognormal distance.
const STREAM_DIST_A: u64 = 2;
const STREAM_DIST_B: u64 = 3;
/// RNG stream for the landing-angle jitter.
const STREAM_ANGLE: u64 = 4;

#[inline]
fn mix(mut z: u64) -> u64 {
    // SplitMix64 finalizer (Steele et al.); full-avalanche integer mixer.
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Stateless counter-based uniform draw in `[0, 1)`.
///
/// The value depends only on the four inputs, never on call order, thread
/// count, or chunk layout — the property that makes stochastic runs
/// snapshot-testable. Distinct `stream` values give independent draws for the
/// same cell and step.
#[inline]
pub fn cell_rand(seed: u64, step: u64, idx: u64, stream: u64) -> f32 {
    let z = mix(seed
        ^ mix(step.wrapping_mul(0x9E37_79B9_7F4A_7C15))
        ^ mix(idx.wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
        ^ stream.wrapping_mul(0x1656_67B1_9E37_79F9));
    // Top 24 bits -> f32 in [0, 1) with a full mantissa.
    ((z >> 40) as f32) * (1.0 / (1u64 << 24) as f32)
}

/// One fuel class: a cell type name plus its flammability multiplier.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FuelClass {
    pub name: String,
    /// Multiplies `p0`; Alexandridis' `(1 + p_veg)(1 + p_den)` folded into one
    /// per-class factor. Must be `>= 0`.
    pub veg_factor: f64,
}

/// Firebrand spotting parameters.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SpottingParams {
    /// Per-step probability that a burning cell launches a firebrand.
    pub p_spot: f64,
    /// Median landing distance in cells (lognormal median, `exp(μ)`).
    pub median_distance: f64,
    /// Lognormal shape parameter `σ` (must be `>= 0`).
    pub sigma: f64,
    /// Uniform jitter around the wind direction, in degrees.
    pub angle_jitter_deg: f64,
}

fn default_p0() -> f64 {
    0.58
}
fn default_c1() -> f64 {
    0.045
}
fn default_c2() -> f64 {
    0.131
}
fn default_slope_a() -> f64 {
    0.078
}
fn default_cell_size() -> f64 {
    30.0
}
fn default_burn_duration() -> u32 {
    1
}

/// Tunable wildfire parameters. Serde defaults follow Alexandridis et al.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WildfireParams {
    /// Seed for the counter-based RNG; fixes the whole run.
    pub seed: u64,
    /// Base ignition probability under no wind on flat terrain.
    #[serde(default = "default_p0")]
    pub p0: f64,
    /// Fuel classes; every other non-Burning/BurnedOut/Inactive type is inert.
    pub fuels: Vec<FuelClass>,
    /// Wind speed in m/s.
    #[serde(default)]
    pub wind_speed: f64,
    /// Direction the wind blows *toward*, degrees; 0° = +x, 90° = +y.
    #[serde(default)]
    pub wind_dir_deg: f64,
    #[serde(default = "default_c1")]
    pub c1: f64,
    #[serde(default = "default_c2")]
    pub c2: f64,
    /// Slope coefficient `a`, per degree of slope angle.
    #[serde(default = "default_slope_a")]
    pub slope_a: f64,
    /// Cell edge length in meters (scales slope angles).
    #[serde(default = "default_cell_size")]
    pub cell_size: f64,
    /// Steps a cell burns before becoming burned out (`>= 1`).
    #[serde(default = "default_burn_duration")]
    pub burn_duration: u32,
    #[serde(default)]
    pub spotting: Option<SpottingParams>,
    /// Override for the burning state's type name (default "Burning").
    #[serde(default)]
    pub burning_name: Option<String>,
    /// Override for the burned-out state's type name (default "BurnedOut").
    #[serde(default)]
    pub burned_name: Option<String>,
}

/// Per-cell environment layers. Empty vectors mean uniform density / flat
/// terrain; otherwise the length must be `width * height`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WildfireEnv {
    pub density: Vec<f32>,
    pub elevation: Vec<f32>,
}

/// State derived from params + env + grid at attach time. Never serialized;
/// rebuilt by [`WildfireModel::attach`].
#[derive(Clone, Debug, Default)]
struct WildfireDerived {
    /// `p0 * veg_factor * density` per cell; `0.0` marks non-fuel.
    p_base: Vec<f32>,
    /// `exp(slope_a * slope_angle_deg)` per (cell, neighbor j); length `8·w·h`.
    slope: Vec<f32>,
    /// The eight Moore offsets, in `neighborhood_offsets` order.
    offsets: [(i32, i32); 8],
    /// Linear index offsets matching `offsets` for the interior fast path.
    lin: [isize; 8],
    burning: CellType,
    burned: CellType,
    inactive: CellType,
    /// Resolved fuel classes as `(type, p0 * veg_factor)`.
    fuels: Vec<(CellType, f32)>,
}

/// The wildfire model. See the [module docs](self).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WildfireModel {
    pub params: WildfireParams,
    #[serde(default)]
    pub env: WildfireEnv,
    #[serde(skip)]
    derived: WildfireDerived,
}

impl WildfireModel {
    pub fn new(params: WildfireParams, env: WildfireEnv) -> Self {
        Self {
            params,
            env,
            derived: WildfireDerived::default(),
        }
    }

    fn burning_name(&self) -> &str {
        self.params.burning_name.as_deref().unwrap_or("Burning")
    }

    fn burned_name(&self) -> &str {
        self.params.burned_name.as_deref().unwrap_or("BurnedOut")
    }

    /// `p_base` for a cell of type `t` at `idx` (0.0 for non-fuel).
    fn p_base_for(&self, t: CellType, idx: usize) -> f32 {
        let density = self.env.density.get(idx).copied().unwrap_or(1.0);
        match self.derived.fuels.iter().find(|(ft, _)| *ft == t) {
            Some((_, base)) => base * density,
            None => 0.0,
        }
    }

    /// The eight per-direction wind factors for the current wind, including
    /// the diagonal distance correction. Cheap enough to rebuild per chunk.
    fn dir_factors(&self) -> [f32; 8] {
        let v = self.params.wind_speed;
        let theta_w = self.params.wind_dir_deg.to_radians();
        let (wy, wx) = theta_w.sin_cos();
        let mut f = [0.0f32; 8];
        for (j, &(dx, dy)) in self.derived.offsets.iter().enumerate() {
            // Spread direction: from the burning neighbor toward this cell.
            let (sx, sy) = (-(dx as f64), -(dy as f64));
            let norm = (sx * sx + sy * sy).sqrt();
            let cos_theta = (wx * sx + wy * sy) / norm;
            let wind = (self.params.c1 * v).exp() * (v * self.params.c2 * (cos_theta - 1.0)).exp();
            f[j] = (wind / norm) as f32;
        }
        f
    }

    /// Sample a spot-fire landing cell for a burning cell, if any.
    fn spot_target(&self, ctx: &ChunkCtx<'_>, idx: usize, x: usize, y: usize) -> Option<usize> {
        let spot = self.params.spotting.as_ref()?;
        let seed = self.params.seed;
        let iu = idx as u64;
        if f64::from(cell_rand(seed, ctx.step, iu, STREAM_SPOT)) >= spot.p_spot {
            return None;
        }
        // Lognormal distance via Box–Muller; guard u_a away from 0.
        let u_a =
            (1.0 - f64::from(cell_rand(seed, ctx.step, iu, STREAM_DIST_A))).max(f64::MIN_POSITIVE);
        let u_b = f64::from(cell_rand(seed, ctx.step, iu, STREAM_DIST_B));
        let z = (-2.0 * u_a.ln()).sqrt() * (std::f64::consts::TAU * u_b).cos();
        let dist = spot.median_distance * (spot.sigma * z).exp();
        let jitter = (2.0 * f64::from(cell_rand(seed, ctx.step, iu, STREAM_ANGLE)) - 1.0)
            * spot.angle_jitter_deg;
        let angle = (self.params.wind_dir_deg + jitter).to_radians();
        let (ay, ax) = angle.sin_cos();
        let tx = x as f64 + dist * ax;
        let ty = y as f64 + dist * ay;
        let (txr, tyr) = (tx.round(), ty.round());
        if txr < 0.0 || tyr < 0.0 || txr >= ctx.width as f64 || tyr >= ctx.height as f64 {
            return None;
        }
        let target = tyr as usize * ctx.width + txr as usize;
        (target != idx).then_some(target)
    }

    /// Next type for one cell plus an optional spotting event target.
    ///
    /// `neighbor` maps a direction slot `j` to the neighbor's type, already
    /// bounds-resolved by the caller (interior: linear offsets; edge: checked).
    #[inline]
    fn next_type<F: Fn(usize) -> CellType>(
        &self,
        ctx: &ChunkCtx<'_>,
        idx: usize,
        local: usize,
        cur: CellType,
        dir: &[f32; 8],
        neighbor: F,
    ) -> CellType {
        let d = &self.derived;
        if cur == d.burning {
            return if ctx.ages[local] + 1 >= self.params.burn_duration {
                d.burned
            } else {
                d.burning
            };
        }
        // Burned out is absorbing and must be checked by type: p_base is keyed
        // to the cell's *initial* class and stays positive after burnout.
        if cur == d.burned {
            return cur;
        }
        let p_base = d.p_base[idx];
        if p_base <= 0.0 {
            return cur; // inactive or inert type: absorbing.
        }
        let mut p_no = 1.0f32;
        let mut any = false;
        let slope = &d.slope[idx * 8..idx * 8 + 8];
        for j in 0..8 {
            if neighbor(j) == d.burning {
                any = true;
                let p = (p_base * dir[j] * slope[j]).clamp(0.0, 1.0);
                p_no *= 1.0 - p;
            }
        }
        if any && cell_rand(self.params.seed, ctx.step, idx as u64, STREAM_IGNITE) < 1.0 - p_no {
            d.burning
        } else {
            cur
        }
    }
}

#[typetag::serde(name = "wildfire")]
impl ExternalModel for WildfireModel {
    fn attach(&mut self, view: &GridView<'_>) -> Result<(), ModelError> {
        let p = &self.params;
        let n = view.width * view.height;
        if view.width == 0 || view.height == 0 {
            return Err(ModelError::InvalidParam("grid must be non-empty".into()));
        }
        if !(0.0..=1.0).contains(&p.p0) {
            return Err(ModelError::InvalidParam(format!(
                "p0 must be in [0, 1], got {}",
                p.p0
            )));
        }
        if p.burn_duration == 0 {
            return Err(ModelError::InvalidParam(
                "burn_duration must be >= 1".into(),
            ));
        }
        if p.cell_size <= 0.0 {
            return Err(ModelError::InvalidParam(format!(
                "cell_size must be > 0, got {}",
                p.cell_size
            )));
        }
        if p.fuels.is_empty() {
            return Err(ModelError::InvalidParam(
                "at least one fuel class is required".into(),
            ));
        }
        for f in &p.fuels {
            if f.veg_factor < 0.0 {
                return Err(ModelError::InvalidParam(format!(
                    "veg_factor for '{}' must be >= 0, got {}",
                    f.name, f.veg_factor
                )));
            }
        }
        if let Some(s) = &p.spotting {
            if !(0.0..=1.0).contains(&s.p_spot) {
                return Err(ModelError::InvalidParam(format!(
                    "p_spot must be in [0, 1], got {}",
                    s.p_spot
                )));
            }
            if s.median_distance <= 0.0 || s.sigma < 0.0 {
                return Err(ModelError::InvalidParam(
                    "spotting requires median_distance > 0 and sigma >= 0".into(),
                ));
            }
        }
        for (name, len) in [
            ("density", self.env.density.len()),
            ("elevation", self.env.elevation.len()),
        ] {
            if len != 0 && len != n {
                return Err(ModelError::LayerLength {
                    layer: if name == "density" {
                        "density"
                    } else {
                        "elevation"
                    },
                    expected: n,
                    got: len,
                });
            }
        }
        let burning = CellType::new(self.burning_name());
        let burned = CellType::new(self.burned_name());
        let reserved = [
            (burning, self.burning_name().to_string()),
            (burned, self.burned_name().to_string()),
            (view.inactive, view.inactive.as_str().to_string()),
        ];
        if burning == burned || burning == view.inactive || burned == view.inactive {
            return Err(ModelError::NameCollision(format!(
                "burning/burned/inactive names must be distinct: {}, {}, {}",
                self.burning_name(),
                self.burned_name(),
                view.inactive.as_str()
            )));
        }
        let mut fuels: Vec<(CellType, f32)> = Vec::with_capacity(p.fuels.len());
        for f in &p.fuels {
            let t = CellType::new(&f.name);
            if let Some((_, r)) = reserved.iter().find(|(rt, _)| *rt == t) {
                return Err(ModelError::NameCollision(format!(
                    "fuel class '{}' collides with '{r}'",
                    f.name
                )));
            }
            if fuels.iter().any(|(ft, _)| *ft == t) {
                return Err(ModelError::NameCollision(format!(
                    "duplicate fuel class '{}'",
                    f.name
                )));
            }
            fuels.push((t, (p.p0 * f.veg_factor) as f32));
        }

        let offs = neighborhood_offsets(Neighborhood2D::Moore, 1);
        debug_assert_eq!(offs.len(), 8);
        let mut offsets = [(0i32, 0i32); 8];
        offsets.copy_from_slice(&offs);
        let mut lin = [0isize; 8];
        for (j, &(dx, dy)) in offsets.iter().enumerate() {
            lin[j] = dy as isize * view.width as isize + dx as isize;
        }

        self.derived = WildfireDerived {
            p_base: Vec::new(),
            slope: Vec::new(),
            offsets,
            lin,
            burning,
            burned,
            inactive: view.inactive,
            fuels,
        };

        let mut p_base = vec![0.0f32; n];
        for (idx, &t) in view.cells.iter().enumerate() {
            p_base[idx] = self.p_base_for(t, idx);
        }
        self.derived.p_base = p_base;

        // Slope table: exp(a * slope_angle_deg) from neighbor j up to the cell.
        // Flat terrain (empty elevation layer) gives all-1.0.
        let mut slope = vec![1.0f32; n * 8];
        if !self.env.elevation.is_empty() {
            let elev = &self.env.elevation;
            let w = view.width as i32;
            let h = view.height as i32;
            for y in 0..h {
                for x in 0..w {
                    let idx = (y * w + x) as usize;
                    for (j, &(dx, dy)) in offsets.iter().enumerate() {
                        let (nx, ny) = (x + dx, y + dy);
                        if nx < 0 || ny < 0 || nx >= w || ny >= h {
                            continue; // stays 1.0; edge path never reads it for OOB.
                        }
                        let nidx = (ny * w + nx) as usize;
                        let run = f64::hypot(dx as f64, dy as f64) * p.cell_size;
                        let rise = f64::from(elev[idx]) - f64::from(elev[nidx]);
                        let angle_deg = (rise / run).atan().to_degrees();
                        slope[idx * 8 + j] = (p.slope_a * angle_deg).exp() as f32;
                    }
                }
            }
        }
        self.derived.slope = slope;
        Ok(())
    }

    fn work_per_cell(&self) -> usize {
        // Eight neighbor reads plus the RNG hash, float math, and the engine's
        // separate bookkeeping pass: heavier than a plain threshold visit.
        20
    }

    /// Fire only moves at its edges, and this loop exploits that.
    ///
    /// A cell's type can only change this step if it is Burning (it ages or
    /// burns out) or if it has at least one Burning neighbor (it might
    /// ignite). Everything else — unburned fuel far from the fire, burned
    /// ground, water — stays exactly as it is. On a big grid the fire front
    /// is a thin line, so "might change" is a tiny fraction of all cells.
    ///
    /// So instead of running the transition math for every cell:
    ///
    /// 1. Copy the current types over as the default next state.
    /// 2. Build a bitmap with one bit per cell: 1 = Burning. (The rows this
    ///    chunk covers, plus one halo row above and below.)
    /// 3. OR together the bitmap's eight one-cell shifts (plus itself). A set
    ///    bit now means "this cell is Burning or touches a Burning cell" —
    ///    the only cells worth visiting.
    /// 4. Walk just those set bits and run the normal per-cell transition
    ///    ([`Self::next_type`]) and spotting draw for them.
    ///
    /// Results are identical to visiting every cell: skipped cells could not
    /// have changed, and they never consumed randomness in the first place
    /// (the ignition draw only happens when a burning neighbor exists).
    fn step_chunk(&self, ctx: &ChunkCtx<'_>, next: &mut [CellType]) -> Vec<ModelEvent> {
        let d = &self.derived;
        let dir = self.dir_factors();
        let mut events = Vec::new();
        let width = ctx.width;
        let height = ctx.height;
        let cells = ctx.cells;
        let start = ctx.start;
        let len = next.len();
        if len == 0 {
            return events;
        }
        // 1. Default: cells untouched by fire keep their type.
        next.copy_from_slice(&cells[start..start + len]);

        // 2. Burning bitmap for the chunk's rows plus a one-row halo.
        let wpr = width.div_ceil(64); // words per row
        let y_first = start / width;
        let y_last = (start + len - 1) / width;
        let y_lo = y_first.saturating_sub(1);
        let y_hi = (y_last + 1).min(height - 1);
        let nrows = y_hi - y_lo + 1;
        let mut burn = vec![0u64; nrows * wpr];
        for r in 0..nrows {
            let row = (y_lo + r) * width;
            for x in 0..width {
                if cells[row + x] == d.burning {
                    burn[r * wpr + x / 64] |= 1 << (x % 64);
                }
            }
        }
        let zero = vec![0u64; wpr];

        for y in y_first..=y_last {
            let r = y - y_lo;
            let cur_b = &burn[r * wpr..(r + 1) * wpr];
            let up: &[u64] = if y > 0 {
                &burn[(r - 1) * wpr..r * wpr]
            } else {
                &zero
            };
            let dn: &[u64] = if y + 1 < height {
                &burn[(r + 1) * wpr..(r + 2) * wpr]
            } else {
                &zero
            };
            // The chunk may start or end mid-row; only visit its own cells.
            let row_start = y * width;
            let x_lo = start.saturating_sub(row_start);
            let x_hi = (start + len - row_start).min(width);
            let row_interior = y >= 1 && y + 1 < height;
            for k in (x_lo / 64)..=((x_hi - 1) / 64) {
                let shl = |row: &[u64]| (row[k] << 1) | if k > 0 { row[k - 1] >> 63 } else { 0 };
                let shr =
                    |row: &[u64]| (row[k] >> 1) | if k + 1 < wpr { row[k + 1] << 63 } else { 0 };
                // 3. Burning-or-touching-Burning mask for these 64 cells.
                let mut m = shl(up)
                    | up[k]
                    | shr(up)
                    | shl(cur_b)
                    | cur_b[k]
                    | shr(cur_b)
                    | shl(dn)
                    | dn[k]
                    | shr(dn);
                // Clip to this chunk's cells within the row.
                if k == x_lo / 64 {
                    m &= !0u64 << (x_lo % 64);
                }
                if k == (x_hi - 1) / 64 {
                    let t = x_hi - k * 64;
                    if t < 64 {
                        m &= (1u64 << t) - 1;
                    }
                }
                // 4. Visit only the set bits.
                while m != 0 {
                    let x = k * 64 + m.trailing_zeros() as usize;
                    m &= m - 1; // clear that bit
                    let idx = y * width + x;
                    let local = idx - start;
                    let cur = cells[idx];
                    let new_type = if row_interior && x >= 1 && x + 1 < width {
                        self.next_type(ctx, idx, local, cur, &dir, |j| {
                            cells[idx.wrapping_add_signed(d.lin[j])]
                        })
                    } else {
                        self.next_type(ctx, idx, local, cur, &dir, |j| {
                            let (dx, dy) = d.offsets[j];
                            let (nx, ny) = (x as i64 + dx as i64, y as i64 + dy as i64);
                            if nx < 0 || ny < 0 || nx >= width as i64 || ny >= height as i64 {
                                d.inactive
                            } else {
                                cells[ny as usize * width + nx as usize]
                            }
                        })
                    };
                    next[local] = new_type;
                    if cur == d.burning {
                        if let Some(target) = self.spot_target(ctx, idx, x, y) {
                            events.push(ModelEvent {
                                target,
                                new_type: d.burning,
                            });
                        }
                    }
                }
            }
        }
        events
    }

    fn event_applies(&self, current_next: CellType, _event: &ModelEvent) -> bool {
        // Only standing fuel can be spot-ignited; re-application to an
        // already-burning target is rejected, making application idempotent.
        self.derived.fuels.iter().any(|(t, _)| *t == current_next)
    }

    fn on_paint(&mut self, idx: usize, new_type: CellType) {
        if idx < self.derived.p_base.len() {
            self.derived.p_base[idx] = self.p_base_for(new_type, idx);
        }
    }

    fn declared_types(&self) -> Vec<CellType> {
        let d = &self.derived;
        let mut out: Vec<CellType> = d.fuels.iter().map(|(t, _)| *t).collect();
        out.push(d.burning);
        out.push(d.burned);
        out
    }

    fn boxed_clone(&self) -> Box<dyn ExternalModel> {
        Box::new(self.clone())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_params() -> WildfireParams {
        WildfireParams {
            seed: 7,
            p0: 0.58,
            fuels: vec![FuelClass {
                name: "Forest".into(),
                veg_factor: 1.0,
            }],
            wind_speed: 0.0,
            wind_dir_deg: 0.0,
            c1: 0.045,
            c2: 0.131,
            slope_a: 0.078,
            cell_size: 30.0,
            burn_duration: 1,
            spotting: None,
            burning_name: None,
            burned_name: None,
        }
    }

    fn attach_on(mut m: WildfireModel, w: usize, h: usize, cells: &[CellType]) -> WildfireModel {
        let view = GridView {
            width: w,
            height: h,
            cells,
            inactive: CellType::inactive(),
        };
        m.attach(&view).expect("attach");
        m
    }

    fn forest_grid(w: usize, h: usize) -> Vec<CellType> {
        vec![CellType::new("Forest"); w * h]
    }

    #[test]
    fn cell_rand_is_uniform_and_stable() {
        let n = 100_000;
        let mut sum = 0.0f64;
        for i in 0..n {
            let v = cell_rand(42, 3, i, STREAM_IGNITE);
            assert!((0.0..1.0).contains(&v));
            sum += f64::from(v);
        }
        let mean = sum / n as f64;
        assert!((mean - 0.5).abs() < 0.01, "mean {mean}");
        // Stable across calls (stateless).
        assert_eq!(cell_rand(42, 3, 17, 0), cell_rand(42, 3, 17, 0));
    }

    #[test]
    fn cell_rand_streams_and_inputs_are_independent() {
        let a = cell_rand(1, 2, 3, STREAM_IGNITE);
        assert_ne!(a, cell_rand(1, 2, 3, STREAM_SPOT));
        assert_ne!(a, cell_rand(1, 2, 4, STREAM_IGNITE));
        assert_ne!(a, cell_rand(1, 3, 3, STREAM_IGNITE));
        assert_ne!(a, cell_rand(2, 2, 3, STREAM_IGNITE));
    }

    #[test]
    fn attach_rejects_bad_params() {
        let cells = forest_grid(2, 2);
        let check = |f: fn(&mut WildfireParams), msg: &str| {
            let mut p = base_params();
            f(&mut p);
            let mut m = WildfireModel::new(p, WildfireEnv::default());
            let view = GridView {
                width: 2,
                height: 2,
                cells: &cells,
                inactive: CellType::inactive(),
            };
            assert!(m.attach(&view).is_err(), "expected error: {msg}");
        };
        check(|p| p.p0 = 1.5, "p0 range");
        check(|p| p.p0 = -0.1, "p0 negative");
        check(|p| p.burn_duration = 0, "burn_duration");
        check(|p| p.cell_size = 0.0, "cell_size");
        check(|p| p.fuels.clear(), "no fuels");
        check(|p| p.fuels[0].veg_factor = -1.0, "veg_factor");
        check(
            |p| p.fuels[0].name = "Burning".into(),
            "fuel collides with burning",
        );
        check(
            |p| p.fuels[0].name = "Inactive".into(),
            "fuel collides with inactive",
        );
        check(
            |p| {
                p.fuels.push(FuelClass {
                    name: "Forest".into(),
                    veg_factor: 2.0,
                })
            },
            "duplicate fuel",
        );
        check(
            |p| p.burned_name = Some("Burning".into()),
            "burning == burned",
        );
        check(
            |p| {
                p.spotting = Some(SpottingParams {
                    p_spot: 2.0,
                    median_distance: 5.0,
                    sigma: 0.4,
                    angle_jitter_deg: 0.0,
                })
            },
            "p_spot range",
        );
        check(
            |p| {
                p.spotting = Some(SpottingParams {
                    p_spot: 0.1,
                    median_distance: 0.0,
                    sigma: 0.4,
                    angle_jitter_deg: 0.0,
                })
            },
            "median_distance",
        );
        check(
            |p| {
                p.spotting = Some(SpottingParams {
                    p_spot: 0.1,
                    median_distance: 5.0,
                    sigma: -0.1,
                    angle_jitter_deg: 0.0,
                })
            },
            "sigma",
        );
    }

    #[test]
    fn attach_rejects_bad_layers_and_empty_grid() {
        let cells = forest_grid(2, 2);
        let view = GridView {
            width: 2,
            height: 2,
            cells: &cells,
            inactive: CellType::inactive(),
        };
        let mut m = WildfireModel::new(
            base_params(),
            WildfireEnv {
                density: vec![1.0; 3],
                elevation: vec![],
            },
        );
        assert_eq!(
            m.attach(&view),
            Err(ModelError::LayerLength {
                layer: "density",
                expected: 4,
                got: 3
            })
        );
        let mut m = WildfireModel::new(
            base_params(),
            WildfireEnv {
                density: vec![],
                elevation: vec![0.0; 5],
            },
        );
        assert!(matches!(
            m.attach(&view),
            Err(ModelError::LayerLength {
                layer: "elevation",
                ..
            })
        ));
        let mut m = WildfireModel::new(base_params(), WildfireEnv::default());
        let empty = GridView {
            width: 0,
            height: 0,
            cells: &[],
            inactive: CellType::inactive(),
        };
        assert!(matches!(m.attach(&empty), Err(ModelError::InvalidParam(_))));
    }

    #[test]
    fn attach_builds_p_base_from_cells_density_and_veg() {
        let f = CellType::new("Forest");
        let s = CellType::new("Shrub");
        let cells = vec![f, s, CellType::inactive(), CellType::new("Burning")];
        let mut p = base_params();
        p.fuels.push(FuelClass {
            name: "Shrub".into(),
            veg_factor: 0.5,
        });
        let env = WildfireEnv {
            density: vec![1.0, 2.0, 1.0, 1.0],
            elevation: vec![],
        };
        let m = attach_on(WildfireModel::new(p, env), 2, 2, &cells);
        let pb = &m.derived.p_base;
        assert!((pb[0] - 0.58).abs() < 1e-6);
        assert!((pb[1] - 0.58).abs() < 1e-6, "0.58 * 0.5 veg * 2.0 density");
        assert_eq!(pb[2], 0.0, "inactive is not fuel");
        assert_eq!(pb[3], 0.0, "burning cell has no fuel base");
    }

    #[test]
    fn wind_factor_is_max_downwind_min_upwind() {
        let cells = forest_grid(3, 3);
        let mut p = base_params();
        p.wind_speed = 8.0;
        p.wind_dir_deg = 0.0; // blowing toward +x
        let m = attach_on(WildfireModel::new(p, WildfireEnv::default()), 3, 3, &cells);
        let dir = m.dir_factors();
        let offs = m.derived.offsets;
        // Spread direction is -offset: the downwind-most spread comes from the
        // neighbor at offset (-1, 0) (fire west of us, wind pushing east).
        let j_down = offs.iter().position(|&o| o == (-1, 0)).unwrap();
        let j_up = offs.iter().position(|&o| o == (1, 0)).unwrap();
        let j_side = offs.iter().position(|&o| o == (0, 1)).unwrap();
        assert!(
            dir[j_down] > dir[j_side],
            "downwind {} > crosswind {}",
            dir[j_down],
            dir[j_side]
        );
        assert!(
            dir[j_side] > dir[j_up],
            "crosswind {} > upwind {}",
            dir[j_side],
            dir[j_up]
        );
        // No wind: cardinal factors 1.0, diagonals 1/sqrt(2).
        let m0 = attach_on(
            WildfireModel::new(base_params(), WildfireEnv::default()),
            3,
            3,
            &cells,
        );
        let dir0 = m0.dir_factors();
        let j_card = offs.iter().position(|&o| o == (0, -1)).unwrap();
        let j_diag = offs.iter().position(|&o| o == (1, 1)).unwrap();
        assert!((dir0[j_card] - 1.0).abs() < 1e-6);
        assert!((dir0[j_diag] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    }

    #[test]
    fn slope_table_monotone_on_ramp() {
        // 3x1 ramp rising toward +x: uphill spread (from lower neighbor) > 1,
        // downhill < 1, flat crosswise = 1.
        let cells = forest_grid(3, 3);
        let mut elev = vec![0.0f32; 9];
        for y in 0..3 {
            for x in 0..3 {
                elev[y * 3 + x] = x as f32 * 10.0;
            }
        }
        let m = attach_on(
            WildfireModel::new(
                base_params(),
                WildfireEnv {
                    density: vec![],
                    elevation: elev,
                },
            ),
            3,
            3,
            &cells,
        );
        let offs = m.derived.offsets;
        let center = 4usize;
        let j_from_west = offs.iter().position(|&o| o == (-1, 0)).unwrap();
        let j_from_east = offs.iter().position(|&o| o == (1, 0)).unwrap();
        let j_from_north = offs.iter().position(|&o| o == (0, -1)).unwrap();
        let s = &m.derived.slope[center * 8..center * 8 + 8];
        assert!(
            s[j_from_west] > 1.0,
            "fire below climbing up: {}",
            s[j_from_west]
        );
        assert!(
            s[j_from_east] < 1.0,
            "fire above burning down: {}",
            s[j_from_east]
        );
        assert!(
            (s[j_from_north] - 1.0).abs() < 1e-6,
            "level ground: {}",
            s[j_from_north]
        );
        // Flat terrain leaves the whole table at 1.0.
        let flat = attach_on(
            WildfireModel::new(base_params(), WildfireEnv::default()),
            3,
            3,
            &cells,
        );
        assert!(flat.derived.slope.iter().all(|&v| v == 1.0));
    }

    #[test]
    fn serde_round_trip_skips_derived() {
        let cells = forest_grid(2, 2);
        let m = attach_on(
            WildfireModel::new(base_params(), WildfireEnv::default()),
            2,
            2,
            &cells,
        );
        assert!(!m.derived.p_base.is_empty());
        let json = serde_json::to_string(&m).unwrap();
        let back: WildfireModel = serde_json::from_str(&json).unwrap();
        assert_eq!(back.params, m.params);
        assert_eq!(back.env, m.env);
        assert!(
            back.derived.p_base.is_empty(),
            "derived is rebuilt on attach, not serialized"
        );
        // Defaults fill in for omitted params.
        let minimal: WildfireModel = serde_json::from_str(
            r#"{"params":{"seed":1,"fuels":[{"name":"F","veg_factor":1.0}]}}"#,
        )
        .unwrap();
        assert_eq!(minimal.params.p0, 0.58);
        assert_eq!(minimal.params.c1, 0.045);
        assert_eq!(minimal.params.burn_duration, 1);
    }

    #[test]
    fn typetag_round_trip_as_external_model() {
        let m: Box<dyn ExternalModel> =
            Box::new(WildfireModel::new(base_params(), WildfireEnv::default()));
        let json = serde_json::to_string(&m).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("wildfire").is_some(), "typetag name: {json}");
        let back: Box<dyn ExternalModel> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.typetag_name(), "wildfire");
    }

    fn ctx<'a>(
        cells: &'a [CellType],
        ages: &'a [u32],
        w: usize,
        h: usize,
        step: u64,
    ) -> ChunkCtx<'a> {
        ChunkCtx {
            cells,
            ages,
            start: 0,
            width: w,
            height: h,
            step,
            inactive: CellType::inactive(),
        }
    }

    #[test]
    fn burning_cell_burns_out_after_duration() {
        let f = CellType::new("Forest");
        let b = CellType::new("Burning");
        let cells = vec![b, f, f, f];
        let mut p = base_params();
        p.p0 = 0.0; // isolate the burn-duration logic
        p.burn_duration = 2;
        let m = attach_on(WildfireModel::new(p, WildfireEnv::default()), 2, 2, &cells);
        let mut next = vec![CellType::inactive(); 4];
        // Age 0: keeps burning (0 + 1 < 2).
        let ages = vec![0u32; 4];
        m.step_chunk(&ctx(&cells, &ages, 2, 2, 0), &mut next);
        assert_eq!(next[0], b);
        // Age 1: burns out.
        let ages = vec![1u32; 4];
        m.step_chunk(&ctx(&cells, &ages, 2, 2, 1), &mut next);
        assert_eq!(next[0], CellType::new("BurnedOut"));
        // Burned stays burned; fuel with p0 = 0 never ignites.
        let cells2 = vec![CellType::new("BurnedOut"), f, f, f];
        m.step_chunk(&ctx(&cells2, &ages, 2, 2, 2), &mut next);
        assert_eq!(next[0], CellType::new("BurnedOut"));
        assert_eq!(next[1], f);
    }

    #[test]
    fn certain_ignition_spreads_to_all_neighbors() {
        // p0 = 1, no wind, flat: neighbors of a burning cell ignite for sure
        // (p = 1 for cardinals; diagonals 1/sqrt(2) < 1 so use a seed-independent
        // check only on cardinals).
        let f = CellType::new("Forest");
        let b = CellType::new("Burning");
        let mut cells = forest_grid(3, 3);
        cells[4] = b;
        let mut p = base_params();
        p.p0 = 1.0;
        let m = attach_on(WildfireModel::new(p, WildfireEnv::default()), 3, 3, &cells);
        let ages = vec![0u32; 9];
        let mut next = vec![CellType::inactive(); 9];
        m.step_chunk(&ctx(&cells, &ages, 3, 3, 0), &mut next);
        for idx in [1usize, 3, 5, 7] {
            assert_eq!(next[idx], b, "cardinal neighbor {idx} must ignite at p = 1");
        }
        assert_eq!(next[4], CellType::new("BurnedOut"), "burn_duration 1");
        assert!(next[0] == b || next[0] == f, "diagonal is stochastic");
    }

    #[test]
    fn burned_out_never_reignites() {
        // Regression: p_base is keyed to the initial fuel class and stays
        // positive after burnout, so eligibility must be a type check.
        let b = CellType::new("Burning");
        let mut cells = forest_grid(3, 3);
        cells[4] = CellType::new("BurnedOut"); // was Forest at attach in a real run
        cells[3] = b;
        let mut p = base_params();
        p.p0 = 1.0;
        p.burn_duration = u32::MAX;
        let mut m = attach_on(
            WildfireModel::new(p, WildfireEnv::default()),
            3,
            3,
            &forest_grid(3, 3),
        );
        // Simulate the mid-run state: derived was built when cell 4 was Forest.
        assert!(m.derived.p_base[4] > 0.0);
        m.derived.p_base[3] = 0.0; // cell 3 ignited earlier in the run
        let ages = vec![0u32; 9];
        let mut next = vec![CellType::inactive(); 9];
        m.step_chunk(&ctx(&cells, &ages, 3, 3, 0), &mut next);
        assert_eq!(
            next[4],
            CellType::new("BurnedOut"),
            "burned stays burned at p = 1"
        );
    }

    #[test]
    fn no_fire_no_change_and_zero_prob_never_ignites() {
        let f = CellType::new("Forest");
        let cells = forest_grid(3, 3);
        let m = attach_on(
            WildfireModel::new(base_params(), WildfireEnv::default()),
            3,
            3,
            &cells,
        );
        let ages = vec![0u32; 9];
        let mut next = vec![CellType::inactive(); 9];
        let ev = m.step_chunk(&ctx(&cells, &ages, 3, 3, 0), &mut next);
        assert!(ev.is_empty());
        assert!(
            next.iter().all(|&t| t == f),
            "no burning neighbors, no change"
        );
    }

    #[test]
    fn edge_cells_treat_oob_as_inactive() {
        // Burning cell in a corner: only in-bounds neighbors are read; no panic.
        let b = CellType::new("Burning");
        let mut cells = forest_grid(2, 2);
        cells[0] = b;
        let mut p = base_params();
        p.p0 = 1.0;
        let m = attach_on(WildfireModel::new(p, WildfireEnv::default()), 2, 2, &cells);
        let ages = vec![0u32; 4];
        let mut next = vec![CellType::inactive(); 4];
        m.step_chunk(&ctx(&cells, &ages, 2, 2, 0), &mut next);
        assert_eq!(next[1], b, "cardinal neighbor ignites");
        assert_eq!(next[2], b, "cardinal neighbor ignites");
    }

    #[test]
    fn chunked_evaluation_matches_whole_grid() {
        // Split the same grid into two chunks; per-cell results must be
        // identical to the single-chunk pass (determinism across chunking).
        let b = CellType::new("Burning");
        let mut cells = forest_grid(4, 4);
        cells[5] = b;
        let mut p = base_params();
        p.p0 = 0.6;
        p.wind_speed = 5.0;
        let m = attach_on(WildfireModel::new(p, WildfireEnv::default()), 4, 4, &cells);
        let ages = vec![0u32; 16];
        let mut whole = vec![CellType::inactive(); 16];
        m.step_chunk(&ctx(&cells, &ages, 4, 4, 3), &mut whole);
        let mut lo = vec![CellType::inactive(); 8];
        let mut hi = vec![CellType::inactive(); 8];
        m.step_chunk(&ctx(&cells, &ages, 4, 4, 3), &mut lo);
        let hi_ctx = ChunkCtx {
            cells: &cells,
            ages: &ages[8..],
            start: 8,
            width: 4,
            height: 4,
            step: 3,
            inactive: CellType::inactive(),
        };
        m.step_chunk(&hi_ctx, &mut hi);
        assert_eq!(&whole[..8], &lo[..]);
        assert_eq!(&whole[8..], &hi[..]);
    }

    #[test]
    fn spotting_generates_deterministic_in_bounds_events() {
        let b = CellType::new("Burning");
        let mut cells = forest_grid(9, 9);
        cells[4 * 9 + 4] = b;
        let mut p = base_params();
        p.p0 = 0.0;
        p.burn_duration = u32::MAX; // keep it burning
        p.wind_dir_deg = 0.0;
        p.spotting = Some(SpottingParams {
            p_spot: 1.0,
            median_distance: 3.0,
            sigma: 0.0,
            angle_jitter_deg: 0.0,
        });
        let m = attach_on(WildfireModel::new(p, WildfireEnv::default()), 9, 9, &cells);
        let ages = vec![0u32; 81];
        let mut next = vec![CellType::inactive(); 81];
        let ev = m.step_chunk(&ctx(&cells, &ages, 9, 9, 0), &mut next);
        assert_eq!(ev.len(), 1, "p_spot = 1 with one burning cell");
        // sigma = 0, no jitter, wind toward +x: lands exactly 3 cells east.
        assert_eq!(ev[0].target, 4 * 9 + 7);
        assert_eq!(ev[0].new_type, b);
        // Deterministic: same call, same events.
        let ev2 = m.step_chunk(&ctx(&cells, &ages, 9, 9, 0), &mut next);
        assert_eq!(ev, ev2);
        // Out-of-bounds landing is dropped: aim off the east edge.
        let mut cells_edge = forest_grid(9, 9);
        cells_edge[4 * 9 + 8] = b;
        let m2 = attach_on(
            WildfireModel::new(m.params.clone(), WildfireEnv::default()),
            9,
            9,
            &cells_edge,
        );
        let ev3 = m2.step_chunk(&ctx(&cells_edge, &ages, 9, 9, 0), &mut next);
        assert!(ev3.is_empty(), "landing past the edge is dropped");
    }

    #[test]
    fn event_applies_only_to_fuel() {
        let cells = forest_grid(2, 2);
        let m = attach_on(
            WildfireModel::new(base_params(), WildfireEnv::default()),
            2,
            2,
            &cells,
        );
        let ev = ModelEvent {
            target: 0,
            new_type: CellType::new("Burning"),
        };
        assert!(m.event_applies(CellType::new("Forest"), &ev));
        assert!(
            !m.event_applies(CellType::new("Burning"), &ev),
            "idempotence guard"
        );
        assert!(!m.event_applies(CellType::new("BurnedOut"), &ev));
        assert!(!m.event_applies(CellType::inactive(), &ev));
    }

    #[test]
    fn on_paint_refreshes_p_base_and_declared_types_lists_all() {
        let cells = forest_grid(2, 2);
        let m0 = WildfireModel::new(base_params(), WildfireEnv::default());
        let mut m = attach_on(m0, 2, 2, &cells);
        assert!(m.derived.p_base[0] > 0.0);
        m.on_paint(0, CellType::inactive());
        assert_eq!(m.derived.p_base[0], 0.0);
        m.on_paint(0, CellType::new("Forest"));
        assert!(m.derived.p_base[0] > 0.0);
        m.on_paint(999, CellType::new("Forest")); // out of range: no-op, no panic
        let names: Vec<&str> = m.declared_types().iter().map(|t| t.as_str()).collect();
        assert_eq!(names, vec!["Forest", "Burning", "BurnedOut"]);
    }

    #[test]
    fn custom_state_names_are_honored() {
        let f = CellType::new("Grass");
        let mut cells = vec![f; 9];
        cells[4] = CellType::new("Fire");
        let mut p = base_params();
        p.fuels = vec![FuelClass {
            name: "Grass".into(),
            veg_factor: 1.0,
        }];
        p.p0 = 1.0;
        p.burning_name = Some("Fire".into());
        p.burned_name = Some("Ash".into());
        let m = attach_on(WildfireModel::new(p, WildfireEnv::default()), 3, 3, &cells);
        let ages = vec![0u32; 9];
        let mut next = vec![CellType::inactive(); 9];
        m.step_chunk(&ctx(&cells, &ages, 3, 3, 0), &mut next);
        assert_eq!(next[4], CellType::new("Ash"));
        assert_eq!(next[1], CellType::new("Fire"));
    }
}
