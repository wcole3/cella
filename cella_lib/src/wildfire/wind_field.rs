//! Terrain-adjusted wind fields: a mass-consistent downscaler.
//!
//! Weather data arrives as one wind for a whole area (a station reading or
//! a 30 km reanalysis cell, i.e. a gridded re-computation of past weather),
//! but a fire on a 30 m grid feels ridges and valleys. Operational tools (WindNinja, Forthofer et al. 2014) fix this
//! with a *mass-consistent* diagnostic model: start from the uniform wind,
//! then adjust it as little as possible so that air is conserved (none is
//! created or destroyed) as it flows over the terrain. Air squeezed over a ridge speeds up; air entering a
//! valley is channelled along it. No momentum equation, no thermal effects
//! (WindNinja's known limits: lee-side recirculation and stable-layer
//! decoupling are not captured), but it reproduces the first-order terrain
//! effects in seconds.
//!
//! This is the two-dimensional, single-layer version. Air moves in a layer
//! of depth `h(x, y) = top − z(x, y)`, where `z` is the terrain and `top` a
//! flat lid `layer_depth_m` metres above the highest cell. Conservation of the
//! column flux `h·u` (layer depth times wind) with a correction written as the
//! gradient of a potential, `u = u0 + ∇φ`, gives the variable-coefficient
//! Poisson equation (a standard "spread-out" PDE: it says how `φ` must bend so
//! the flux has no sources or sinks)
//!
//! ```text
//! ∇·(h ∇φ) = −∇·(h u0) = −(u0 ∂h/∂x + v0 ∂h/∂y)
//! ```
//!
//! solved on the cell grid by Gauss–Seidel (sweep the cells repeatedly, each
//! time replacing `φ` with the average implied by its neighbours) with
//! over-relaxation (overshoot each update a little to converge faster).
//! The edges use zero-gradient (Neumann) boundaries (`φ` has no slope
//! across the edge), so the flux through the edges stays `h·u0`. The
//! result is returned as meteorological components: `u` eastward (+x), `v`
//! northward (−y on a north-up grid), ready for
//! [`super::WildfireModel::set_wind_field`].

/// Result of a downscaling: per-cell `u` (eastward) and `v` (northward), m/s.
#[derive(Clone, Debug, PartialEq)]
pub struct WindField {
    /// Eastward wind per cell, row-major, m/s.
    pub u: Vec<f32>,
    /// Northward wind per cell, row-major, m/s.
    pub v: Vec<f32>,
    /// Solver iterations used (sweeps over the grid; both unit solves summed
    /// when the field came from a [`MassConsistentBasis`]).
    pub iterations: usize,
    /// Final maximum absolute update of the potential (m²/s); convergence
    /// measure.
    pub residual: f64,
}

/// Solver settings. The defaults are meant for 30 m grids; the automatic
/// coarsening (see `coarsen`) is what keeps big grids affordable.
#[derive(Clone, Debug)]
pub struct MassConsistentOptions {
    /// Lid height above the highest terrain cell, metres. Smaller = stronger
    /// terrain response (ridges squeeze a thinner layer). WindNinja's default
    /// domain top is a few hundred metres; 300 m is a reasonable start.
    pub layer_depth_m: f64,
    /// Over-relaxation factor, meant to lie in (1, 2) (1 = plain
    /// Gauss–Seidel; 2 or more diverges). 1.8 is the default.
    pub omega: f64,
    /// Stop when the largest potential update in a sweep drops below this
    /// (same units as [`WindField::residual`], m²/s).
    pub tolerance: f64,
    /// Hard cap on sweeps per unit solve; the solver returns whatever it has
    /// reached, so check `residual` against `tolerance`.
    pub max_iterations: usize,
    /// Solve on a grid coarsened by this factor (each coarse cell is the mean
    /// of a `factor × factor` block of the terrain), then interpolate the
    /// correction back to the full grid. 0 = automatic: the smallest factor
    /// that brings the coarse grid to 60 000 cells or fewer, but never more
    /// than 16 (WindNinja itself runs at 100–150 m for fire support).
    /// 1 = full resolution.
    pub coarsen: usize,
}

