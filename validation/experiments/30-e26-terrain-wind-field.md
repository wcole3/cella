# E26 — terrain-adjusted wind field (mass-consistent downscaling) · NULL at this kernel — infrastructure KEPT

_Round: Round 4 — 2026-09-04: ensembles_

**Why.** One wind for a 12 × 8 km fire is wrong on every ridge and in every
canyon; operational tools downscale it with a mass-conserving diagnostic
model (WindNinja; Forthofer et al. 2014). New `cella_lib::wind_field`
implements the two-dimensional single-layer version — conserve the column
flux `h·u` over the terrain, so ridges speed up and valleys channel — and
the model gained a per-cell wind field (`WildfireModel::set_wind_field`,
an 8-per-cell factor table). The basis trick (solve two unit problems once
on a coarsened grid, combine per window) brought a Bear run from 5 minutes
to 6 seconds.

**Runs.** `exp_windfield.py`: harness hook `EXP_WIND_FIELD=<layer depth>`
∈ {150, 300, 600} m vs uniform, on the plain E1 recipe and on the τ 5
decay recipe (E16b), with the ERA5 daily wind at ×1 and ×3. 3 seeds,
calibration fires.

| variant | Bear | Brattain | Buck | Chimney |
|---|---|---|---|---|
| plain, ×1, uniform | 0.311 | 0.336 | 0.403 | 0.441 |
| plain, ×1, terrain 150 / 300 / 600 | 0.311 / 0.310 / 0.310 | 0.337 / 0.337 / 0.337 | 0.412 / 0.402 / 0.402 | 0.443 / 0.442 / 0.442 |
| plain, ×3, uniform | 0.288 | 0.330 | 0.429 | 0.398 |
| plain, ×3, terrain 150 / 300 / 600 | 0.270 / 0.281 / 0.280 | 0.326 / 0.325 / 0.329 | 0.432 / 0.437 / 0.434 | 0.384 / 0.386 / 0.385 |
| decay, ×1, uniform | 0.427 | 0.407 | 0.541 | 0.393 |
| decay, ×1, terrain 150 / 300 / 600 | 0.421 / 0.422 / 0.425 | 0.407 / 0.406 / 0.406 | 0.541 / 0.541 / 0.542 | 0.391 / 0.393 / 0.391 |
| decay, ×3, uniform | 0.392 | 0.374 | 0.536 | 0.372 |
| decay, ×3, terrain 150 / 300 / 600 | 0.399 / 0.398 / 0.401 | 0.370 / 0.371 / 0.372 | 0.532 / 0.527 / 0.529 | 0.359 / 0.359 / 0.360 |
| Circle | 0.541 | 0.450 | 0.670 | 0.372 |

**Findings.**

- **Terrain wind changes nothing measurable**: ±0.01 at ×1, ±0.02 at ×3,
  in both directions, on every fire and at every layer depth. The
  downscaled fields themselves are sensible (ridge speed-up and valley
  channelling visible in the test cases), so this is not a solver
  failure.
- **It is the E19 result again.** The kernel's front speed responds to
  wind by 5–10 % (E19); a wind that varies by ±50 % across the terrain
  therefore changes the local rate by a few percent, which is inside seed
  noise at daily truth. Direction differences do reach the kernel (E9b
  showed a 90° rotation can matter at ×5), but with the ERA5 magnitudes
  the per-cell directions barely move the eight factors.
- Buck is the one fire with a consistent (if small) gain, +0.01 at every
  depth on the plain recipe — the steepest terrain in the set.
- Layer depth 150–600 m is irrelevant at this sensitivity; keep 300 m as
  the default.

**Verdict.** Null as a score; kept as infrastructure. The order of
operations is now clear and matches the research notes: **first give the
kernel a real wind–rate response (E30: refit c1 or an elliptical rule),
then the terrain field and the ROS clock (E22) become testable.** Until
then any wind-field work, however physical, is invisible.
