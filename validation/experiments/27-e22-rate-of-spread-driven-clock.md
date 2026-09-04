# E22 / E22b — a rate-of-spread-driven clock · REJECTED at daily truth (null when normalised, harmful when not)

_Round: Round 3 — 2026-09-02: real weather in, explosion out_

**Why.** E19: the model's front speed responds to wind by 5–10 % where
reality responds by 2–3×, and E11: a constant tick rate cannot fix it. So
let the clock follow the weather. New harness hook `EXP_TICK_SCALE`
(per-window multiplier on `steps_per_hour`, ticks accumulated as a float).
Ticks per hour ∝ 1 + k·U^1.5 with U the hourly station wind (Rothermel's
wind exponent is ≈ 1.5–2), k ∈ {0.05, 0.1, 0.2, 0.4}
(U = 5 m/s → ×1.6 / 2.1 / 3.2 / 5.5).

- **E22**: multiplier normalised so the run's *total* ticks stay at 50/day
  — only the timing of ticks moves to windy hours. On the E17 recipe (p0 ×4,
  moisture, decay τ5) and on the plain E1 recipe. `exp_clock.py`.
- **E22b**: no normalisation — windy hours add ticks (mean ×1.4–2.2) — on
  the E17 recipe, whose decay caps the total. `exp_clock_b.py`.

3 seeds, calibration fires.

| variant | Bear | Brattain | Buck | Chimney |
|---|---|---|---|---|
| recipe, constant clock | 0.418 (×0.8) | **0.384** (×1.0) | **0.502** (×0.8) | 0.390 (×0.7) |
| recipe, clock k 0.1 (normalised) | 0.428 (×0.8) | 0.374 (×1.0) | 0.483 (×0.7) | 0.400 (×0.6) |
| recipe, clock k 0.4 (normalised) | **0.437** (×0.8) | 0.364 (×0.9) | 0.473 (×0.6) | 0.391 (×0.6) |
| plain E1, constant clock | 0.288 (×2.9) | 0.320 (×3.3) | 0.360 (×0.3) | 0.444 (×1.8) |
| plain E1, clock k 0.4 (normalised) | 0.277 (×2.8) | 0.321 (×3.1) | 0.359 (×0.3) | **0.455** (×1.6) |
| recipe, clock k 0.1 (un-normalised, ×1.4–1.6 ticks) | 0.339 (×1.5) | 0.254 (×2.6) | 0.463 (×1.0) | 0.410 (×1.6) |
| recipe, clock k 0.2 (un-normalised, ×1.9–2.2 ticks) | 0.284 (×2.0) | 0.206 (×3.6) | 0.404 (×1.2) | 0.312 (×2.5) |
| Circle | 0.541 | 0.450 | 0.670 | 0.372 |

**Findings.**

- **Moving ticks to windy hours is invisible at daily truth** (±0.02,
  both directions). Expected in hindsight: the daily masks cannot tell
  whether a cell burned at 14:00 or 02:00. Chimney gains +0.01 at k 0.4 —
  the fast fire is the one that would benefit, and only sub-daily truth
  can show it.
- **Adding ticks is the E11 result again.** More ticks = more burn on
  every fire, even under the decay: area ×1.5–3.6 and IoU down 0.08–0.18.
  The decay caps the *late* burn; extra early ticks explode the *early*
  burn before it acts. Speed and total burn remain one knob because the
  kernel gives wind almost no directional speed (E19): extra ticks widen
  the fire, they do not stretch it.
- Therefore the tick clock is the *right* place to put wind speed only
  once the kernel makes wind stretch the fire — i.e. after re-fitting c1/c2
  for elongation, or with an elliptical (Huygens-style) spread rule. Until
  then a wind clock is a faster circle.

**Verdict.** Rejected at daily cadence. Keep `EXP_TICK_SCALE`: it is the
mechanism for the ROS-driven clock and for GOFER/PT-FireSprd hourly
scoring, where it should be re-tested together with a kernel refit.
