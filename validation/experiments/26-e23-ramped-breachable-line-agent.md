# E23 — the fire-line agent with ramping resources, breachable line and anchor-and-flank tactics · REJECTED (still no middle ground)

_Round 3 (2026-09-02) · 3 seeds · calibration fires · runner `exp_fireline2.py`, hooks `EXP_LINE_RAMP_DAYS`, `EXP_LINE_TYPE`, `EXP_LINE_TACTIC` · results `exp23_fireline2.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** E18's crew was all-or-nothing. We added the three fixes
E18 asked for: crews ramp up over three days, the line is hard-to-burn
fuel rather than a perfect wall, and it is built on the upwind side
first. Still binary. A perfect line strangles; a breachable line is
crossed as if it were not there. The one case that lands at the right
area (Bear) has a *worse* score than no line, because the line stopped
the fire in the wrong places. Rule-based crews are parked.

**Question.** Do realistic tactics give a line agent a middle ground
between fence and nothing?

**What we changed.** Resources ramp as 1 − e^(−t/3 d); the line is a
low-flammability fuel class `Line` (veg_factor 0.1) instead of Inactive,
so wind and slope can breach it; the edge is built from the up-wind side
of the fire centre first, the head last. One change at a time at 1000
cells/day, then all three at 300 / 1000 / 3000 cells/day with p0 ×1 and
×2. ERA5 daily wind, E1 recipe.

**How we scored it.** Mean IoU with area ratio in brackets, 3 seeds, four
calibration fires.

**Result.**

| variant | Bear | Brattain | Buck | Chimney |
|---|---|---|---|---|
| ctrl | 0.311 (×3.4) | 0.336 (×3.7) | 0.403 (×0.6) | 0.441 (×2.1) |
| 1000/day, ramp only | 0.258 (×0.2) | 0.184 (×0.2) | 0.413 (×0.4) | 0.366 (×0.7) |
| 1000/day, breachable only | 0.243 (×0.8) | 0.321 (×3.4) | 0.413 (×0.4) | 0.437 (×2.0) |
| 1000/day, upwind only | 0.187 (×0.1) | 0.056 (×0.0) | 0.374 (×0.3) | 0.210 (×0.1) |
| 300/day, all three | 0.262 (×2.8) | 0.338 (×3.6) | **0.434** (×0.4) | 0.444 (×2.1) |
| 1000/day, all three | 0.273 (×0.9) | 0.338 (×3.6) | 0.426 (×0.4) | 0.443 (×2.0) |
| 3000/day, all three | 0.269 (×0.9) | 0.323 (×3.1) | 0.419 (×0.4) | 0.445 (×1.7) |
| all three, p0 ×2 (any rate) | 0.24–0.26 (×4.5–7) | 0.31 (×4.3) | 0.20–0.24 (×5) | 0.41 (×3.5) |
| Circle | 0.541 | 0.450 | 0.670 | 0.372 |

How to read it: mean IoU with area ratio in brackets. "ramp only" etc.
change one thing from E18; "all three" combine them. Bold is the one cell
above its control.

- **A perfect line strangles, a breachable line is ignored.** Inactive
  line (ramp-only, upwind-only) closes the ring and kills the fire as in
  E18. Line at veg_factor 0.1 is crossed on Brattain and Chimney (area
  ×3.4–3.6, same as control).
- Bear is the one fire where the combination lands at the right area
  (×0.9), and its IoU is *worse* than control (0.27 vs 0.31): the line
  stopped the fire in the wrong places.
- **Doubling p0 to compensate blows through every line.** Area ×4–7.
- Ranking by up-wind-ness is not "anchor and flank": with a 1 m/s ERA5
  wind the up-wind side is nearly arbitrary, so the agent still paints a
  near-complete ring. Real crews stop building where the line will not
  hold; this agent has no notion of fire intensity.

**What it means.** Three iterations (E18, E23) have not produced a curve
between fence and nothing. There is a fuel-factor sweet spot between 0.1
(ignored) and 0 (perfect) that was not scanned, but the Bear result says
even a line that holds in the right *amount* holds in the wrong
*places*, so tuning that factor would fix area, not shape. A rule-based
agent needs an intensity model to decide where line holds, which this CA
does not have. The repaint mechanism stays for *observed* daily lines
when we have them.

**Questions this raises.**

- Is the material (a multiplier that dries out) the missing piece? →
  E27: no, same binary outcome.
- Where would observed line and drop locations come from? Open; incident
  GIS perimeter polygons per day.
- Can stopping be handled without a "where"? → E21 (containment fraction
  as a multiplier: no), E28 (containment as a daily probability per
  ensemble member: yes).

**Verdict.** Rejected. Parked.

**Later.** E27, E28.
