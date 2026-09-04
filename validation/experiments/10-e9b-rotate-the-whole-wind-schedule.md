# E9b — rotate the whole wind schedule · finding, not a lever

_Round: Round 2 — 2026-09-01: is the wind right, and why is the model too round?_

If the convention were wrong by a constant, one rotation would win on
**every** fire. `exp_wind_rotation.py`: E1 best (p0, dur) per fire, 3
seeds, rotations 0/90/180/270°, wind off (×0) and wind ×5 (rotated too).
Mean IoU:

| Fire | rot 0 | rot 90 | rot 180 | rot 270 | wind off | ×5 rot 0 | ×5 rot 90 | ×5 rot 180 | ×5 rot 270 |
|---|---|---|---|---|---|---|---|---|---|
| Bear | 0.311 | 0.316 | 0.322 | 0.306 | 0.315 | 0.264 | 0.253 | 0.307 | 0.283 |
| Brattain | 0.336 | 0.337 | 0.333 | 0.331 | 0.335 | **0.321** | 0.286 | 0.260 | 0.266 |
| Buck | 0.403 | 0.428 | 0.415 | 0.392 | 0.417 | 0.408 | 0.407 | 0.357 | 0.348 |
| Chimney | 0.441 | 0.427 | 0.453 | 0.474 | 0.445 | 0.298 | 0.278 | 0.456 | **0.571** |

Findings:

- At the converted ERA5 speeds (0.1–3 m/s daily domain means) **wind is
  inert**: rotations and "wind off" all sit within ±0.02 of each other.
  `exp(c1·V)` at V ≈ 1 is 1.05; the kernel needs V ≳ 5 to matter.
- At ×5 no single rotation wins everywhere (Brattain/Buck prefer 0°,
  Bear 180°, Chimney 270°) — **not a convention bug**. A constant offset
  would show one winner.
- Chimney ×5 rotated 270° scores **0.571 (final 0.597)** — far above
  its control (0.298) and the Circle (0.372). Rotating by −90° turns the
  ERA5 "toward ENE" wind into "toward WSW". Contemporary reporting
  (New Times SLO, 2016-08-25) says the Aug 20–21 runs were driven by
  gusts "out of the east" pushing the fire west, then a north-east shift.
  ERA5's ~1 m/s daily domain mean carries none of that. **The wind input
  is wrong on the days that mattered, not the wind math.**
