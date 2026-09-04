# E10 — wind-direction ORACLE (upper bound, cheats) · finding

_Round: Round 2 — 2026-09-01: is the wind right, and why is the model too round?_

Question: if we had perfect daily wind direction, how much would this
kernel buy? `exp_wind_oracle.py` replaces each day's wind with the
direction the observed fire actually grew that day (centroid of the
day's new burn vs the previous burned set) at a fixed speed V; calm days
keep ERA5. Reads the truth → never a reportable score. 3 seeds.

| Fire | ERA5 | oracle V=2 | V=5 | V=8 | Circle |
|---|---|---|---|---|---|
| Bear | 0.311 | 0.301 | 0.277 | 0.259 | 0.541 |
| Brattain | 0.336 | 0.345 | 0.346 | 0.341 | 0.450 |
| Buck | 0.403 | 0.396 | 0.381 | 0.364 | 0.670 |
| Chimney | 0.441 | 0.464 | 0.491 | **0.524** | 0.372 |

Even a perfect direction is worth at most +0.08 (Chimney) and *hurts*
Bear and Buck, because stronger wind narrows the burn (area ratio Bear
×3.4 → ×0.67) while p0 was tuned at ERA5 speed. Caveat: the centroid
direction is a crude oracle (Chimney's rotated-ERA5 run above beat it).
Conclusion: **better wind data alone is not the next step**; the kernel
cannot express the shapes we are missing, because of E11/E12 below.
