# Wildfire Model — Research Experiment Log

This file tracks every experiment we run trying to improve the wildfire
model's real-world scores. One entry per experiment: what we tried, why,
what happened (numbers), and the verdict. Failed experiments stay in the
log — knowing what *doesn't* work is half the record (test plan §8 rule 5).

Ground rules for every entry, from [TEST_PLAN.md](TEST_PLAN.md):

- Experiments run on the **four calibration fires only** (Bear, Brattain,
  Buck, Chimney). The holdout pair (Ferguson, Pier) is never touched.
- "Mean IoU" below = mean overlap score over the observation series,
  t = 0 excluded (it is 1.0 by construction). Higher is better.
- Every number is quoted next to the Circle (area-matched radial null) on
  the same fire — beating the Circle is the bar that matters
  (see [ANALYSIS.md](ANALYSIS.md)).
- Scan-grade results use 1 seed (cheap, noisy); anything promoted gets a
  3-seed verification before it is believed.

**Tooling** (all committed):

- `cella_lib/examples/wildfire_experiment.rs` — copy of the scoring
  harness with experiment-only hooks the real harness must not have:
  `EXP_P0_SCALE` (per-day p0 multiplier schedule), `EXP_WIND_SCALE`
  (wind multiplier), `EXP_SEED_BASE` (seed offset for per-seed dumps).
- `validation/scripts/experiments/` — the runners, in run order:
  `exp_sweep.py` (E1), `make_p0_schedules.py` + `exp_variants.py`
  (E2–E5), `exp_spotting.py` (E6–E7), `exp_ensemble.py` (E8),
  `exp_verify.py` (3-seed verification). Outputs land in the gitignored
  `validation/results/experiments/`.

---

## Round 1 — 2026-08-15

Starting point: uncalibrated textbook parameters lose to the Circle on
5 of 6 fires (TEST_PLAN §5). Question: which levers close the gap?

### E1 — p0 × burn_duration scan · KEPT (the big lever)

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

### E2 — canopy-cover density layer · REJECTED

Hypothesis: per-cell density from the HDF5 `230CC` layer adds the spatial
heterogeneity the model lacks. Mapping: forest cells 0.5 + 0.5·(CC/75),
grass kept at 1.0 (CC is *tree* cover; grass must not be punished).
`exp_variants.py::cc_density`.

Result: Bear +0.018, Brattain −0.017, Buck −0.045, Chimney −0.009.
Mixed-to-negative. Other mappings might work; this one doesn't.

### E3 — weather-driven daily p0 schedule · KEPT (first physics win)

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

### E4 — wider veg_factor spread · REJECTED

Grass 2.0 / GrassShrub 1.4 / Shrub 1.0 / TimberUnder 0.7 /
TimberLitter 0.3 / Slash 0.8 (vs the narrow defaults). Bear −0.061,
Buck −0.064, others ≈ flat. This particular spread is wrong; per-class
veg_factors stay in the calibration search space for the campaign.

### E5 — wind gust multiplier ×2 / ×4 · REJECTED

Hypothesis: ERA5 daily-mean winds (0.6–1.2 m/s) flatten the gusts;
at V ≈ 1 the wind kernel `exp(0.045·V)` ≈ 1.05 is nearly inert.
`EXP_WIND_SCALE`. Result: helps only Chimney at ×2 (+0.013), hurts Bear
(−0.02/−0.04), ≈ flat elsewhere. Consistently *narrows* the burn
(area ratios drop) — directionality without accuracy. The real fix is
per-cell wind (§10.4), not a scalar multiplier.

### E6 — 4× time resolution · REJECTED (as tested)

Hypothesis: 50 steps/day × 30 m caps front speed at 1.5 km/day; real
runs move 10–30 km/day. Tested steps ×4 with p0/4 (crude rate scaling).
Result: Bear −0.05, Brattain −0.02, Buck +0.03, Chimney −0.02. The p0/4
compensation is too crude near the percolation cliff; a fair test needs
a joint (steps, p0) scan. At daily truth cadence the over-burn error
dominates the too-slow error anyway.

### E7 — spotting on · REJECTED

