# E13 — daily temperature schedule × tick rate · schedule KEPT, rate REJECTED

_Round: Round 2 — 2026-09-01: is the wind right, and why is the model too round?_

`exp_sched_timeres.py`: 50 vs 200 steps/day, with and without the E3
`temp_vpd` schedule, at the E11 best p0 for each rate, dur 4.8 h, 3
seeds. Mean IoU:

| Fire | 50 const | 50 + temp | 200 const | 200 + temp |
|---|---|---|---|---|
| Bear | 0.311 | 0.313 | 0.263 | 0.271 |
| Brattain | 0.336 | 0.339 | 0.327 | 0.324 |
| Buck | 0.427 | **0.485** | 0.420 | 0.476 |
| Chimney | 0.437 | 0.440 | 0.439 | 0.436 |

The schedule is worth +0.06 on Buck at either rate (Buck 0.485 is a new
best for that fire; final IoU 0.44 vs 0.35) and never hurts — the E3
result reproduces at 3 seeds. Tick rate still adds nothing on top. The
temperature proxy is a weak driver (±0.04 per °C of anomaly); the fires
that grow 4–7 km in a day need a driver with far more day-to-day range —
real hourly wind, humidity/fuel moisture — than daily ERA5 means offer.
