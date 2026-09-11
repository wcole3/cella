# E41 — the Ellipse null · FINDING — wind direction carries real shape, beyond noise, on two of the three gating fires; the kernel is a fair target for E30

_Round 6 (2026-09-11) · no ensemble, deterministic nulls only · runner `exp_r6_ellipse.py` (`nulls` mode) · results `exp41_ellipse.json` · pre-registered TEST_PLAN v1.8 · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** A third dumb forecaster, next to persistence and the
Circle: an ellipse stretched along the window's wind instead of a plain
disc, area-matched the same way. The prediction, written before the run,
was that the ordinary `ellipse_era5` variant would only tie the Circle
everywhere (ERA5 winds here are too gentle, LB ≈ 1.0–1.4). It did not.
`ellipse_era5` beats the Circle beyond the E33 noise floor on Brattain
(+0.019) and Ferguson (+0.131), ties on Buck, and loses on Bear and
Chimney; the ×3 and station variants push the same pattern further
(Ferguson +0.225 at ×3). Two of the three fires the campaign uses to gate
E30 — Brattain and Ferguson — show a real gain from wind direction alone,
using only the ERA5 input the model already has. The fallback clause in
the pre-registration is triggered: wind direction carries shape, so the
kernel (not the inputs) is the fair thing to spend E30 on.

**Question.** Does wind *direction*, in the inputs this campaign actually
has (ERA5 daily, or the nearest weather station), carry any shape signal
on these six fires — before spending any effort making the fire model's
own wind response better at using it?

**What we ran.** A new `nulls` mode in `wildfire_smc`: no ensemble
members, just the deterministic forecasters computed window by window
from `truth.json` and `scenario.json`/`station_hourly.json`. The Circle
grows a chamfer-distance disc from the ignition mask, thresholded to the
observed burned area each day (unchanged from Round 4 on). The Ellipse
grows the same way but anisotropically: a Dijkstra minimum-travel-time
search where the cost of a step is `|offset| / r(θ)`, `θ` the angle
between the step and the window's wind-*toward* direction, and
`r(θ) = b² / (a − c·cos θ)` with `a = LB(U)`, `b = 1`,
`c = √(a² − b²)` — an ellipse with the ignition sitting at the *rear*
focus, not the centre. Concretely, at `LB = 3`: head rate (downwind)
`a + c ≈ 5.83`, back rate (upwind) `a − c ≈ 0.17`, flank rate (crosswind)
`b²/a ≈ 0.33` — about 34× faster downwind than upwind, 17× faster downwind
than sideways. `LB(U) = 0.936·e^{0.2566U} + 0.461·e^{−0.1548U} − 0.397`
(Anderson 1983, U at 10 m, clamped to [1, 8]). The Ellipse's mask is
carried forward window to window the same way the Circle's threshold
order effectively is — it grows from *its own* previous mask, never from
the observed mask directly. Three variants: `ellipse_era5` (the
scenario's ERA5 daily wind, the campaign's default input), `ellipse_
station` (the vector mean — average the per-hour wind as an arrow, speed
and direction together, then read the length and direction back off the
sum — of `station_hourly.json`'s hourly rows falling inside each window;
rotating from the station's compass bearing to the grid's toward-angle
commutes with that averaging, so the mean is computed directly in grid
coordinates), `ellipse_era5x3` (ERA5 wind speed × 3, direction unchanged
— a sensitivity probe from E9c, not a forecaster). Grid convention
checked against `wind_toward_grid_deg`'s own doc and tests before writing
the direction cost: a west wind (`from_deg = 270`) blows *toward* +x on
this north-up, +y-down grid, confirmed by a new unit test that grows the
Ellipse under that exact wind and checks the mask reaches at least twice
as far right of the seed as it reaches up or down.

**How we scored it.** IoU and binary Brier at every observation, mean and
final over the series, exactly as the Circle — all six fires, nothing
chosen per fire. A difference from the Circle smaller than the fire's E33
five-seed sd (Bear 0.015, Brattain 0.004, Buck 0.039, Chimney 0.012,
Ferguson 0.007, Pier 0.003) is a tie.