impl Default for MassConsistentOptions {
    fn default() -> Self {
        MassConsistentOptions {
            layer_depth_m: 300.0,
            omega: 1.8,
            tolerance: 1e-3,
            max_iterations: 2000,
            coarsen: 0,
        }
    }
}

/// The terrain part of the solution, computed once per landscape.
///
/// The correction is linear in the driving wind: `φ = u0·φ_a + vy0·φ_b`, where
/// `φ_a` and `φ_b` solve the Poisson problem for a unit eastward wind and a
/// unit wind along grid +y (southward on a north-up grid; `vy0 = −v0`).
/// Store their gradients and every later wind is one pass of multiply-adds,
/// instead of a fresh iterative solve per weather window.
#[derive(Clone, Debug)]
pub struct MassConsistentBasis {
    width: usize,
    height: usize,
    /// ∂φ_a/∂x, ∂φ_a/∂y, ∂φ_b/∂x, ∂φ_b/∂y on the full grid (grid axes: x right,
    /// y down), dimensionless (multiply by the wind to get m/s).
    ax: Vec<f32>,
    ay: Vec<f32>,
    bx: Vec<f32>,
    by: Vec<f32>,
    /// Sweeps used by the two unit solves, added together.
    pub iterations: usize,
    /// Larger of the two solves' final residuals (see [`WindField::residual`]).
    pub residual: f64,
}

impl MassConsistentBasis {
    /// Solve the two unit problems over `elevation` (`width × height`, m,
    /// row 0 north). Panics if `elevation.len() != width * height`.
    pub fn new(elevation: &[f32], width: usize, height: usize, cell_size_m: f64, opts: &MassConsistentOptions) -> Self {
        let n = width * height;
        assert_eq!(elevation.len(), n, "elevation layer must be width * height");
        let factor = if opts.coarsen == 0 {
            let mut f = 1usize;
            while (width / f) * (height / f) > 60_000 && f < 16 {
                f += 1;
            }
            f
        } else {
            opts.coarsen.max(1)
        };
        let (cw, ch) = (width.div_ceil(factor), height.div_ceil(factor));
        // Coarsen: each coarse cell takes the mean elevation of its block
        // (edge blocks may be smaller; `count` tracks how many cells fell in).
        let mut coarse = vec![0.0f32; cw * ch];
        let mut count = vec![0u32; cw * ch];
        for y in 0..height {
            for x in 0..width {
                let ci = (y / factor) * cw + x / factor;
                coarse[ci] += elevation[y * width + x];
                count[ci] += 1;
            }
        }
        for (c, &k) in coarse.iter_mut().zip(&count) {
            if k > 0 {
                *c /= k as f32;
            }
        }
        let dx = cell_size_m * factor as f64;
        let (ga, ita, ra) = solve_potential(&coarse, cw, ch, dx, 1.0, 0.0, opts);
        let (gb, itb, rb) = solve_potential(&coarse, cw, ch, dx, 0.0, 1.0, opts);
        // Interpolate the coarse gradients to the full grid (bilinear on cell
        // centres; clamped at the edges).
        let up = |g: &[f64]| -> Vec<f32> {
            let mut out = vec![0.0f32; n];
            for y in 0..height {
                let fy = ((y as f64 + 0.5) / factor as f64 - 0.5).clamp(0.0, (ch - 1) as f64);
                let y0 = fy.floor() as usize;
                let y1 = (y0 + 1).min(ch - 1);
                let ty = fy - y0 as f64;
                for x in 0..width {
                    let fx = ((x as f64 + 0.5) / factor as f64 - 0.5).clamp(0.0, (cw - 1) as f64);
                    let x0 = fx.floor() as usize;
                    let x1 = (x0 + 1).min(cw - 1);
                    let tx = fx - x0 as f64;
                    let v = g[y0 * cw + x0] * (1.0 - tx) * (1.0 - ty)
                        + g[y0 * cw + x1] * tx * (1.0 - ty)
                        + g[y1 * cw + x0] * (1.0 - tx) * ty
                        + g[y1 * cw + x1] * tx * ty;
                    out[y * width + x] = v as f32;
                }
            }
            out
        };
        MassConsistentBasis {
            width,
            height,
            ax: up(&ga.0),
            ay: up(&ga.1),
            bx: up(&gb.0),
            by: up(&gb.1),
            iterations: ita + itb,
            residual: ra.max(rb),
        }
    }

