# E15 — hourly fuel-moisture damping from station RH/T · REJECTED (as tested)

_Round: Round 3 — 2026-09-02: real weather in, explosion out_

**Question.** Operational CA simulators stop the fire at night and on humid
days by multiplying the spread probability with a fuel-moisture factor
(PROPAGATOR, Trucchia et al. 2020, using the Burgan & Rothermel 1984
damping with a moisture of extinction of 0.3). We now have hourly RH and
temperature from the nearest station (E14). Does the textbook damping fix
the explosion?

**Method.** `exp_station.py moisture`, 3 seeds, E1 (p0, dur) recipes,
hourly station wind ×1 throughout. Per hourly window the harness scales
p0 by

    EMC = Fosberg/Simard 1-h fuel moisture (%) from RH and T
    r   = EMC / M_x,   M_x ∈ {25 %, 35 %}
    η   = 1 − 2.59 r + 5.11 r² − 3.52 r³   (Rothermel 1972), clipped to [0, 1]

Mean η over a fire's hours came out 0.59–0.66, so p0 was re-scanned at
×{1, 1.5, 2, 3}. A `night` variant (p0 × 0.3 from 20:00 to 08:00 local,
no moisture) isolates the plain day/night cycle. Hourly p0 changes are
now cheap thanks to the new `WildfireModel::set_p0` (see round-3.md).

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

(mean IoU, area ratio in brackets.)

**Findings.**

- **Periodic damping does not stop the fire; it only slows it.** With the
  E1 p0 the damped fire dies (area ×0.1–0.2). Multiply p0 back up and the
  same explosion returns (×3–5). The best damped run on every fire is at
  or below the undamped ERA5 control. The knife edge is untouched.
- Why, in one sentence: whether a cell *ever* burns depends on whether the
  fire stays above the percolation threshold long enough to reach it;
  night-time or humid hours pause the front but the next dry afternoon
  resumes it, so the reachable set — and the over-burn — is the same.
  A stopping mechanism has to be **cumulative and one-way**, which is what
  E16b tests.
- The Fosberg EMC from a valley airport is also a poor proxy for fuel
  moisture on a ridge 40–70 km away; a RAWS record or a 10-h/100-h fuel
  moisture model would be fairer. But the failure above is structural, not
  an input problem: even a perfect hourly moisture would only modulate,
  not cap.

**Verdict.** Rejected as tested. Keep the moisture code (it is the right
physics for *rate*, and it will matter once sub-daily truth arrives), but
it is not the answer to "burns everything".
