# E11 / E11b — joint (steps/day, p0, burn_duration) scan · REJECTED (cap is not the binding limit)

_Round: Round 2 — 2026-09-01: is the wind right, and why is the model too round?_

The fair version of E6: steps/day ∈ {50, 100, 200, 400}, p0 re-scanned
at each rate (0.04–0.30, extended down to 0.01 for 200/400 in E11b because
0.04 already burns everything there), burn duration held in *hours*
(2.4 h, 4.8 h → ticks). Single seed, `exp_timeres.py` +
`exp_timeres_lowp0.py`. Best mean IoU per rate at dur 4.8 h, with the p0
that achieved it:

| Fire | 50/day | 100/day | 200/day | 400/day | Circle |
|---|---|---|---|---|---|
| Bear | **0.317** (0.12) | 0.287 (0.08) | 0.264 (0.03) | 0.275 (0.015) | 0.541 |
| Brattain | **0.337** (0.22) | 0.329 (0.10) | 0.325 (0.04) | 0.318 (0.02) | 0.450 |
| Buck | 0.401 (0.10) | **0.432** (0.04) | 0.408 (0.02) | 0.424 (0.015) | 0.670 |
| Chimney | 0.430 (0.30) | **0.440** (0.12) | 0.437 (0.06) | 0.432 (0.03) | 0.372 |

Findings:

- **Raising the cap 8× moves every fire by less than ±0.03.** The best
  p0 scales as 1/steps (Bear 0.12 → 0.015), i.e. the model is close to
  rate-invariant: p0 × steps/day is the real knob, and it sets speed and
  total burn *together*. Letting the front move faster on a day only
  makes it burn more on every day.
- The percolation cliff gets *sharper* with more ticks: at 400/day
  Brattain goes from ×0.1 area (p0 0.010) to ×2.4 (p0 0.015).
- dur 2.4 h is worse than 4.8 h at every rate (as E1 found).

So the observed 2–7 km/day advances are not missing because of a tick
budget; they are missing because a constant-p0 CA has one speed. Real
fires have fast days and stopped days. That points at time-varying
drivers, not resolution — tested next.