    /// The field for a uniform wind `u0` eastward, `v0` northward (m/s). Cheap:
    /// one multiply-add pass over the grid, no solving.
    pub fn field(&self, u0: f64, v0: f64) -> WindField {
        let n = self.width * self.height;
        let vy0 = -v0; // grid y points south
        let mut u = vec![0.0f32; n];
        let mut v = vec![0.0f32; n];
        for i in 0..n {
            // φ = u0·φ_a + vy0·φ_b in grid components.
            let dpdx = u0 * f64::from(self.ax[i]) + vy0 * f64::from(self.bx[i]);
            let dpdy = u0 * f64::from(self.ay[i]) + vy0 * f64::from(self.by[i]);
            u[i] = (u0 + dpdx) as f32;
            v[i] = -(vy0 + dpdy) as f32;
        }
        WindField { u, v, iterations: self.iterations, residual: self.residual }
    }
}

/// Solve ∇·(h∇φ) = −(u0 ∂h/∂x + vy0 ∂h/∂y) on the given grid (grid axes;
/// `vy0` is the southward component, `dx` the cell size in metres). Returns
/// the gradient fields (∂φ/∂x, ∂φ/∂y), iterations and final residual.
/// The layer depth `h` is floored at 1 m so the equation never divides by zero.
#[allow(clippy::type_complexity)]
fn solve_potential(
    elevation: &[f32],
    width: usize,
    height: usize,
    dx: f64,
    u0: f64,
    vy0: f64,
    opts: &MassConsistentOptions,
) -> ((Vec<f64>, Vec<f64>), usize, f64) {
    let n = width * height;
    let zmax = elevation.iter().cloned().fold(f32::MIN, f32::max) as f64;
    let top = zmax + opts.layer_depth_m;
    let h: Vec<f64> = elevation.iter().map(|&z| (top - z as f64).max(1.0)).collect();
    let idx = |x: usize, y: usize| y * width + x;
    // Layer depth at the face between a cell and its east / south neighbour:
    // the harmonic mean 2ab/(a+b), the usual choice for a PDE coefficient that
    // varies cell to cell.
    let hx = |x: usize, y: usize| -> f64 {
        let a = h[idx(x, y)];
        let b = h[idx(x + 1, y)];
        2.0 * a * b / (a + b)
    };
    let hy = |x: usize, y: usize| -> f64 {
        let a = h[idx(x, y)];
        let b = h[idx(x, y + 1)];
        2.0 * a * b / (a + b)
    };
    // First derivative (in cells) of f at index i: central difference inside,
    // one-sided at the two ends, 0 for a one-cell-wide axis.
    let d1 = |f: &dyn Fn(usize) -> f64, i: usize, len: usize| -> f64 {
        if len == 1 {
            0.0
        } else if i == 0 {
            f(1) - f(0)
        } else if i + 1 == len {
            f(i) - f(i - 1)
        } else {
            (f(i + 1) - f(i - 1)) / 2.0
        }
    };
    let mut rhs = vec![0.0f64; n];
    for y in 0..height {
        for x in 0..width {
            let dhdx = d1(&|i| h[idx(i, y)], x, width) / dx;
            let dhdy = d1(&|j| h[idx(x, j)], y, height) / dx;
            rhs[idx(x, y)] = -(u0 * dhdx + vy0 * dhdy);
        }
    }
    let mut phi = vec![0.0f64; n];
    let inv_dx2 = 1.0 / (dx * dx);
    let mut iterations = 0;
    let mut residual = f64::INFINITY;
    while iterations < opts.max_iterations {
        let mut max_delta = 0.0f64;
        for y in 0..height {
            for x in 0..width {
                let i = idx(x, y);
                let mut coef = 0.0;
                let mut acc = 0.0;
                if x > 0 {
                    let c = hx(x - 1, y) * inv_dx2;
                    coef += c;
                    acc += c * phi[idx(x - 1, y)];
                }
                if x + 1 < width {
                    let c = hx(x, y) * inv_dx2;
                    coef += c;
                    acc += c * phi[idx(x + 1, y)];
                }
                if y > 0 {
                    let c = hy(x, y - 1) * inv_dx2;
                    coef += c;
                    acc += c * phi[idx(x, y - 1)];
                }
                if y + 1 < height {
                    let c = hy(x, y) * inv_dx2;
                    coef += c;
                    acc += c * phi[idx(x, y + 1)];
                }
                if coef == 0.0 {
                    continue;
                }
                let new = (acc - rhs[i]) / coef;
                let delta = new - phi[i];
                phi[i] += opts.omega * delta;
                max_delta = max_delta.max(delta.abs());
            }
        }
        iterations += 1;
        residual = max_delta;
        if max_delta < opts.tolerance {
            break;
        }
    }
    let mut gx = vec![0.0f64; n];
    let mut gy = vec![0.0f64; n];
    for y in 0..height {
        for x in 0..width {
            gx[idx(x, y)] = d1(&|i| phi[idx(i, y)], x, width) / dx;
            gy[idx(x, y)] = d1(&|j| phi[idx(x, j)], y, height) / dx;
        }
    }
    ((gx, gy), iterations, residual)
}

