# E3 — weather-driven daily p0 schedule · KEPT (first physics win)

_Round: Round 1 — 2026-08-15_

Hypothesis: the model over-burns partly because p0 is constant in time —
real fires slow on rainy/cool days. Proxy schedules from ERA5 layers
(`make_p0_schedules.py`): rain `exp(-k·wet)` with 1-day carryover;
temperature `1 + 0.04·(T − mean)` clamped to [0.4, 1.6]; and the two
multiplied. Applied per wind window via `EXP_P0_SCALE`.

| Fire | control | rain k=1 | temp | rain×temp |
|---|---|---|---|---|
| Bear | 0.317 | 0.350 | 0.318 | **0.353** |
| Brattain | 0.337 | 0.344 | 0.339 | **0.346** |
| Buck | 0.414 | 0.332 | **0.460** | 0.356 |
| Chimney | 0.435 | 0.439 | 0.441 | **0.441** |

Findings: temperature never hurts and is worth +0.05 on Buck. Rain is
strong medicine — it collapsed Bear's over-burn from ×3.3 to ×0.3 area
(two rainy days carry real stopping information) but over-suppresses
Buck, which was already under-burning. Next step is folding a proper
fuel-moisture proxy into `WildfireModel` itself (roadmap §10.4) instead
of a harness hack.