![Small multiples, one bar chart per fire, of mean IoU for persistence, the Circle, and the three Ellipse variants, with the Circle's E33 ±1 sd band](figures/e41-ellipse-null.svg)

**Result.**

| Fire | persistence | Circle | ellipse_era5 | ellipse_station | ellipse_era5x3 | sd (E33) |
|---|---|---|---|---|---|---|
| Bear | 0.091 / 0.042 / 0.086 | 0.541 / 0.524 / 0.055 | 0.513 / 0.486 / 0.060 **LOSES** | 0.496 / 0.442 / 0.066 **LOSES** | 0.424 / 0.401 / 0.076 **LOSES** | 0.015 |
| Brattain | 0.017 / 0.007 / 0.162 | 0.450 / 0.435 / 0.123 | 0.469 / 0.470 / 0.114 **BEATS** | 0.333 / 0.341 / 0.161 **LOSES** | 0.475 / 0.496 / 0.108 **BEATS** | 0.004 |
| Buck | 0.205 / 0.138 / 0.085 | 0.670 / 0.616 / 0.041 | 0.701 / 0.717 / 0.036 tie | 0.654 / 0.723 / 0.041 tie | 0.654 / 0.677 / 0.043 tie | 0.039 |
| Chimney | 0.119 / 0.052 / 0.139 | 0.372 / 0.262 / 0.159 | 0.247 / 0.179 / 0.199 **LOSES** | 0.205 / 0.157 / 0.213 **LOSES** | 0.178 / 0.136 / 0.223 **LOSES** | 0.012 |
| Ferguson* | 0.007 / 0.003 / 0.185 | 0.373 / 0.361 / 0.167 | 0.503 / 0.498 / 0.121 **BEATS** | 0.421 / 0.397 / 0.155 **BEATS** | 0.598 / 0.608 / 0.092 **BEATS** | 0.007 |
| Pier* | 0.199 / 0.144 / 0.152 | 0.559 / 0.544 / 0.105 | 0.566 / 0.553 / 0.103 **BEATS** (barely) | 0.510 / 0.458 / 0.123 **LOSES** | 0.550 / 0.535 / 0.109 **LOSES** (barely) | 0.003 |

How to read it: each cell is mean IoU / final IoU / mean Brier (IoU
higher is better, Brier lower is better). **BEATS**/**LOSES** marks a
mean-IoU difference from the Circle bigger than the fire's sd in that
direction; unmarked ellipse cells are ties. `*` is the holdout pair.
Wall time (the whole `nulls` run, all five forecasters, one fire): Bear
0.8 s, Buck 1.1 s, Brattain 3.0 s, Chimney 2.4 s, Pier 3.7 s, Ferguson
10.6 s (1155×1316, the largest grid) — 21.6 s total, run in parallel
across three fires at a time.

**Prediction vs. result, line by line:**

- *"ellipse_era5 ties the Circle (inside sd) on all six fires."* — **Wrong
  on four of six.** It ties only on Buck (+0.031, sd 0.039). It beats the
  Circle beyond sd on Brattain (+0.019, sd 0.004), Ferguson (+0.131, sd
  0.007) and — barely — Pier (+0.007, sd 0.003), and loses beyond sd on
  Bear (−0.028, sd 0.015) and Chimney (−0.124, sd 0.012).
- *"ellipse_station moves Chimney and Brattain by more than sd in some
  direction."* — **Right**, and both moves are losses: Chimney −0.167,
  Brattain −0.117. (It also moves Bear −0.045, Ferguson +0.049 and Pier
  −0.049 beyond their sd; the prediction did not say those two were the
  only ones that would move.)
- *"ellipse_era5x3 beats the Circle on Chimney only."* — **Wrong,
  reversed.** It *loses* to the Circle on Chimney (−0.194, the biggest
  loss in the table) and instead beats it on Brattain (+0.026) and
  Ferguson (+0.225); it ties on Buck and loses narrowly on Bear and Pier.
- *"If instead ellipse_station or era5x3 beats the Circle on Brattain,
  Ferguson or Pier, wind direction carries shape and E30 is worth its
  cost."* — **Triggered.** era5x3 beats the Circle on Brattain and
  Ferguson; era5 does too, and also (barely) on Pier; station beats it on
  Ferguson. This is the clause that decides the verdict, and it fired.

- **Chimney's loss is not a surprise in hindsight.** README already
  records that Chimney "grows *against* the ERA5 wind" (E9). Stretching
  an ellipse along wind that points the wrong way spends area on cells
  that never burned instead of spreading it evenly like the Circle does
  — worse than guessing symmetrically. All three Ellipse variants lose
  on Chimney, and lose *harder* the more wind is trusted (era5 −0.124,
  era5x3 −0.194): consistent, not noise.
- **Ferguson's gain is large despite a tiny LB.** ERA5 speed on Ferguson
  peaks at 0.79 m/s, `LB` never exceeds 1.16 — barely elliptical per
  window. The +0.131 mean-IoU gain comes from thirty windows of a mild,
  *consistently* directed stretch compounding, not from any one window's
  shape. A weak but steady direction is still information over a long
  series.
- **Bear and Chimney both lose on every variant.** Bear is the one
  calibration fire besides Chimney where the Ellipse never wins; unlike
  Chimney there is no recorded "grows against the wind" finding for
  Bear, so this is a new, unexplained miss worth a look if E30 is
  scoped.
- **Buck ties everywhere.** Its sd (0.039) is far the widest of the six
  (E33), so this is the fire where "tie" means least — even the ±0.03
  swings here are inside noise on Buck alone.

**What it means.** The pre-registered fallback triggered: on two of the
three fires used to gate E30 (Brattain, Ferguson), an ellipse stretched
along the *existing* ERA5 wind — nothing new, no better data — beats the
area-matched Circle by more than the campaign's own noise floor, using
only wind direction. The fire model's own wind response (the
Alexandridis `c1`/`c2` kernel, unchanged since Round 1) is not extracting
that signal today: E24–E38 show the model losing to the Circle on shape
on exactly these elongated fires. The blocker is the kernel, not the
input, on Brattain and Ferguson; Chimney's result says the opposite
(there, the *input* is actively wrong, and no kernel can fix a direction
that points the wrong way); Bear's is unexplained. Pier's evidence is too
close to its own sd (0.003) to lean on either way.

**Questions this raises.**

- Chimney's ERA5 wind points against the true spread direction; does the
  terrain-adjusted field (E26, "NULL at this kernel") do better once
  there is a directional null to check it against? Open.
- Bear loses on every Ellipse variant with no known cause; worth a
  one-fire look (per-window wind vs. per-window true growth direction)
  before E30 is scoped broadly. Open.
- Does a kernel change (E30) that increases the *effective* anisotropy
  the model itself achieves under ERA5 wind reproduce this Ellipse's gain
  on Brattain and Ferguson without Chimney's loss growing worse? Open:
  **E30**.

**Verdict.** FINDING. The fallback in the pre-registration fired: wind
direction, from the input already on hand, carries real shape signal on
Brattain and Ferguson, beyond noise, using nothing but the existing ERA5
schedule. E30 (kernel work to make the fire model itself more directional)
is a reasonable next spend — with the caveat that on Chimney the same
signal actively points the wrong way, so a stronger kernel is not a free
win everywhere.

**Later.** E30 (not yet run).
