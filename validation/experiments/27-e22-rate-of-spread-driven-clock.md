# E22 / E22b — a rate-of-spread-driven clock · REJECTED at daily truth (null when normalised, harmful when not)

_Round 3 (2026-09-02) · 3 seeds · calibration fires · runners `exp_clock.py`, `exp_clock_b.py`, hook `EXP_TICK_SCALE` · results `exp22_clock.json`, `exp22b_clock_unnorm.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** E19 said the tick length should follow the weather. So we
let the clock run faster in windy hours. Two versions: keep the day's
total ticks fixed and just move them to the windy hours (E22), or let
windy hours add ticks (E22b). Moving ticks changed nothing the daily
satellite maps can see. Adding ticks was E11 again: more ticks means
more burn on every fire, because the kernel widens the fire rather than
stretching it. A wind clock only makes sense after the kernel makes wind
stretch the fire.

**Question.** Does a clock that follows hourly wind speed improve the
score?

**What we changed.** Harness hook `EXP_TICK_SCALE`: per-window multiplier
on `steps_per_hour`, ticks accumulated as a float. Ticks per hour ∝
1 + k·U^1.5 with U the hourly station wind (Rothermel's wind exponent is
≈ 1.5–2), k ∈ {0.05, 0.1, 0.2, 0.4} (U = 5 m/s → ×1.6 / 2.1 / 3.2 / 5.5).

- **E22**: multiplier normalised so the run's *total* ticks stay at
  50/day; only the timing moves. On the E17 recipe (p0 ×4, moisture, decay τ5) and on the
  plain E1 recipe.
- **E22b**: no normalisation; windy hours add ticks (mean ×1.4–2.2). On
  the E17 recipe, whose decay caps the total.

**How we scored it.** Mean IoU with area ratio in brackets, 3 seeds, four
calibration fires.

**Result.**

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

How to read it: "recipe" is the E17 recipe; "normalised" rows hold the
total ticks fixed; "un-normalised" rows add ticks. Bold is the best row
per fire.

- **Moving ticks to windy hours is invisible at daily truth** (±0.02,
  both directions). The daily masks cannot tell whether a cell burned at
  14:00 or 02:00. Chimney gains +0.01 at k 0.4; the fast fire is the one
  that would benefit, and only sub-daily truth can show it.
- **Adding ticks is the E11 result again.** More ticks = more burn on
  every fire, even under the decay: area ×1.5–3.6 and IoU down 0.08–0.18.
  The decay caps the *late* burn; extra early ticks explode the *early*
  burn before it acts.

**What it means.** Speed and total burn remain one knob because the
kernel gives wind almost no directional speed (E19: 5–10 % from calm to
8 m/s): extra ticks widen
the fire, they do not stretch it. The tick clock is the right place to
put wind speed only once the kernel makes wind stretch the fire, i.e.
after re-fitting c1/c2 for elongation or with an elliptical spread rule.
Until then a wind clock is a faster circle.

**Questions this raises.**

- Does the kernel stretch the fire anywhere in knob space? → E37: no.
- Does the clock help on hourly truth after a kernel refit? Open (E30,
  then GOFER/PT-FireSprd).

**Verdict.** Rejected at daily cadence. Keep `EXP_TICK_SCALE` for the
sub-daily tiers.

**Later.** E26 (terrain wind, also null at this kernel), E37, E30.