Three settings inside the pre-registered spotting space (p_spot
0.001–0.005, median 5–20 cells). Every one neutral-to-catastrophic:
mid/far settings add ×2–4 area and cost 0.1–0.2 mean IoU on every fire.
Spotting amplifies exactly the failure we already have (over-burn). Not
worth revisiting until the model can *stop* — then it may help the fast
wind-driven fires.

### E8 — ensemble burn-probability threshold · finding, not a lever

Score "cells burned in ≥ q of 5 seeds" for q = 1..5 (`exp_ensemble.py`).
Union (q = 1) is best or tied on all four fires (+0.05 Buck, +0.02 Bear);
stricter voting only ever loses. Diagnosis: **the over-burn halo is
deterministic** — every seed agrees on it (percolation), while seeds
differ in which parts of the *real* burn they cover. Seed averaging can
therefore never remove false alarms. Union-of-seeds is a legitimate small
post-processing gain if we ever report ensemble masks.

### Verified results (3 seeds, `exp_verify.py`)

Per-fire recipe = E1 best + best E3 schedule. Global recipe = one setting
for all fires (p0 0.22, dur 5, temperature schedule) — the honest
headline mode.

| Fire | per-fire | global | Circle | arrival MAE (uncalibrated was) |
|---|---|---|---|---|
| Bear | 0.347 | 0.222 | 0.541 | 40 h (87 h) |
| Brattain | 0.344 | 0.320 | 0.450 | 98 h (36 h) |
| Buck | 0.448 | 0.362 | 0.670 | 43 h (186 h) |
| Chimney | **0.443** | **0.418** | 0.372 | 51 h (20 h) |

Scan-grade numbers held up under 3 seeds. The Circle still wins 3 of 4 —
progress is real but "knowing where to stop" (moisture, suppression)
remains the gap, as the pre-registered failure hypotheses predicted (§7).

### Engine findings (not score-related, arguably worth more)

1. **p0 write-after-attach footgun.** `params.p0` is baked into
   `derived.p_base` at attach ([wildfire.rs](../cella_lib/src/wildfire.rs)
   `attach()`); writing `params.p0` afterwards is silently ignored. Wind
   params are read live per chunk, which hides the inconsistency — the
   first E3 run was a silent no-op because of it (all variants identical
   to control; caught because temperature schedules *cannot* leave results
   unchanged). Workaround used: `boxed_clone()` + `attach_model()` per
   window. Engine task: either re-derive on param change or make the
   baked-in params impossible to write directly.
2. **Front-speed cap** (E6 above): a structural ceiling to keep in mind
   for sub-daily truth (PT-FireSprd, GOFER) even though daily scoring
   doesn't expose it.

### Round 1 conclusions → what to do next

1. Calibration campaign (§6) with informed priors: dur ∈ {5, 10},
   per-fire p0 ∈ [0.10, 0.35], per-class veg_factors free, spotting off.
2. Promote the weather schedule from harness hack to a `WildfireModel`
   fuel-moisture proxy input (§10.4) — temperature-based first (never
   hurt), rain with care (over-suppresses under-burners).
3. File the p0-footgun engine fix.
4. Skip: spotting, gust multipliers, this CC density mapping, naive
   time-resolution scaling.

---

## Round 2 — 2026-09-01: is the wind right, and why is the model too round?

Trigger: a GUI tester saw "wind direction 0°" push the fire **east** and
expected north-to-south (the weather-report convention). Before touching
parameters again, Round 2 audits the wind path end to end against the
observed fires, then asks what actually limits the shape of the burn.

**Tooling added:** `EXP_WIND_ROT_DEG` hook in `wildfire_experiment.rs`;
runners `exp_wind_rotation.py` (E9), `exp_wind_oracle.py` (E10),
`exp_timeres.py` (E11). Shape/speed diagnostics are one-off scripts whose
outputs are kept in `results/experiments/` (`obs_front_speed.json`,
`exp12_shape.json`).

**Gotcha that cost one run:** the repo root and `cella_lib/` are separate
cargo build roots. `cargo run` inside `cella_lib/` writes to
`cella_lib/target/`; a stale `target/release/examples/wildfire_experiment`
at the repo root silently ran without the new hook (every rotation scored
identically — the tell). Runners now point at `cella_lib/target/`.

