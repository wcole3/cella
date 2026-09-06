# E5 — wind gust multiplier ×2 / ×4 · REJECTED

_Round 1 (2026-08-15) · 1 seed · calibration fires · runner `exp_variants.py` · raw results not kept in `results/` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** The wind we feed the model is one daily average for the
whole map, and daily averages are gentle, 0.6–1.2 m/s on these fires. At that speed the
model's wind rule does almost nothing. We asked whether multiplying the
wind by 2 or by 4 would put the missing gusts back. It did not help: one
fire gained a little, one lost, the other two did not move, and every
fire's burn got narrower without landing in the right place. Multiplying
a bad wind does not make a good one.

**Question.** Does a stronger version of the same daily-average wind
improve the overlap score?

**What we changed.** One harness knob, `EXP_WIND_SCALE`, multiplies every
day's wind speed by 2 or by 4 before the model sees it. Everything else
stays at the E1 recipe.

**Why we expected it to matter.** The wind rule multiplies the spread
chance by `exp(0.045 × speed)`. At 1 m/s that is 1.05, a 5 % nudge. At
4 m/s it is 1.2. Station records later showed real peaks of 7–14 m/s on
these fires (E14).

**How we scored it.** Mean IoU, higher is better, four calibration fires,
one seed, against the E1 control. We also watched the area ratio
(simulated burned area ÷ observed) to tell "burned less" from "burned in
better places".

**Result.** Change in mean IoU from the E1 control:

| Fire | control | ×2 | ×4 |
|---|---|---|---|
| Bear | 0.317 | −0.02 | −0.04 |
| Brattain | 0.337 | ≈ 0 | ≈ 0 |
| Buck | 0.414 | ≈ 0 | ≈ 0 |
| Chimney | 0.435 | +0.013 | not recorded |

How to read it: at one seed a change under about 0.02 is noise (E33 later
measured the floor). Area ratios fell on every fire as the multiplier
rose; the exact values were not kept.

- Only Chimney, the fast wind-driven fire, gained, and only a little.
- Bear lost more the stronger the wind.
- Every fire burned a narrower shape. The wind rule adds direction by
  taking spread away from the other directions.

**What it means.** Stronger wind makes the model's fire thinner, not more
accurate. First sign of something later experiments pin down: in this
model wind changes how much burns, not how far the fire stretches (E19,
E37). It also says the daily-average input is the wrong shape of data.
Gusts matter for hours, not days, and a multiplier cannot recover hours
from a daily mean.

**Questions this raises.**

- Is the daily wind too weak, pointing the wrong way, or both? → E9b,
  E9c: too weak everywhere, wrong way on Chimney's run days.
- Would hourly or per-cell wind do better than a scalar? → E14 (hourly
  station wind: no), E26 (terrain wind: no change).
- Why does more wind narrow the fire instead of stretching it? → E19,
  E37: the wind rule is a speed knob, not a shape knob.

**Verdict.** Rejected. The real fix is a better wind input (per-cell wind,
roadmap §10.4) and a wind rule that can stretch the fire, not a scalar
multiplier.

**Later.** E9b/E9c (wind inert at ERA5 speeds; Chimney wants a moderate
westward wind the data does not contain), E19 (front speed responds to
wind by 5–10 %), E37 (wind moves fires toward "bigger and rounder" across
the whole knob space).
