# E9b — rotate the whole wind schedule · finding, not a lever

_Round 2 (2026-09-01) · 3 seeds · calibration fires · runner `exp_wind_rotation.py` · results `exp9_rotation.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** If the wind convention were wrong by a constant angle, one
rotation of the whole wind schedule would win on every fire. We rotated
it by 0°, 90°, 180° and 270°, switched wind off, and repeated at five
times the wind speed. At the real ERA5 speeds every variant scores the
same: the wind is too weak to matter. At ×5 different fires prefer
different rotations, so there is no convention bug. One cell stands out:
Chimney at ×5 rotated 270° scores 0.571, far above anything else, and the
rotation matches newspaper reports of easterly gusts on the run days. The
wind *input* is wrong where it mattered, not the wind maths.

**Question.** Does one rotation win everywhere (a convention bug), and
does wind matter at all at ERA5 speeds?

**What we changed.** Harness hook `EXP_WIND_ROT_DEG` rotates every day's
wind direction by a constant. E1 recipe, 3 seeds, rotations 0/90/180/270°,
wind off (×0), and wind ×5 at each rotation.

**Why we expected it to matter.** A constant convention error would show
as one rotation winning on all four fires.

**How we scored it.** Mean IoU, 3 seeds, four calibration fires.

**Result.**

| Fire | rot 0 | rot 90 | rot 180 | rot 270 | wind off | ×5 rot 0 | ×5 rot 90 | ×5 rot 180 | ×5 rot 270 |
|---|---|---|---|---|---|---|---|---|---|
| Bear | 0.311 | 0.316 | 0.322 | 0.306 | 0.315 | 0.264 | 0.253 | 0.307 | 0.283 |
| Brattain | 0.336 | 0.337 | 0.333 | 0.331 | 0.335 | **0.321** | 0.286 | 0.260 | 0.266 |
| Buck | 0.403 | 0.428 | 0.415 | 0.392 | 0.417 | 0.408 | 0.407 | 0.357 | 0.348 |
| Chimney | 0.441 | 0.427 | 0.453 | 0.474 | 0.445 | 0.298 | 0.278 | 0.456 | **0.571** |

How to read it: mean IoU, higher is better. "rot 0" is the unrotated
control at real speed. The first five columns are at ×1; the last four at
×5 speed. Bold marks the best ×5 rotation for the two fires where one
stands out. Circle: Bear 0.541, Brattain 0.450, Buck 0.670, Chimney 0.372.

- At ERA5 speeds (0.1–3 m/s daily domain means) **wind is inert**:
  rotations and "wind off" all sit within ±0.02 of each other.
  `exp(c1 × V)` at V ≈ 1 is 1.05; the kernel needs V ≳ 5 to matter.
- At ×5 no single rotation wins everywhere (Brattain and Buck prefer 0°,
  Bear 180°, Chimney 270°). A constant offset would show one winner.
  **Not a convention bug.**
- Chimney ×5 rotated 270° scores **0.571 (final 0.597)**, far above its
  ×5 unrotated run (0.298), its ×1 control (0.441) and the Circle (0.372).
  Rotating by −90° turns the ERA5 "toward ENE" wind into "toward WSW".
  Contemporary reporting (New Times SLO, 2016-08-25) says the Aug 20–21
  runs were driven by gusts "out of the east" pushing the fire west, then
  a north-east shift. ERA5's ~1 m/s daily domain mean carries none of
  that.

**What it means.** Two things at once. First, at the speeds we feed it
the wind rule does nothing, so every Round 1 wind result (E5) and every
per-fire p0 was found in a model that effectively had no wind. Second,
when the wind is strong *and* pointed the way the fire really went, the
model can beat the Circle by a wide margin on a wind-driven fire. The
kernel is not useless; it is starved.

**Questions this raises.**

- Is Chimney's 270° a lucky angle? → E9c: no, a broad plateau from 225°
  to 300° at ×3–5 scores 0.50–0.57.
- With perfect daily direction, how much would the kernel buy? → E10: at
  most +0.08, and it hurts the slow fires.
- Can real hourly wind supply what the daily mean lacks? → E14: the
  nearest station is 41 km away in a valley and does not see the
  easterly gusts either.

**Verdict.** Finding. Wind maths right, wind input too weak and, on
Chimney, wrong when it mattered. Codified as TEST_PLAN §2.1.

**Later.** E9c, E10, E14, E19 (front speed barely responds to wind), E26
(terrain wind changes nothing at this kernel), E37 (wind is a speed knob
not a shape knob).