### E9a — wind convention audit (code + converter + raster) · NO BUG

Three independent checks, all consistent:

1. **Code.** `wind_dir_deg` is the direction the wind blows *toward*,
   0° = +x, 90° = +y (down the grid). `dir_factors()` gives the
   neighbor at offset (−1, 0) — fire west of the cell — the maximum
   factor when θ = 0, so fire moves east. Unit test
   `wind_factor_is_max_downwind_min_upwind` pins this.
2. **Converter.** `convert_pytorchfire.py` maps ERA5 (u east, v north)
   to `atan2(−v, u)`, which is the same convention *if* raster row 0 is
   the north edge.
3. **Raster orientation.** Compared the LANDFIRE aspect layer (compass
   bearing of downslope, 0° = N) against the numerical gradient of the
   elevation layer on all six fires: mean cos = **+0.96** under
   "row 0 = north", ≈ 0 under "row 0 = south". Row 0 is north.

So the pipeline is self-consistent. The tester's expectation was the
meteorological convention (bearing the wind comes **from**, 0° = north,
clockwise). Conversion: `wind_dir_deg = (from_bearing + 90) mod 360` —
a north wind (from 0°) is `90` here. Now documented on the parameter
(rustdoc + the GUI panel description). Open question for the GUI: expose
a "from" bearing instead of the grid angle (see conclusions).

### E9b — rotate the whole wind schedule · finding, not a lever

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

### E9c — is Chimney's 270° a lucky angle? · NO (robust plateau)

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

### E10 — wind-direction ORACLE (upper bound, cheats) · finding

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

### Observed front speed vs the model's hard cap · finding (data)

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

### E12 — shape: how round is the model? · finding

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

### E11 / E11b — joint (steps/day, p0, burn_duration) scan · REJECTED (cap is not the binding limit)

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

### E13 — daily temperature schedule × tick rate · schedule KEPT, rate REJECTED

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

### Round 2 conclusions → what to do next

1. **No wind bug, but a home-made convention.** "Toward, 0° = +x" was
   ours alone (Alexandridis defines only a relative angle; PyTorchFire
   uses toward/east/counter-clockwise; every weather source and
   operational simulator uses the bearing the wind comes *from*). Done
   the same day: the parameter is now `wind_from_deg` in that weather
   convention everywhere — model field, panel, configs, converter,
   scenario format v2. Old `wind_dir_deg` files are rejected on load.
   Every number in this log is unchanged: the conversion is exact
   (`toward = from + 90°`) and the six-fire reports were re-run to
   confirm.
2. **Wind input, not wind math, is the weak link.** ERA5 daily domain
   means are too weak to act (E9b) and wrong on Chimney's run days (E9c).
   Codified as TEST_PLAN §2.1 + process rule 6: every source's wind
   convention, units, height and averaging get inspected and written into
   `provenance.weather` before any comparison is quoted.
   Before any more wind work, get hourly ERA5 (or a station record) into
   the scenario schedule and re-run E9b: if the rotation effect vanishes
   and Chimney improves at ×1, the kernel is vindicated.
3. **Stop scanning resolution.** E6, E11, E11b, E13 all say the same
   thing: p0 × steps is one knob. Do not spend more runs on steps/day.
4. **Time-varying drivers are the lever.** The temperature schedule is
   now a 3-seed-verified win on the fire it fits (Buck). Promote it into
   `WildfireModel` as a proper input (roadmap §10.4) and add a
   higher-range driver (hourly wind speed and RH → fuel-moisture proxy)
   so fast days and stopped days both become representable.
5. **Shape diagnostics stay in the harness.** Elongation and daily
   growth direction (E12) caught what IoU hides (Brattain: 2.7 vs 1.05).
   Worth adding to the report JSON so every run carries them.
6. **Fix the two build roots trap** in the runner scripts (done for the
   new ones) and in the older `exp_*.py`, which use `cargo run` from
   `cella_lib/` and are therefore safe — but only by accident.
