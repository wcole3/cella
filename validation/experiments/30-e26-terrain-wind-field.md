# E26 — terrain-adjusted wind field (mass-consistent downscaling) · NULL at this kernel — infrastructure KEPT

_Round 4 (2026-09-04) · 3 seeds · calibration fires · runner `exp_windfield.py`, hook `EXP_WIND_FIELD` · results `exp26_windfield.json` · sources `research-decline-wind-suppression.md` §2 · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** One wind for a 12 × 8 km fire is wrong on every ridge and
in every canyon. We built the standard fix, a mass-conserving downscaler
(WindNinja; Forthofer et al. 2014) that speeds wind over ridges and channels it through
valleys, and gave the model a per-cell wind. It changed nothing
measurable: ±0.01 at real wind, ±0.02 at three times the wind. The
downscaled fields look right; the kernel's speed responds to wind by only
5–10 % (E19), so a wind that varies by 50 % across the terrain changes the
local rate by a few percent. Fix the kernel first.

**Question.** Does a terrain-aware wind field improve the score?

**What we changed.** New `cella_lib::wind_field`: the two-dimensional
single-layer mass-consistent model (conserve the column flux h·u over the
terrain). `WildfireModel::set_wind_field` takes an 8-per-cell factor
table. The basis trick (solve two unit problems once on a coarsened grid,
combine per window) brought a Bear run from 5 minutes to 6 seconds.
Harness hook `EXP_WIND_FIELD=<layer depth>` ∈ {150, 300, 600} m vs
uniform, on the plain E1 recipe and on the τ 5 decay recipe (E16b), with
ERA5 daily wind ×1 and ×3.

**How we scored it.** Mean IoU, 3 seeds, four calibration fires.

**Result.**

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

How to read it: mean IoU; each "terrain" cell lists three layer depths.
Compare each terrain row with the uniform row above it.

- **Terrain wind changes nothing measurable**: ±0.01 at ×1, ±0.02 at ×3,
  in both directions, on every fire and at every layer depth. The
  downscaled fields themselves are sensible (ridge speed-up and valley
  channelling visible in the test cases).
- **It is the E19 result again.** Direction differences do reach the
  kernel (E9b showed a 90° rotation can matter at ×5), but with ERA5
  magnitudes the per-cell directions barely move the eight factors.
- Buck, the steepest terrain in the set, is the one fire with a
  consistent small gain, +0.01 at every depth on the plain recipe.
- Layer depth 150–600 m is irrelevant at this sensitivity; 300 m is the
  default.

**What it means.** The order of operations is now clear: first give the
kernel a real wind–rate response (E30: refit c1 or an elliptical rule),
then the terrain field and the ROS clock (E22) become testable. Until
then any wind-field work, however physical, is invisible.

**Questions this raises.**

- Does the kernel respond to wind anywhere in knob space? → E37: it
  changes size, not shape.
- Is Buck's +0.01 real? Open; below the noise floor E33 later measured.

**Verdict.** Null as a score; kept as infrastructure.

**Later.** E37, E30 (not yet run).
