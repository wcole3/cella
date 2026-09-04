# E12 — shape: how round is the model? · finding

_Round: Round 2 — 2026-09-01: is the wind right, and why is the model too round?_

Seed-0 arrival fields at E1 best recipes (`exp12_shape.json`).
Elongation = √(λ₁/λ₂) of the burned set's second-moment matrix (1.0 =
disc); daily growth direction = centroid of new burn relative to the
previous burned set.

| Fire | elongation truth / model / Circle (final) | mean \|truth − model\| growth dir | mean \|truth − ERA5 wind\| | mean \|model − ERA5 wind\| |
|---|---|---|---|---|
| Bear | 2.30 / 2.14 / 1.25 | 91° | 92° | 47° |
| Brattain | 2.70 / **1.05** / 1.24 | 68° | 65° | 73° |
| Buck | 1.02 / 1.69 / 1.37 | 75° | 71° | 77° |
| Chimney | 1.56 / 1.34 / 1.00 | 53° | 128° | 127° |

(90° = no relationship.) Brattain, the most elongated real fire, comes
out of the model as a near-perfect disc. On Chimney both the truth and
the model grow *against* the ERA5 wind (128°/127°) — the model is
following fuel and terrain corridors, which is also why it is the one
fire that beats the Circle. Bear's model growth tracks the wind (47°)
while the real fire does not (92°) — the ERA5 direction there is
actively misleading.