/// Downscale a uniform wind (`u0` eastward, `v0` northward, m/s) over the
/// terrain `elevation` (row-major, `width × height`, metres, row 0 north)
/// with cell edge `cell_size_m`. For many winds over the same terrain, build
/// a [`MassConsistentBasis`] once and call its `field` instead.
///
/// Flat terrain returns the uniform wind exactly. Speed is highest where the
/// layer is thinnest (ridge crests) and lowest where it is thickest (valley
/// floors); direction bends to follow flux-conserving paths around
/// obstacles. An empty grid returns an empty field. Panics if the layer length
/// does not match the grid.
pub fn mass_consistent(
    elevation: &[f32],
    width: usize,
    height: usize,
    cell_size_m: f64,
    u0: f64,
    v0: f64,
    opts: &MassConsistentOptions,
) -> WindField {
    if width * height == 0 {
        return WindField { u: vec![], v: vec![], iterations: 0, residual: 0.0 };
    }
    MassConsistentBasis::new(elevation, width, height, cell_size_m, opts).field(u0, v0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_terrain_returns_the_uniform_wind() {
        let elev = vec![100.0f32; 20 * 10];
        let f = mass_consistent(&elev, 20, 10, 30.0, 3.0, -1.5, &MassConsistentOptions::default());
        assert!(f.u.iter().all(|&x| (x - 3.0).abs() < 1e-6));
        assert!(f.v.iter().all(|&x| (x + 1.5).abs() < 1e-6));
        assert!(f.iterations <= 2, "nothing to correct");
    }

    #[test]
    fn a_ridge_across_the_wind_speeds_the_air_up_on_the_crest() {
        // Terrain rises from 0 to 200 m at the middle column and back down;
        // wind from the west. Flux h·u must be conserved along each row, so
        // the crest (thinnest layer) carries the fastest wind.
        let (w, h) = (61usize, 5usize);
        let mut elev = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let d = (x as f32 - 30.0).abs();
                elev[y * w + x] = (200.0 - 10.0 * d).max(0.0);
            }
        }
        let opts = MassConsistentOptions { layer_depth_m: 200.0, tolerance: 1e-6, max_iterations: 20_000, coarsen: 1, ..Default::default() };
        let f = mass_consistent(&elev, w, h, 30.0, 5.0, 0.0, &opts);
        let row = 2;
        let crest = f.u[row * w + 30];
        let foot = f.u[row * w + 2];
        assert!(crest > 1.5 * foot, "crest {crest} should be much faster than the foot {foot}");
        // Flux conservation: h·u roughly constant along the row.
        let top = 200.0 + 200.0;
        let flux = |x: usize| (top - elev[row * w + x] as f64) * f.u[row * w + x] as f64;
        let (f_foot, f_crest) = (flux(5), flux(30));
        assert!((f_crest - f_foot).abs() / f_foot < 0.15, "flux foot {f_foot} vs crest {f_crest}");
        assert!(f.v.iter().all(|&x| x.abs() < 0.5), "a ridge square to the wind adds no cross-wind");
    }

    #[test]
    fn an_isolated_hill_deflects_the_wind_around_its_flanks() {
        let (w, h) = (41usize, 41usize);
        let mut elev = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let r = ((x as f32 - 20.0).powi(2) + (y as f32 - 20.0).powi(2)).sqrt();
                elev[y * w + x] = (250.0 - 25.0 * r).max(0.0);
            }
        }
        let opts = MassConsistentOptions { layer_depth_m: 150.0, tolerance: 1e-6, max_iterations: 20_000, coarsen: 1, ..Default::default() };
        let f = mass_consistent(&elev, w, h, 30.0, 5.0, 0.0, &opts);
        // Upwind of the hill the flow splits: north of centre it acquires a
        // northward component, south of centre a southward one.
        let north = f.v[(20 - 6) * w + 12];
        let south = f.v[(20 + 6) * w + 12];
        assert!(north > 0.05 && south < -0.05, "north {north} south {south}");
        // Speed on the summit exceeds the free wind.
        let summit = (f.u[20 * w + 20].powi(2) + f.v[20 * w + 20].powi(2)).sqrt();
        assert!(summit > 5.5, "summit speed {summit}");
    }

    #[test]
    fn basis_reproduces_the_direct_solve_and_coarsening_keeps_the_ridge_speed_up() {
        let (w, h) = (61usize, 9usize);
        let mut elev = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                elev[y * w + x] = (200.0 - 10.0 * (x as f32 - 30.0).abs()).max(0.0);
            }
        }
        let full = MassConsistentOptions { tolerance: 1e-6, max_iterations: 20_000, coarsen: 1, ..Default::default() };
        let basis = MassConsistentBasis::new(&elev, w, h, 30.0, &full);
        let direct = mass_consistent(&elev, w, h, 30.0, 3.0, 2.0, &full);
        let via = basis.field(3.0, 2.0);
        for i in 0..w * h {
            assert!((direct.u[i] - via.u[i]).abs() < 1e-4 && (direct.v[i] - via.v[i]).abs() < 1e-4);
        }
        let coarse = MassConsistentOptions { tolerance: 1e-6, max_iterations: 20_000, coarsen: 3, ..Default::default() };
        let c = mass_consistent(&elev, w, h, 30.0, 5.0, 0.0, &coarse);
        let crest = c.u[4 * w + 30];
        let foot = c.u[4 * w + 2];
        assert!(crest > 1.3 * foot, "coarse crest {crest} vs foot {foot}");
    }

    #[test]
    #[should_panic(expected = "width * height")]
    fn wrong_layer_length_panics() {
        mass_consistent(&[0.0; 5], 2, 2, 30.0, 1.0, 0.0, &MassConsistentOptions::default());
    }
}
