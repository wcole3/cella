# E10 — wind-direction ORACLE (upper bound, cheats) · finding

_Round 2 (2026-09-01) · 3 seeds · calibration fires · runner `exp_wind_oracle.py` · results `exp10_oracle.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** Suppose we had perfect daily wind direction. How much would
the wind rule buy? We cheated on purpose: each day's wind was replaced by
the direction the real fire grew that day, at a fixed speed. The answer is
"not much": at most +0.08 on Chimney, and Bear and Buck got *worse*,
because stronger wind narrows the burn while p0 was tuned for the weak
wind. Better wind data alone is not the next step.

**Question.** What is the ceiling on what this wind rule can do with
perfect direction?

**What we changed.** Each day's wind direction := direction from the
previous burned set to the centroid of that day's new burn (read from the
truth, so never a reportable score), at speed V = 2, 5 or 8 m/s. Calm
days keep ERA5. E1 recipe, 3 seeds.

**How we scored it.** Mean IoU, four calibration fires, against the ERA5
control and the Circle.

**Result.**

| Fire | ERA5 | oracle V=2 | V=5 | V=8 | Circle |
|---|---|---|---|---|---|
| Bear | 0.311 | 0.301 | 0.277 | 0.259 | 0.541 |
| Brattain | 0.336 | 0.345 | 0.346 | 0.341 | 0.450 |
| Buck | 0.403 | 0.396 | 0.381 | 0.364 | 0.670 |
| Chimney | 0.441 | 0.464 | 0.491 | **0.524** | 0.372 |

How to read it: mean IoU, higher is better. "ERA5" is the honest control.
Every oracle column peeks at the truth. Bold is the best oracle cell.

- Chimney gains up to +0.08 with perfect direction at 8 m/s.
- Bear and Buck lose more the stronger the oracle wind. Bear's area ratio
  goes from ×3.4 to ×0.67: the wind narrows the burn.
- Brattain barely moves.

**What it means.** Even a perfect direction is worth little, because the
kernel converts wind into a narrower fire, not a fire that reaches
further. The centroid direction is also a crude oracle: Chimney's
rotated-ERA5 run in E9b (0.571) beat it. The shapes we are missing are
not reachable through the wind rule as it stands; see the front-speed
note, E11 and E12 below.

**Questions this raises.**

- Why does wind narrow the burn instead of stretching it? → Front-speed
  note, E19: the downwind front is already pinned at the speed cap and
  wind changes speed by only 5–10 %.
- Would per-cell wind (terrain) change this? → E26: no, at this kernel.

**Verdict.** Finding. Better wind data alone is not the next step; the
kernel cannot express the shapes we are missing.

**Later.** Front-speed note, E12, E19, E26, E37.
