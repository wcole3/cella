# E15 — hourly fuel-moisture damping from station RH/T · REJECTED (as tested)

_Round 3 (2026-09-02) · 3 seeds · calibration fires · runner `exp_station.py moisture` · results `exp15_moisture.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** Operational simulators slow the fire at night and on humid
days by multiplying spread by a fuel-moisture factor. With hourly humidity
and temperature from E14 we could do the textbook version. It does not
fix the explosion. With the E1 p0 the damped fire dies; turn p0 back up
and the same over-burn returns. A pause is not a stop: whatever the fire
can reach on a dry afternoon, it reaches eventually. A stopping mechanism
has to be cumulative and one-way, which E16 tests.

**Question.** Does hourly fuel-moisture damping cap the burn?

**What we changed.** Per hourly window the harness scales p0 by

    EMC = Fosberg/Simard 1-h fuel moisture (%) from RH and T
    r   = EMC / M_x,   M_x ∈ {25 %, 35 %}
    η   = 1 − 2.59 r + 5.11 r² − 3.52 r³   (Rothermel 1972), clipped to [0, 1]

Mean η over a fire's hours came out 0.59–0.66, so p0 was re-scanned at
×{1, 1.5, 2, 3}. A `night` variant (p0 × 0.3 from 20:00 to 08:00 local,
no moisture) isolates the plain day/night cycle. Hourly station wind ×1
throughout. Hourly p0 changes are cheap thanks to the new
`WildfireModel::set_p0` ([round-3.md](round-3.md)).

**Why we expected it to matter.** PROPAGATOR (Trucchia et al. 2020) uses
exactly this damping (Burgan & Rothermel 1984, extinction moisture 0.3)
and is operational in Italy.

**How we scored it.** Mean IoU with area ratio in brackets, 3 seeds, four
calibration fires, against the station ×1 control and the ERA5 E1 best.

**Result.**

| variant | Bear | Brattain | Buck | Chimney |
|---|---|---|---|---|
| station ×1 (control) | 0.292 (×2.9) | 0.320 (×3.3) | 0.360 (×0.3) | 0.443 (×1.8) |
| night ×1 | 0.180 (×0.1) | 0.265 (×1.5) | 0.280 (×0.2) | 0.176 (×0.1) |
| night ×1.5 | 0.263 (×1.6) | 0.313 (×3.0) | 0.339 (×0.3) | 0.392 (×1.3) |
| night ×2 | 0.280 (×3.5) | 0.331 (×3.7) | 0.373 (×0.3) | 0.432 (×2.0) |
| moist M_x 25 ×1 | 0.195 (×0.1) | 0.285 (×1.7) | 0.293 (×0.2) | 0.229 (×0.2) |
| moist 25 ×1.5 | 0.275 (×2.2) | 0.323 (×3.1) | 0.350 (×0.3) | 0.305 (×0.3) |
| moist 25 ×2 | 0.306 (×3.7) | 0.327 (×3.8) | 0.361 (×0.4) | 0.348 (×0.4) |
| moist 25 ×3 | 0.293 (×5.6) | 0.312 (×4.3) | 0.275 (×4.2) | 0.375 (×0.4) |
| moist 35 ×1.5 | 0.277 (×2.7) | 0.325 (×3.3) | 0.355 (×0.3) | 0.432 (×1.7) |
| moist 35 ×2 | 0.305 (×4.0) | 0.327 (×4.0) | 0.358 (×0.7) | 0.443 (×2.3) |
| ERA5 daily (E1 best) | 0.311 | 0.336 | 0.403 | 0.441 |
| Circle | 0.541 | 0.450 | 0.670 | 0.372 |

How to read it: mean IoU with the area ratio in brackets. "×1" after a
variant is the p0 multiplier that compensates the damping. Read each
column top to bottom: as the multiplier rises, the area ratio climbs back
from "died" to "exploded" without the score ever beating the ERA5 row.

- **Periodic damping does not stop the fire; it only slows it.** With
  the E1 p0 the damped fire dies (area ×0.1–0.2). Multiply p0 back up and
  the same explosion returns (×3–5). The best damped run on every fire is
  at or below the undamped ERA5 control.
- Why, in one sentence: whether a cell *ever* burns depends on whether
  the fire stays above the percolation threshold long enough to reach it;
  humid hours pause the front but the next dry afternoon resumes it, so
  the reachable set is the same.
- The Fosberg moisture from a valley airport is also a poor proxy for a
  ridge 40–70 km away; a RAWS record or a 10-h/100-h fuel-moisture model
  would be fairer. But the failure is structural: even perfect hourly
  moisture would only modulate, not cap.

**What it means.** The right physics for *rate* is not the answer to
"burns everything". Any fix for the over-burn has to be cumulative and
one-way.

**Questions this raises.**

- What mechanism is one-way? → E16b: a monotone decay of p0 (biggest gain
  in the log); E28: a daily containment roll.
- Does the moisture cycle earn its keep once the fire can stop? → E17:
  costs nothing and slightly improves the final map.
- Would it matter with hourly truth? Open; the GOFER and PT-FireSprd
  tiers are where timing becomes the metric.

**Verdict.** Rejected as tested. Keep the moisture code.

**Later.** E16, E17, E20 (the optimiser switched moisture off on three of
four fires because daily truth cannot see it), Round 3 conclusion 2.
