# Observed front speed vs the model's hard cap · finding (data)

_Round: Round 2 — 2026-09-01: is the wind right, and why is the model too round?_

The front can advance at most one cell per step: 50 steps/day × 30 m =
**1.5 km/day**. From the truth arrival fields (Euclidean distance from
the previous day's burned set to each newly burned cell,
`obs_front_speed.json`), all six fires:

| Fire | max daily advance | windows over the cap | new cells in those windows |
|---|---|---|---|
| Bear | 2.3 km | 3 / 22 | 37 % |
| Brattain | 4.9 km | 10 / 21 | **98 %** |
| Buck | 2.3 km | 3 / 29 | 40 % |
| Chimney | 6.1 km | 8 / 15 | **80 %** |
| Ferguson (holdout) | 7.3 km | 18 / 29 | 87 % |
| Pier (holdout) | 3.4 km | 7 / 30 | 63 % |

Most of the real burned area arrives on days the model *cannot* keep up
with, then the model catches up over the following days by burning
everywhere. This is the "too slow at the start, unstoppable at the end"
shape problem from ANALYSIS.md §5, now quantified, and it explains why
the wind kernel cannot elongate the burn: elongation needs the downwind
front to outrun the flanks, but the downwind front is already pinned at
the cap. E11 tests raising the cap.
