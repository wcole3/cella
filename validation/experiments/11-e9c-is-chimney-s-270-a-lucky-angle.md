# E9c — is Chimney's 270° a lucky angle? · NO (robust plateau)

_Round: Round 2 — 2026-09-01: is the wind right, and why is the model too round?_

Same recipe (p0 0.30, dur 5, 3 seeds), sweeping the rotation and the
gust multiplier around the E9b winner (`results/experiments/exp9b_chimney/`):

| ×5 rot 200 | 225 | 240 | 270 | 300 | ×3 rot 270 | ×8 | ×12 |
|---|---|---|---|---|---|---|---|
| 0.485 | 0.543 | 0.568 | **0.571** | 0.500 | 0.539 | 0.475 | 0.406 |

(mean IoU; control ×1 rot 0 = 0.441, wind off 0.445, Circle 0.372.)
Anything from "toward SW" to "toward WNW" at 3–5 m/s scores 0.50–0.57;
too strong (×8+) narrows the burn below the observed area (×0.9) and
loses again. So the model wants a moderate, mostly-westward wind on
Chimney — exactly what the reporting describes and what the ERA5 daily
domain mean does not contain. This is **not a calibration result** (the
rotation was chosen by looking at the truth); it is evidence about the
input, kept here as a negative result for "ERA5 daily means are a
sufficient wind input".
