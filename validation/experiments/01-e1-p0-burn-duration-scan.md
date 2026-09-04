# E1 — p0 × burn_duration scan · KEPT (the big lever)

_Round: Round 1 — 2026-08-15_

Coarse grid: p0 ∈ {0.08, 0.12, 0.16, 0.22, 0.30} × burn_duration
∈ {2, 5, 10}, both inside the pre-registered search space (§6).
`exp_sweep.py`, 60 single-seed runs.

| Fire | best (p0, dur) | mean IoU | Circle | note |
|---|---|---|---|---|
| Bear | 0.12, 10 | 0.317 | 0.541 | was ~0.12 final uncalibrated |
| Brattain | 0.22, 10 | 0.337 | 0.450 | |
| Buck | 0.16, 5 | 0.414 | 0.670 | under-burns (×0.7) at best point |
| Chimney | 0.30, 5 | 0.435 | 0.372 | **beats the Circle** |

Findings: burn_duration matters as much as p0 (dur = 2 kills every fire
— the front outruns its own fuel); optimal p0 spans 0.12–0.30 across
fires, so one global setting costs ~0.07 mean IoU (global best
p0 = 0.22/dur = 5 averages 0.334 vs 0.376 for per-fire bests).
