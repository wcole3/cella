# Wildfire Model Validation — Test Plan (v1)

**Guiding principle: the goal is the best answer, not making the model look
good.** Every rule in this plan exists to prevent us from fooling ourselves.
Where the model fails, the failure is the result — it gets reported, analyzed,
and used to decide what to build next.

This plan is *pre-registered*: metrics, baselines, calibration protocol, and
the parameter search space are fixed here **before** results are collected.
Changing them afterwards requires a new plan version with the change and its
reason logged in §9.

*Reading this for the first time, or not deep in the code?
[ANALYSIS.md](ANALYSIS.md) explains the scores, the baselines, and the
current results in plain language.*

---

## 1. What is under test

The `cella_lib` wildfire model (`WildfireModel`): Alexandridis-style
stochastic CA — per-cell ignition probability `p0 × veg_factor × density`
with exponential wind/slope modifiers, burn duration in ticks, optional
lognormal spotting. Deterministic per seed; scored as seed ensembles.

Out of scope for v1: per-cell wind fields, fuel moisture, suppression. Each
is expected to be a *cause of measured failure* — the point of v1 is to
quantify how far the model gets without them.

## 2. Data

All sources are converted to the canonical scenario format (FORMATS.md).
Truth quality is part of the record: every score table carries the truth's
own `spatial_accuracy_m`.

| Tier | Scenarios | Truth cadence | Role |
|---|---|---|---|
| T0 | Six-fire pack (Bear, Chimney, Pier, Brattain, Ferguson, Buck) | daily | primary development + calibration set |
| T1 | Dogrib 2001 | final perimeter | cross-model benchmark (Prometheus F1 0.74, Cell2Fire 0.83) |
| T2 | PT-FireSprd (Portugal, 80 fires) | ~3-hourly | held-out generalization + junction/reversal stress |
| T3 | GOFER (28 CA fires) | hourly, ±1 km edges | arrival-time / growth-rate scoring; plume + terrain stress |
| T4 | Camp Fire 2018 (NIST points) | sub-hourly | spotting endgame |

### 2.1 Weather inputs: inspect before you compare (added v1.1)

Every comparison — against an observed fire *or* against another fire
model's published score — is only as good as the weather feed behind it.
Round 2 of [experiments/](experiments/README.md) found that our wind
maths was right and our wind *input* was the problem: ERA5 daily domain
means at 0.1–3 m/s leave the wind kernel inert, and on Chimney 2016 they
point the opposite way to the gusts that actually drove the fire. A
convention slip would have looked exactly the same in the score table.
So, **before any weather-driven result is quoted**, the person running the
comparison walks this list and records the answers in
`scenario.json → provenance.weather` (see [FORMATS.md](FORMATS.md)):

1. **Direction convention.** Is the direction the bearing the wind comes
   *from* (weather-report, 0° = north, clockwise — what cella's
   `wind_from_deg` expects) or the direction it blows *toward*? Where is
   0°: north or east? Clockwise or counter-clockwise? Other simulators
   differ (PyTorchFire: toward, 0° = east, counter-clockwise; Prometheus,
   FARSITE, WindNinja: from, 0° = north, clockwise). Write down the
   conversion used.
2. **Grid orientation.** cella assumes row 0 is the northern edge. Prove
   it for each new source with an independent layer — we check the
   LANDFIRE aspect layer against the elevation gradient (cosine ≈ +0.96
   when rows run north→south, ≈ 0 when flipped). Never assume it from
   the file's bounds.
3. **Units and height.** m/s, km/h, knots or mph? 10 m wind (ERA5), 20 ft
   / 6.1 m open wind (FARSITE, US fire weather) or mid-flame wind? A 20 ft
   → mid-flame adjustment is a factor of 2–4; a knots → m/s slip is ×2.
4. **Time averaging.** Hourly values, daily means, or instantaneous
   snapshots? A daily *vector* mean of u/v cancels a wind that swings
   during the day and hides gusts entirely (Chimney: mean 1 m/s from the
   south-west; reported easterly gusts on the run days). Prefer hourly;
   if only daily means exist, say so in `simplifications` and expect the
   wind kernel to be nearly inert.
5. **Space averaging.** ERA5 cells are ~31 km; our domains are 10–30 km,
   so one domain mean is all the data offers. Canyon channelling and
   sea-breeze fronts are invisible at that scale. Record the source
   resolution next to the grid resolution.
6. **Vector mean vs speed mean.** `hypot(mean u, mean v)` is smaller than
   `mean(hypot(u, v))` whenever direction varies across the field. State
   which one the converter took.
7. **Other drivers the comparison model had.** Temperature, humidity,
   precipitation, fuel-moisture codes (FFMC/DMC/DC in Canadian streams),
   suppression records. A model that consumed them is not comparable to
   one fed wind alone; list what each side saw.
8. **Two cheap sanity experiments, run once per new source**, both in
   `scripts/experiments/`: `exp_wind_rotation.py` (a rotated schedule must
   *not* beat the unrotated one — if 90°/180°/270° wins, the convention or
   the orientation is wrong) and the growth-direction alignment in the E12
   shape diagnostic (mean |observed growth direction − wind| should sit
   well below the 90° of "no relationship" on wind-driven fires).

A weather-driven score quoted without these answers on file is a
pre-registration violation, same as reshuffling the holdout.

**Holdout rule:** calibration may use at most 4 of the 6 T0 fires. The
remaining 2 T0 fires and every higher tier are *test-only*: no parameter may
be chosen, directly or indirectly, by looking at their scores. The T0 split
is fixed now, by alphabetical order — **calibration: Bear, Brattain, Buck,
Chimney; holdout: Ferguson, Pier.** Leave-one-fire-out cross-validation
within the calibration four is encouraged; re-splitting to move a
badly-scoring fire out of the holdout is forbidden.

## 3. Metrics (fixed)

Computed by `cella_lib/examples/wildfire_validate.rs` at each observation
time of the truth, as ensemble means:

1. **IoU (Jaccard)** and **Sørensen** of burned sets — the field standard;
   comparable to published baselines.
2. **Miss rate** (observed burned the model missed) and **false rate**
   (model burned that observation didn't) — the two failure directions IoU
   folds together.
3. **Arrival-time MAE** (hours) over cells burned in both, quantized to the
   observation cadence. Primary metric for T3+ where perimeter edges are
   coarse but timing is good.
4. **Burned-area-over-time curves** (recorded in every report JSON).

Ensembles: minimum 3 seeds during development, 10 for reported results;
report the mean and, for reported results, the min–max across seeds. Never
report a best seed.

## 4. Baselines (fixed) — the honesty floor

Every scored run includes, on identical truth:

- **Persistence**: ignition set never grows. Floor of floors — a model below
  this is destroying information.
- **Area-matched radial**: disc grown from the ignition set (chamfer
  distance), area matched to the observed area at every observation time.
  This null has *perfect area calibration* by construction, so beating it is
  evidence of genuine spatial skill: the model knows *where*, not just *how
  much*. Both are computed inside the harness on every run — they cannot be
  forgotten or omitted.

External bars, for context, not as targets to tune toward: neural-CA IoU
> 0.6 at 72 h (same T0 data, calibrated per fire with 10-day assimilation);
Cell2Fire F1 0.83 on Dogrib.

## 5. Starting-point record (2026-08-14, pre-calibration)

Textbook Alexandridis defaults (p0 = 0.58, burn_duration 5, first-guess
veg_factors, no spotting), all six T0 fires, 3 seeds, final IoU:

| Fire | model | persistence | radial null | arrival MAE (h) | verdict |
|---|---|---|---|---|---|
| Bear_2020 | 0.124 | 0.042 | 0.524 | 86.6 | loses to null |
| Brattain_2020 | 0.196 | 0.007 | 0.435 | 36.2 | loses to null |
| Buck_2017 | 0.143 | 0.138 | 0.616 | 185.8 | barely beats persistence |
| Chimney_2016 | **0.282** | 0.052 | 0.262 | 19.8 | **beats the null** |
| Ferguson_2018 | 0.287 | 0.003 | 0.361 | 103.0 | loses to null |
| Pier_2017 | 0.224 | 0.144 | 0.544 | 84.4 | loses to null |

**The uncalibrated model loses to the area-matched radial null on five of
six fires** — it systematically over-burns (false rates ~0.8+), so its
spatial pattern is currently worth less than "a disc of the right size".
The one exception, Chimney 2016, was the fastest wind-driven fire in the
set: where real spread is strongly directional, even the uncalibrated wind
kernel adds spatial signal. Slow fires (Buck: 185 h arrival MAE) are where
over-spread hurts most — consistent with the missing fuel-moisture /
suppression physics (§7). A quick probe also mapped a percolation cliff on
Bear: p0 0.10 dies (5k cells), p0 0.20 over-burns (212k), the observed 56k
sits in the narrow band between — per-fire sensitivity is high, which is
itself a finding about the model family.

## 6. Calibration protocol

- **Search space (pre-registered):** p0 ∈ [0.05, 0.6]; burn_duration ∈
  {1..20}; per-class veg_factor ∈ [0.1, 2.0] (6 classes); density from
  canopy cover on/off; spotting off, or (p_spot ∈ [0, 0.05], median ∈
  [1, 30] cells, σ ∈ [0.2, 1.0]). `steps_per_hour` is a declared constant
  (50/day), not searched.
- **Objective:** mean IoU across the calibration fires' observation series
  (not final-only — final-only rewards burning everything eventually).
- **Method:** coarse grid search, then coordinate descent; 3 seeds per
  evaluation; budget 500 evaluations per campaign, logged.
- **Two calibration modes, reported separately and labeled:**
  (a) *global* — one parameter set for all calibration fires (the honest
  headline number); (b) *per-fire* — what the published CA papers do; upper
  bound, reported only alongside (a), never alone.
- Calibrated parameter sets are written to
  `validation/results/calibrations/<campaign>.json` with the search log.

## 7. Failure-mode hypotheses (pre-registered)

What we expect to break, per tier — recorded now so the analysis is a check
against predictions, not a story fitted afterwards:

- T0: over-spread on low-wind days (no fuel moisture); stalls at late-fire
  containment lines (no suppression).
- T1 Dogrib: octagonal front-shape artifacts from the 8-neighbor lattice
  under the mid-run wind shift.
- T2 junction fires (Pedrógão): under-prediction of acceleration where
  fronts merge — a memoryless per-cell CA has no mechanism for it.
- T3 Creek: night-time under-spread and mis-timed plume-driven runs —
  growth decoupled from ambient wind.
- T3 Dixie: canyon wind channeling not representable by uniform wind.
- T4 Camp: lognormal spotting kernel cannot reproduce massed ember transport
  across the canyon; urban "non-burnable" fuels stall the front where the
  real fire accelerated.

Each scored tier gets a short written comparison against these predictions
in `validation/results/analysis/`.

## 8. Process rules

1. Any model or converter change → full re-run of every previously reported
   scenario before new numbers are quoted. Reports embed the git hash.
2. All scenarios in a tier get reported, including the bad ones. No
   cherry-picking fires, seeds, days, or metrics.
3. Truth is never edited to fit; suspected truth errors are logged in the
   scenario's provenance and taken upstream.
4. Engine changes remain gated by the existing correctness suite (FNV
   snapshots, thread-equivalence, ≥99 % coverage) — validation runs are not
   a substitute for it.
5. Negative results are kept: rejected calibrations, failed hypotheses, and
   worse-than-null configurations stay in the results directory and the
   analysis notes.
6. Weather inputs are inspected before they are compared (§2.1). The
   `provenance.weather` block in `scenario.json` is filled in for every
   source, and any result quoted against another model or an observation
   names the wind convention, units, height, and averaging on both sides.

## 9. Plan changelog

- v1.9 (2026-09-23, before the E48 run): Round 7
  (`docs/superpowers/plans/round-7-experiments.md`) pre-registers six
  experiments, E44–E49, that ask one question — does the E30b Arm B
  configuration (arrival kernel, rear-focus wind law, 4× clock, the
  widened `arrival_x4` prior, a learned per-member `wind_rot_deg` gene
  at ±90°) deserve to replace the Bernoulli recommendation, and why does
  it work? Arm B is validated so far only as a one-seed pilot (E30b);
  none of it is a production default yet. Shared-machine rules bind
  every one of these batches: 2 workers max, `nice -n 10`, one batch at
  a time, `uptime` checked before launch (wait and re-check every 10
  minutes above a 1-minute load of 8), load reported at launch and
  finish; gate/reset/state-correction stay off; every report carries
  `binary_git`/`binary_built_utc` and the runner refuses to start a
  batch on a dirty or stale binary. Runner: `validation/scripts/
  experiments/r7_common.py` (the `ARM_B` preset, the shared `run`/
  `run_all`, the binary_git and load gates, and the per-fire mean/sd
  summariser with the delta-in-sd verdict column worded exactly as
  `48-e30b-uncapped-clock-direction-gene-pilot.md`'s tables: "tie" /
  "beyond 1 sd (gain|loss)" / "**beyond 2 sd (gain|loss)**", tie = within
  1 sd, beyond 2 sd = the bar for a claimed gain or loss) plus one
  `exp_r7_e44.py` … `exp_r7_e49.py` per experiment. **Noise floor for all
  six:** the E33 five-seed sd (Bear 0.015, Brattain 0.004, Buck 0.039,
  Chimney 0.012, Ferguson 0.007, Pier 0.003) until E44 produces Arm B's
  own five-seed sd, from which point every arrival-kernel arm (E45, E46,
  E47, E49) is judged against Arm B's own sd instead. Honest-validation
  rules apply throughout: all six fires, nothing chosen per fire;
  Ferguson and Pier holdout; every report carries persistence, Circle,
  Ellipse and lagged nulls; five-seed (or fewer, where pre-registered
  below) mean and sd reported, never the best seed; every prediction
  below is written before its run and checked clause by clause after.

  **E44 — five-seed E30b Arm B and E37b at the 4× clock (the promotion
  test).** *Question:* does the full, multi-seed Arm B configuration
  beat E33, and does it reach shapes E37/E37b's illumination said were
  previously unreachable? *Design:* forecast — `ARM_B` preset, seeds
  0–4, six fires, `assim` mode, 30 runs at 2 workers (≈10 h, one batch,
  nothing else launched alongside it); plus E37b re-run at the 4× clock
  — `map` mode (MAP-Elites illumination, 960 evaluations per fire:
  `SMC_GENERATIONS=30` × `SMC_POP=32`, identical to E37/E37b), Arm B
  preset, six fires, one batch, run only after the forecast batch.
  *Arms:* Arm B only (no Arm A re-run — E30b already scored it). *Seeds:*
  0–4 (forecast); illumination has no seed axis. *Fires:* all six.
  *Score family:* one-window-ahead consensus IoU (forecast); reachable-
  wedge coverage over growth × elongation (illumination). *Noise floor:*
  E33 five-seed sd (above). *Prediction, written before the run:*
  five-seed mean beats E33 beyond 2 sd on Brattain, Chimney, Ferguson;
  ties Bear and Pier; Buck within its own sd. *Stop rule:* if the
  five-seed mean loses to E33 beyond 1 sd on any fire, Arm B is not
  promoted — E45 and E46 still run (they explain the pilot regardless of
  whether it is promoted), E47 does not run. Runner: `exp_r7_e44.py` →
  `exp44_arm_b_5seed.json` (forecast), `exp44_e37b_4x_illuminate.json`
  (illumination). Write-up: `validation/experiments/
  50-e44-full-e30b-arm-b.md`.

  **E45 — mechanism ablations on the `wind_rot_deg` gene.** *Question:*
  does the gene work because per-member angular diversity helps the
  ensemble fit each day's actual wind, or because the filter learns one
  correct bearing? *Design:* two arms, same seeds as E44. Arm B-σ0: Arm
  B preset with `SMC_WIND_ROT_SIGMA=0` — mutation sigma 0 on
  `wind_rot_deg`, so each member keeps its birth rotation (diversity,
  no learning); `SMC_WIND_ROT_SIGMA` is a new knob (default = current
  behaviour) if the gene spec cannot already take a per-gene sigma. Arm
  B-20: Arm B preset with `SMC_WIND_ROT_GENE=20` (±20° instead of ±90° —
  learned-correction-only test). *Seeds (both branches pre-registered;
  E44's own result selects one):* seeds 0–4 **if** E44's Arm B five-seed
  sd is ≤ the E33 sd on ≥ 4 fires; **otherwise** seeds 0–2, and the
  write-up must say which branch applied. *Fires:* all six, 2 workers,
  one arm per batch. *Score family:* one-window-ahead consensus IoU,
  plus the per-window posterior spread (IQR) of `wind_rot_deg` for Arm
  B, Arm B-σ0 and Arm B-20 (new field in the `assim` report if absent).
  *Noise floor:* Arm B's own five-seed sd from E44 (E33's sd if E44 used
  the 3-seed branch and its own sd is not yet meaningful at that seed
  count). *Prediction, written before the run:* diversity wins — Arm
  B-σ0 within 1 sd of Arm B on ≥ 4 fires; Arm B-20 loses to Arm B beyond
  1 sd on Bear and Pier (Arm B's own learned medians there are ±43°).
  *Stop rule:* none — this experiment runs regardless of E44's stop
  rule; its write-up must name the mechanism in one sentence or say it
  cannot. Runner: `exp_r7_e45.py` → `exp45_wind_rot_mechanism.json`.
  Write-up: `validation/experiments/51-e45-wind-rot-mechanism.md`.

  **E46 — station hourly wind as the driver input.** *Question:* is the
  coarse ERA5 daily-mean wind the real problem, or does the gene do more
  than repair a bad input? *Design:* new knob `SMC_WIND_SOURCE=era5|
  station` (default `era5`, unchanged behaviour) — under `station` the
  driver receives the station vector mean over each window instead of
  the ERA5 daily vector, same daily cadence, falling back to ERA5 for
  any window with a station log gap (fallbacks counted in the report);
  unit test with a synthetic log of known mean. Three arms, 3 seeds
  (0–2), six fires, 2 workers, one arm per batch: (a) E33's recommended
  config (no arrival kernel) + station wind; (b) Arm B preset with
  `SMC_WIND_ROT_GENE` unset (no gene) + station wind; (c) Arm B preset
  (gene on) + station wind. *Score family:* one-window-ahead consensus
  IoU. *Noise floor:* Arm B's own sd from E44 for arms (b)/(c); E33's sd
  for arm (a). *Prediction, written before the run:* (a) ties E33
  everywhere (a round Bernoulli blob only cares about wind speed); (b)
  recovers most of the gene's gain on Ferguson and Chimney (E41: ERA5
  direction is wrong there and direction carries signal on those two);
  (c) beats (b) by less than 1 sd on ≥ 4 fires. If (c) beats (b) beyond
  2 sd on ≥ 3 fires, the gene does more than repair the input. *Stop
  rule:* none. Runner: `exp_r7_e46.py` → `exp46_station_wind_input.json`.
  Write-up: `validation/experiments/52-e46-station-wind-input.md`.

  **E47 — arrival + rear_focus + spotting genes.** *Question:* does
  spotting (E43: widens the reachable wedge under Bernoulli) still add
  reach once combined with the arrival kernel, or is it redundant with
  what the arrival kernel already reaches? *Runs only if E44 did not
  trip its stop rule.* *Design:* two batches — illumination (`map` mode,
  Arm B preset + `SMC_SPOT=1`, six fires, one batch, compared against
  E44's own E37b-at-4× table, not re-run) and forecast (Arm B preset +
  `SMC_SPOT=1`, seeds 0–2, six fires, 2 workers). *Score family:*
  reachable-wedge coverage (illumination); one-window-ahead consensus
  IoU (forecast). *Noise floor:* Arm B's own sd from E44. *Prediction,
  written before the run:* Ferguson coverage rises above Arm B alone;
  forecast IoU ties Arm B on all six (spotting adds reach the filter
  rarely needs). *Stop rule:* a forecast loss beyond 1 sd on any fire
  means the wider prior costs more than reach buys (reported, not a
  gate on anything downstream — E47 is the last experiment in this
  round). Runner: `exp_r7_e47.py` →
  `exp47_arrival_spotting_illuminate.json`,
  `exp47_arrival_spotting_forecast.json`. Write-up:
  `validation/experiments/54-e47-arrival-plus-spotting.md`.

  **E48 — why Brattain fails under arrival without the gene.**
  *Question:* E30b Arm A lost −13 sd on Brattain, the fire predicted to
  need the least help — why? *Design:* read-mostly. Re-run seed 0 on
  Brattain only, Arm A and Arm B (2 runs, 2 workers), with per-window
  diagnostics enabled via a new opt-in knob `SMC_DIAG=1` (added fields
  only; no existing field changes): consensus perimeter vs truth per
  window, learned p0 and `wind_scale` (and `wind_rot_deg` for B)
  trajectories, the ERA5 wind vector per window, the station vector
  mean per window (`station_vector_mean` already exists), and a
  head-vs-flank decomposition of the miss (cells missed downwind of the
  ignition centroid vs cross-wind). Compared against the E41 Ellipse
  null's per-window series on Brattain (not re-run). *Score family:*
  none — this is a finding, not a scored comparison. *Noise floor:* not
  applicable. *Prediction, written before the run:* ERA5 direction is
  right on the daily mean but wrong on the two or three windows that
  carry most of the burned area, and Arm B's gene diversity covers
  exactly those windows. *Stop rule:* none. Runner: `exp_r7_e48.py` →
  `exp48_brattain_arrival_diagnosis.json`. Write-up: `validation/
  experiments/49-e48-brattain-arrival-diagnosis.md`.

  **E49 — the containment operator under the 4× clock.** *Question:*
  Chimney's contained fraction fell to 0.56–0.59 under both E30b arms
  (E33: 0.94) while IoU rose — is the containment operator's period,
  growth-rate window, or per-tick rate constant failing to scale with
  `SMC_STEPS_SCALE`? *Design:* first, read the operator and
  `SMC_STEPS_SCALE` handling and write down each constant and whether it
  scales. If something does not scale that should: fix it (opt-in via
  the preset, so E44's own reports stay reproducible) and re-run
  Chimney seeds 0–4 on Arm B (5 runs, 2 workers) — the branch this
  task's skeleton (`exp_r7_e49.py`) declares. If everything scales:
  instead sweep the containment threshold on the four calibration fires
  only (Bear, Brattain, Buck, Chimney), one seed, three values, holdout
  untouched — threshold values are Task 7's own choice and are not
  pre-registered here. *Score family:* contained fraction and
  one-window-ahead consensus IoU, before/after (fix branch) or across
  threshold values (sweep branch). *Noise floor:* Arm B's own sd from
  E44 (fix branch only; the sweep branch is a calibration-fire
  comparison, not judged against the six-fire noise floor). *Prediction,
  written before the run:* the fraction is correct, not a bug — the
  fire really grows for longer under the faster clock, and the ICS-209
  lead E42 measured (13–21 days) shrinks toward the real 5–10 days.
  *Stop rule:* none. Runner: `exp_r7_e49.py` →
  `exp49_containment_4x_clock.json`. Write-up: `validation/experiments/
  53-e49-containment-under-4x-clock.md`.

- v1.8 (2026-09-11, before the E41 run): a third dummy forecaster, the
  **Ellipse null**, declared alongside persistence and the Circle. The
  Circle grows a chamfer distance field from the ignition and thresholds
  it to match the observed area each day; the Ellipse does the same with
  a wind-oriented minimum-travel-time growth (Dijkstra, cost
  `|offset| / r(θ)`, `r(θ) = b² / (a − c·cos θ)`, `a = LB(U)`, `b = 1`,
  ignition at the rear focus), so it answers a narrower question than the
  fire model itself: does wind *direction*, in the inputs this campaign
  actually has, carry any shape signal at all? Three variants, all
  pre-registered: **ellipse_era5** (the scenario's ERA5 daily wind — the
  campaign's default input), **ellipse_station** (vector mean of
  `station_hourly.json` over the window; `None` where that file is
  absent), **ellipse_era5x3** (ERA5 wind speed × 3, direction unchanged —
  a sensitivity probe, not a forecaster, following E9c's finding that
  strengthened wind is what Chimney wanted). `LB(U) =
  0.936·e^{0.2566U} + 0.461·e^{−0.1548U} − 0.397` (Anderson 1983, U at
  10 m, clamped to [1, 8]). Scored exactly as the Circle: IoU and binary
  Brier at each observation, mean and final over the series. New `nulls`
  mode in `wildfire_smc` (no ensemble members) reports all three
  variants plus persistence and the Circle for all six fires; the Circle
  and persistence continue to be computed inside `open`/`assim` mode as
  before, joined there by `ellipse_era5` only (as `ellipse_iou` /
  `brier_ellipse`) so every future ensemble table carries it too.
  **Prediction, written before the run:** ERA5 winds on these six fires
  are 0.5–0.7 m/s so `LB ≈ 1.1` — ellipse_era5 ties the Circle (inside
  the E33 sd) on all six fires. ellipse_station moves Chimney and
  Brattain by more than sd, in *some* direction (station wind is real
  but measured 40–70 km from the fire). ellipse_era5x3 beats the Circle
  on Chimney only. If instead ellipse_station or ellipse_era5x3 beats
  the Circle on Brattain, Ferguson or Pier, wind direction carries shape
  and the kernel work E30 would spend on it is worth its cost; if every
  variant only ties the Circle on those three, the *inputs* are the
  blocker and E30 is not.
  **Post-hoc addendum (2026-09-11, after seeing the E41 results, before
  any further run):** review of the rear-focus results found that the
  template alone bakes in a front/back rate skew, `(a + c) / (a − c)`,
  from `LB` before any visible stretch — at `LB = 1.1` (about Ferguson's
  own peak) that ratio is already ≈ 2.4. A win on Brattain or Ferguson
  could therefore come from the ellipse encoding "which way the fire
  runs" (a sign) rather than "how stretched" (a magnitude). One control
  variant is added, **not pre-registered**, to separate the two:
  `ellipse_era5_centred` — same ERA5 wind and the same `LB(U)`, but the
  ignition at the ellipse's *centre* instead of its rear focus, so head
  and back rates are equal (both `a`) and only the flank rate (`1`)
  differs: `r(θ) = a·b / √(b²·cos²θ + a²·sin²θ)`. It cannot express a
  front/back sign at all — if it still beats the Circle where the
  rear-focus version does, the stretch itself carries signal; if it
  only ties where the rear-focus version beats, the sign was doing the
  work. Same scoring, same six fires; no change to the three
  pre-registered variants' numbers.
  Added after E41 (2026-09-11, before the E42 analysis; no new runs):
  **E42 posterior trajectories** — the E33 five-seed reports already on
  disk carry everything needed to ask two more questions of the filter's
  posterior: does the learned p0 drift systematically with day-of-fire
  across fires, and does the learned containment operator agree with
  ICS-209's reported percent contained? Per-observation mean ± sd across
  the five E33 seeds of p0, duration, wind × and area ratio, aligned by
  day since the first mask; and, per fire, every member's FINAL
  `(contain_a, contain_b)` run against the OBSERVED daily growth from
  truth to get a daily hazard and a cumulative contained-by-day curve,
  compared with ICS-209's `PCT_CONTAINED_COMPLETED` on the same day axis.
  **Prediction, written before the analysis:** p0 posterior falls over
  days 1–5 on Bear, Buck, Pier (the slow fires) and rises on Ferguson
  (the model under-burns it); wind × is flat everywhere (E26: the kernel
  does not listen). The learned containment curve reaches 50 % contained
  members 5–10 days *earlier* than ICS-209 reports 50 % containment on
  Bear and Buck (E21: the model needs stopping before crews report it).
  Added after E38 ran (2026-09-11, before the E39 run): **E39
  area-ratio-gated immigrant reset** — E38's plain reset repaired Buck
  seed 3's lock-in (+0.105) but cost Pier a small mean loss (−0.008, 4/5
  seeds) and a little Brier on four fires, because it re-ignites
  immigrants even where the fire has genuinely stopped. `EnsembleConfig`
  gains `immigrant_reset_gate: Option<f64>` (default `None`): when set,
  an immigrant gets a fresh state only if the last assimilation's area
  ratio (mean member burned area over observed burned area) is **below**
  the gate value; `None` leaves the plain `immigrant_reset` bool in
  charge unmodified, so E38's run is reproduced bit-for-bit. Gate value
  pre-registered at **1.0**. Env knob in `wildfire_smc`:
  `SMC_IMM_RESET_GATE=1.0` (implies reset on, subject to the gate).
  Five seeds matched to E33/E38, all six fires.
  **Prediction, written before the run:** Buck seed 3 keeps its +0.10;
  Pier's mean loss (−0.008, 4/5 seeds) disappears (inside sd); Brier
  cost of E38 on Brattain/Chimney/Pier halves or vanishes; everywhere
  else ties E33.
  Added after E39 (2026-09-11, before the E40 run): **E40
  observed-perimeter immigrants** — E38 showed that immigrants starting
  *uncontained* can repair a locked-in population (+0.105 on Buck seed
  3) but E39 showed that gating that reset on the area ratio makes it
  inert: by the time the population under-predicts, the members' grids
  already have no burning cells left, so an uncontained flag on a
  copied burnt-out grid cannot restart anything. The missing piece is
  the *state*, not the flag: an immigrant needs a grid with a live
  burning rim, and the only honest source of one at assimilation time is
  the observed perimeter itself — standard particle-filter state
  correction (Rochoux et al. 2014; Xue, Gu & Hu 2012). Engine config
  `immigrant_source: Prior | Observed` (default `Prior`). With
  `Observed`, at each assimilation the immigrants' grid state is built
  from the observed mask: burned cells → the model's burned type;
  burned cells with at least one unburned fuel neighbour (the rim) → the
  burning type with age 0; everything else untouched from a fresh
  scenario grid. Genome fresh from the prior, driver state fresh
  (uncontained). The `assim` runner already builds an observed `Sim`
  (`observed_sim`) for scoring — reuse it. Env: `SMC_IMM_SOURCE=observed`.
  Five seeds matched to E33, all six fires, reset and gate (E38/E39)
  left off so the effect is isolated.
  **Prediction, written before the run:** Bear mean forecast IoU up by
  > sd (its late-day stall repaired), Ferguson up by > sd (days 4–7
  catch-up); Chimney and Buck ties; Brier worse by ≤ 0.005 on Pier (rim
  members disagree on where nothing will burn). Consensus never falls
  below persistence on any day.
  **Post-hoc addendum (2026-09-11, after seeing the E40 results, before
  the E40b run):** review of E40's own results found that its headline
  comparison (E40 vs the E33 twin, both scored against *plain*
  persistence — the frozen ignition mask) was the wrong dummy competitor
  for a mode that reseeds part of its population from the mask at
  t_{k−1} every window. The fair competitor sees exactly that and no
  more: **lagged persistence** (the observed mask at t_{k−1}, unchanged,
  scored at t_k) and **lagged Circle** (the Circle's own chamfer growth,
  area-matched, but re-seeded from the mask at t_{k−1} instead of the
  fixed ignition mask every window). Both computed only for the second
  scored window on: `lagged_persistence_iou`, `brier_lagged_persistence`,
  `lagged_circle_iou`, `brier_lagged_circle` in `wildfire_smc`'s per-window
  report (`assim` and `open` modes), plus their series means. Checked
  against E40's own raw reports (no rerun needed — both nulls depend only
  on the truth, not the ensemble): lagged persistence alone (0.88–0.96
  mean IoU across the six fires) beats E40's own consensus (0.50–0.67) by
  0.3–0.4 on every fire, because E40 only ever corrects 20 % of the
  population (`immigrant_source: Observed`, renamed `state_correction:
  Immigrants` in the same change that adds this) — the other 80 % is an
  uncorrected forecast dragging the consensus down. **E40b** (post-hoc,
  not itself pre-registered as a headline result): a new
  `state_correction: All` applies the same grid rebuild to *every*
  resampled child, each keeping its own learned/mutated genome, so
  learning continues while the state is corrected every window. Env:
  `SMC_STATE_CORRECTION=all`. Five seeds matched to E33/E40, all six
  fires, reset and gate left off, same base configuration.
  **Prediction, written before the E40b run:** with every child
  state-corrected, consensus IoU ≥ lagged persistence on every window
  where the fire grew, and it beats the lagged Circle on Ferguson and
  Brattain, ties elsewhere.
  Added after E42 (2026-09-11, before the E43 run): **E43 spotting
  illumination** — E37 found the model's reachable growth × elongation
  region is a wedge (small fires can be any shape, large fires are
  round) and left open whether spotting (E7) is a mechanism that gives a
  *large* fire more reach, i.e. moves the wedge. `wildfire_smc`'s `map`
  mode gains `SMC_SPOT=1`: switches spotting on in the config's wildfire
  model and adds two genes to the existing spread genes (p0,
  burn_duration, wind ×): `model.spotting.p_spot`, log-uniform
  0.001–0.005 (E7's pre-registered spotting space, `exp_spotting.py`'s
  SPOT_GRID: lo 0.001, mid 0.005, far 0.002 — already shown to move
  burned area 2–4×, so wide enough to show an effect without
  extrapolating past what has been tested), and
  `model.spotting.median_distance`, linear 2–20 cells (E7 tested 5–20;
  widened down to 2 to also cover a jump barely ahead of the front),
  both within `SpottingParams`'s declared bounds. Same MAP-Elites
  settings as E37: batch 32, 30 generations, iso + line emitter, 5 days
  of the scenario's own weather, growth axis 0–0.10 (20 bins),
  elongation axis 1–4 (20 bins), no stopping rule, no objective. All six
  fires (including the holdout pair); E37's own archive numbers are
  reused for the comparison rather than re-run.
  **Prediction, written before the run:** Coverage of the archive rises
  on every fire (spotting adds growth), but the maximum elongation at
  the observed size rises by < 0.2 on Brattain/Ferguson/Pier: spot fires
  merge into a rounder mass. The three dots stay outside the wedge.
  **Post-hoc addendum (2026-09-11, after seeing the E43 results, before
  the replay diagnostic run below): a component check.** E43's own
  elongation (`Metric::Elongation`) is a second-moment measure over
  *every* tracked cell with no connectivity distinction, and the archive
  stores no mask or component stats — so a round main body plus a
  handful of spot-fire embers landed well downwind can read as
  "elongated" exactly the way a genuinely stretched single blob would,
  which would undercut E43's "Ferguson/Pier inside the wedge" headline.
  A new `replay` mode in `wildfire_smc` (`SMC_MAP_REPLAY=<path>`)
  re-evaluates a stored elite's genome and reports connected-component
  stats (8-connected): total burned cells, the largest component's share
  of that total, the number of components, and the largest component's
  own elongation next to the whole-set figure. Run for the top 5 elites
  by elongation at or above the observed day-5 growth, on each of the
  six fires, 3 fresh seeds per elite (the archive does not record which
  search-time evaluation produced a given elite, so these are
  re-evaluations of the stored genome, not reproductions of the original
  stochastic run — see `Evolution::evaluate_genome_sim`'s own doc
  comment).
  **Prediction, written before the run:** on Ferguson and Pier the
  largest-component fraction of the best at-size elites is < 0.7 and
  their largest-component elongation falls below the observed value; on
  Brattain it is unchanged.
  Added after E43 (2026-09-11, before the E30a run): **E30a arrival-time
  kernel** — E37 found large fires come out round under the Bernoulli
  rule because it gives every unburned cell one ignition-probability
  roll per tick per burning neighbour, which saturates in every
  direction once enough neighbours are burning. `WildfireParams` gains
  `spread: "bernoulli" | "arrival"` (default `bernoulli`, nothing
  existing changes): under `arrival`, the same per-direction wind/slope
  factor is used as a rate instead of a probability, accumulated into a
  new per-cell `heat` buffer (0 at attach) until it reaches 1, so
  direction sets ignition *time* and the head:flank *speed* ratio
  (`dir[head] / dir[flank]`) survives regardless of size or burn
  duration. `arrival_jitter` (default 0.2) is a per-cell log-normal
  multiplier, one draw per cell for the whole run, keeping ensembles
  diverse without breaking reproducibility. Independently,
  `WildfireParams` gains `wind_law: "exponential" | "rear_focus"`
  (default `exponential`): the existing law
  `exp(c1·v)·exp(c2·v·(cosθ−1))` is kept for E30a's own c2 scan;
  `rear_focus` is a new Anderson-1983 ellipse template, `dir[j] =
  exp(c1·v) · r(θ_j)/r_max`, `r(θ) = 1/(a−c·cosθ)`, `a = LB(v)` (this
  plan's own `LB(U)` formula, clamped [1, 8]), `c = √(a²−1)`, `r_max =
  a+c` — at `v = 0`, `a = 1`, `c = 0`, so it reduces to the exponential
  law's own no-wind case exactly. Added per a controller ruling made
  after E41 showed the exponential law's head:back ratio is
  only `exp(2·c2·v)` = 1.17 at 0.6 m/s, far too weak to reproduce the
  front/back rate skew (≈ 2.4 at `LB ≈ 1.1`) that E41 found actually
  carries the wind-direction shape signal in the six fires' real (ERA5)
  wind. Three measurements, all on a flat/uniform grid so terrain and
  fuel heterogeneity cannot contribute shape (isolating the wind kernel
  alone): (1) the E19 flat-grid front-speed table (`wildfire_ros`, 240×120,
  full-height burning column), both spread rules, wind 0/2/5/8 m/s, p0
  0.12/0.22/0.44, burn duration 5/10, 3 seeds; (2) point-ignition
  elongation (E12's second-moment measure) vs. size: a 3×3 ignition at
  the centre of a 400×400 uniform grid, 8 m/s toward +x, elongation
  recorded at 2/5/10/20 % burned, both spread rules, 3 seeds; (3) a
  length-to-breadth table on the arrival rule at 10 % size for 2/5/8
  m/s: `c2` ∈ {0.131, 0.2, 0.3, 0.45} under the exponential law, plus
  the rear-focus law, against this plan's own `LB(U)`, and the
  closed-form head:back ratio at 0.6 m/s for both laws (no simulation
  needed for that number). Runner `exp_r6_arrival_flat.py`; figure
  `e30a()` in `figures_r6.py`.
  **Prediction, written before the run (from the task brief):** Bernoulli
  elongation at 8 m/s falls from > 1.5 at 2 % to < 1.3 at 20 %; arrival
  elongation stays within ± 0.15 across sizes at every wind. The default
  c2 (0.131) gives LB ≈ 1.6 at 8 m/s (Anderson: 7.9); c2 ≈ 0.3–0.45 is
  needed to approach Anderson at 5 m/s, and no single c2 matches at all
  three winds because the factor is exponential in v.
  **Prediction, written before the run (addendum, after the controller
  ruling that added the rear-focus law):** the rear-focus law gives
  head:back ≥ 2 already at 0.6 m/s, and its LB is within 20 % of
  Anderson at 2, 5 and 8 m/s at every size, under the arrival rule;
  under the Bernoulli rule its elongation still collapses with size.
  **E30a v2 (2026-09-11):** the rule was redefined as minimum travel
  time after v1 showed a death threshold and a saturated head; v1
  numbers are kept in the file for the record. **Prediction:**
  elongation flat with size at every wind for both laws; jitter-0 LB
  equals cosh(c2·v) within 15 % under the exponential law and is within
  20 % of Anderson under rear-focus at 2/5/8 m/s; p0 0.12 fires no
  longer die.
  **E30a measurement fix (2026-09-11):** measurement domain changed to
  upwind ignition on 900×300 after the centred grid's head hit the
  boundary at ≈ 10 % size; rule unchanged. The centred 400×400 domain
  put the boundary only 200 cells from a centred ignition in every
  direction; at 8 m/s under rear-focus the head's own cost is
  `1 / (p0 · exp(c1·v))` ≈ 5.8 ticks/cell (p0 = 0.12), so the head
  reached that boundary at ≈ tick 1,160 — almost exactly when an
  LB ≈ 7 shape's own area (`π·200²/7` ≈ 18,000 cells) crosses the old
  10 % checkpoint (16,000 cells of a 160,000-cell grid). The measured
  "collapse with size" in the v2 report above is this boundary
  artefact, not a property of minimum travel time. Checkpoints are now
  absolute burned-cell counts (2,000/5,000/10,000/20,000), not a
  fraction of the grid, each carrying a `boundary_contact` flag so a
  contaminated row can be identified rather than silently trusted.
  **E30a fix round 3 (2026-09-11):** documentation and two data points,
  no rule or measurement change. The rear-focus overshoot found by the
  measurement fix above is traced to a specific, reproducible mechanism:
  `rear_focus`'s `r(θ)` is the exact polar equation of Anderson's
  ellipse, but the arrival rule only samples it at the 8 grid
  directions, and at high eccentricity the ellipse's area collapses onto
  the single head direction faster than the grid can track — at `a=7`,
  `r(45°) ≈ 0.48` is already 29× down from the head's `r(0) = 13.93` and
  only 3.4× above the flank's `r(90°) = 0.14`. The resulting shape is a
  "needle" polygon strictly thinner than the ellipse (e.g. at `a=2`, the
  polygon's half-width at focus-frame `x=2.5` is 0.39 against the true
  ellipse's 0.93), and because shortest-path propagation on a fixed
  lattice scales its own per-tick reach polygon self-similarly (a convex
  polygon's Minkowski self-sum is a bigger copy, never a rounder one),
  the needle-vs-ellipse gap does not shrink as the fire grows — it is
  the same mechanism behind Table 2's "flat with size" finding, just
  read as "flat at the wrong (needle, not ellipse) level." Two new
  template points were added to the LB table at jitter 0 (template
  LB 1.2, wind 0.9690 m/s; template LB 2.0, wind 3.1771 m/s — wind
  values solved by bisecting `anderson_lb(v)` to hit the target exactly)
  to bracket the already-tested 1.5/3.2/7.0 points below and above. The
  file now states plainly that the (arrival, rear_focus) recommendation
  is validated for LB ≤ 1.5 only (wind ≲ 2 m/s in this model's units),
  which covers the six real fires' own ERA5 wind speeds (E41: 0.5–0.7
  m/s, `anderson_lb` 1.09–1.14) with margin; no prediction was
  re-registered since no rule changed, only the explanation and two
  bracketing measurements were added.
  **E30a v2b (2026-09-12):** diagonal cost double-count found in review
  and fixed; all v2 tables regenerated; rule otherwise unchanged.
  `step_chunk_arrival`'s cost had an extra `norm_j` factor on top of the
  one already inside `dir[j]`, doubling the diagonal-vs-cardinal cost
  ratio (`2×` instead of `√2×`) at every wind, including calm wind.
  Fixed (`cost_j = jitter / (p_base · dir[j] · slope)`), covered by a
  new isotropy test. Most of fix round 3's measured rear-focus overshoot
  was this bug, not the 8-direction hull effect it described: the fix
  removes 17 of 16.5 overshoot points at template LB 1.2 (now −0.8 %,
  i.e. a hair *under* Anderson) down to 138 of 373 points at LB 7 (still
  +235 % over — the hull effect alone, now isolated). At the ensemble's
  real operating ceiling (`wind_scale` gene × ERA5, LB ≤ ~1.3) the
  overshoot is now ≈ 0 %, not the previously-stated "≤ ~20 %". The
  exponential law's own closed-form check moved the other way (13 %
  short of `cosh(c2·v)` under the bug → 31.5 % short fixed; that unit
  test's bound widened from 15 % to 35 % with an explicit comment). No
  rule, law, or parameter changed — only the arrival rule's internal
  cost arithmetic and the numbers it produces.
  **E30a v2b, fix round 5 (2026-09-12):** the exponential-law "closed
  form" `cosh(c2·v)` used since fix round 1 was itself a controller
  error (it assumed the shape's half-width sits at the 90° flank rate;
  the true minimum-travel-time half-width is `max_θ [e^(c2·v·(cosθ−1))·
  sinθ]`, which peaks near 50° at c2·v ≈ 1, not 90°); corrected in the
  unit test (a 1°-step scan, bound restored to 15 %) and the experiment
  file. Against the correct closed form the exponential law is within
  3 % at 8 m/s and 2 % at 5 m/s, not "31.5 % short" — its template is
  simply nearly round at every tested wind by construction, which is
  the quantitative form of E19/E37's finding, not a new shortfall.
  Added after E30a (2026-09-12, before the E30 run): **E30 arrival-time
  kernel on the six real fires (Task 8)** — E30a (fix rounds 1–5)
  validated `spread: arrival` with `wind_law: rear_focus` for LB ≤ 1.5,
  which covers these six fires' actual operating range: ERA5 wind
  0.5–0.7 m/s times the ensemble's own `wind_scale` gene (ceiling ×1.5)
  gives LB ≤ ~1.3, comfortably inside the validated regime and nowhere
  near its LB ≈ 1.5 boundary. `wildfire_smc` gains four independent env
  knobs (`configure_spread`, following the `SMC_SPOT`/`enable_spotting`
  pattern): `SMC_SPREAD=bernoulli|arrival`,
  `SMC_WIND_LAW=exponential|rear_focus`, `SMC_C2=<f64>`,
  `SMC_ARRIVAL_JITTER=<f64>`; any left unset keeps the scenario config's
  own default. Two parts: (1) **E37b acceptance** — re-run E37's
  illumination (`map` mode, identical settings: batch 32, 30
  generations, 5 days of the scenario's own weather, growth axis
  0–0.10, elongation axis 1–4, no objective, no stopping rule) with
  `SMC_SPREAD=arrival SMC_WIND_LAW=rear_focus`; the wind × gene range is
  unchanged (0–1.5). `c2` is unchanged from its default (0.131) —
  `rear_focus` has no `c2` knob at all (`c2` only shapes the
  `exponential` law), so there is nothing to set there; `arrival_jitter`
  is likewise left at its default (0.2). (2) **E30 forecast** — five
  seeds (0–4) × six fires, `r5_common.BASE_ENV` (assim, β 10, σ 0.2,
  immigrants 0.2, containment-only stopping) plus `SMC_SPREAD=arrival
  SMC_WIND_LAW=rear_focus`; E39's area-ratio gate (Task 4 finding:
  inert, left off) and E40/E40b's state correction (Task 5/6 finding: a
  different score family, left at `none`) are both off (`BASE_ENV` sets
  neither) so the kernel change is isolated, matching E33's twins seed
  for seed. Spotting (E43) is a separate, not-yet-combined mechanism and
  stays out of scope here (named under "Questions this raises").
  **Prediction, written before the run:** E37b — the maximum elongation
  at the observed size rises above the observed value on at least two
  of Brattain, Ferguson, Pier (their dots move inside the reachable
  region) *if* the scenario wind, scaled by the gene, reaches ≥ 1.0
  m/s; with the six fires' actual ERA5 winds of 0.5–0.7 m/s the
  head:flank ratio is still < 1.2, so the honest expectation is that
  only Chimney and Brattain move and the input wind speed itself (not
  the kernel) is the remaining blocker (E41 already showed the wind's
  *sign*, not its magnitude, carries the front/back skew at these
  speeds). Forecast: Chimney's mean consensus IoU rises by more than
  its E33 sd (0.012); Brattain and Ferguson rise by ≥ 0.02; Bear, Buck
  and Pier tie (inside their own E33 sd); Brier is not worse than
  E33's by more than 0.005 on any fire.
  Added after E30 (2026-09-12, before the E30b run): **E30b uncapped
  clock, wider speed prior, and a learned wind-direction offset (pilot,
  one seed, two arms)** — E30's own diagnosis identified two confounds
  and a real finding: (1) under the arrival rule the head moves at most
  one cell per tick, which at E30's 50 ticks/day caps the front at
  1.5 km/day; Brattain's day-5 shape needs its rear-focus head to cover
  ≈ 480 cells in 250 ticks (≈ 1.9 cells/tick), unreachable by
  construction at the E25 prior's own top (p0 0.6 ≈ 0.62 cells/tick);
  (2) the learned p0 sat at 0.30–0.43 on every fire (E33: 0.19–0.34),
  nowhere near the prior's edge, so the filter was not speed-starved by
  the *prior* alone — something else was costing it IoU; (3) E41 already
  showed the ERA5 daily wind direction is wrong on Chimney and Bear and
  right on Ferguson and Brattain, and a directional kernel is punished
  by a wrong direction in a way a round Bernoulli blob never was — the
  *direction input*, not only the speed prior, is a live suspect. Both
  arms keep E30's base (arrival, rear_focus, c2/jitter default, gate/
  reset/state-correction off) and add a 4× clock: `SMC_STEPS_SCALE=4`
  multiplies the scenario's `steps_per_hour` (200 ticks/day, cap
  6 km/day) and, through the same field, the wildfire driver's
  `steps_per_day`, so one observation window still spans one day of
  forcing. `model.p0` prior widens to log-uniform **[0.02, 0.6]** at
  200 ticks/day (per-day head speed 4–120 cells/day, versus E30's
  4–31); `burn_duration` prior widens to **[20, 80]** (lifetime in
  hours unchanged); containment and wind × priors are unchanged (they
  act per day, not per tick). New prior file
  `validation/scripts/experiments/priors/arrival_x4.json`. **Arm A** is
  the above alone. **Arm B** adds a free, per-member gene
  `wind_rot_deg` uniform on **[−90, 90]** degrees
  (`SMC_WIND_ROT_GENE=90`; `cella_lib::wildfire::driver::GENE_WIND_ROT_DEG`),
  added to the forcing's wind *from*-bearing in the driver before it is
  written into the model (mod 360) — each member learns its own
  correction to the reported wind direction, on top of (not instead of)
  the existing fixed, whole-schedule `SMC_WIND_ROT_DEG` rotation.
  **Pilot scope:** forecasts only, seed 0 × six fires × two arms (12
  runs, 4 workers), judged against `exp33_noise.json` seed 0 and
  `exp30_arrival_fires.json` seed 0 with the E33 sd as the tie bar; no
  E37b re-run in the pilot. Wall time is reported per run.
  **Prediction, written before the run:** Arm A — Ferguson and Brattain
  (direction right per E41) recover to within sd of E33 or better;
  Chimney and Bear (direction wrong) still lose to E33 by more than
  2 sd; Buck and Pier tie. Arm B — Chimney recovers to at least E33
  (E9c showed rotating the wind toward the reported direction lifted it
  to 0.57) and Bear ties E33; the learned `wind_rot_deg` median on
  Chimney is greater than 45° in magnitude and less than 20° on
  Ferguson. If Arm B still loses to E33 on Chimney by more than 2 sd,
  the direction law is not the fix and the full E30b (five seeds plus
  E37b at 4×) is not run.
- v1.7 (2026-09-05, before the Round 5 runs E32–E37): a round about the
  *methods*, not the fire model. Base configuration = E31's recommended
  row (assim, β 10, σ 0.2, immigrants 0.2, containment only, M 32). Declared:
  E33 noise floor (5 seeds; per-fire sd becomes the bar for every later
  delta, replacing the 2-replicate E25d spread); E32 ensemble size M ∈ {8,
  16, 32, 64, 128}; E34 operator ablation (immigrants {0, 0.1, 0.2, 0.4},
  σ {0.1, 0.2, 0.4}, β {5, 10, 20}, genome crossover on/off — crossover is
  a new ensemble option); E35 prior width (narrow ±25 % around the E28/E31
  posterior medians, the E25 broad prior, very broad p0 0.02–0.95 /
  duration 2–60); E36 offline evolution fitted to the first 3 observation
  days only, then run forward and scored as forecasts on days 4+ against
  the filter's forecasts on the same days (the honest form of E20); E37
  MAP-Elites illumination of the fire model's knob space (growth × bounding
  -box aspect descriptors, no objective) to ask whether *any* setting
  reaches Brattain's observed elongation. Same metrics; same
  calibration/holdout split; all six fires reported because nothing is
  chosen per fire. A difference smaller than the E33 sd on that fire is
  reported as a tie.
  Added after E33 ran (2026-09-05, before E38 runs): **E38 immigrant
  reset** — E33 showed a seed in which every member was contained by day
  12 and the population could not recover because immigrants inherit the
  parent's contained flag; E38 runs the base configuration with
  `immigrant_reset` on (five seeds, so it is judged against E33's own
  five), predicting a gain on Buck seed 3 and a tie elsewhere.
- v1.6 (2026-09-05, before the E31 run): the wildfire-specific ensemble
  (`WildfireEnsemble`, `prior` block) was replaced by the model-agnostic
  `cella_lib::explore` engine with a `WildfireDriver` supplying the weather
  schedule, decay and containment roll (`docs/explore.md`). E31 is declared
  as a **replication, not an experiment**: re-run E25 (assimilating, 20 %
  immigrants) and E28 (containment only, decay off) through the unchanged
  runners `exp_smc_imm.py` / `exp_containment_op.py`. Acceptance: six-fire
  mean consensus IoU within 0.02 and mean Brier within 0.005 of the
  recorded rows (E25d replicate spread ≤ 0.02 is the bar). Bit-identity is
  not expected: the member seeds and gene draws come from a different
  generator. No metric, baseline or split changes.
- v1.5 (2026-09-04, before E26–E28 runs): declared the terrain wind field
  (mass-consistent downscaling, layer depth ∈ {150, 300, 600} m), painted
  retardant (density multiplier ∈ {0.02, 0.05, 0.1}, recovery ∈ {24, 72, ∞} h)
  and a containment-probability operator inside the ensemble (daily
  stochastic termination by growth rate, parameters in the prior) as the
  next mechanisms under test. Their sources are in
  `experiments/research-decline-wind-suppression.md`. Same metrics; same
  calibration/holdout split; ensemble runs report all six fires because no
  parameter is chosen per fire.
- v1.4 (2026-09-04, before E24/E25 runs): ensemble metrics declared for
  the Monte Carlo and assimilating-ensemble experiments. A probability map
  (fraction of members burned per cell) is scored by (a) **Brier score**
  against the observed mask, next to the Brier of the deterministic nulls
  (persistence, radial — each 0/1); (b) **consensus IoU** (cells with
  probability ≥ 0.5); (c) best-threshold IoU over 0.1..0.9 (reported, but
  it peeks at the truth to pick the threshold — diagnostic only); (d) mean
  member IoU. In assimilation mode every score at t_k is a
  **one-window-ahead forecast**: members are resampled on the observation
  at t_{k−1} and scored on t_k before seeing it. One broad prior for all
  fires (p0 ∈ [0.08, 0.6] log-uniform, dur ∈ [5, 20], τ ∈ [2, 100] d
  log-uniform, wind × ∈ [0, 1.5]); nothing is chosen per fire, so the
  holdout pair is scored alongside the calibration four.
- v1.3 (2026-09-02, before E20 runs): search space for the per-fire
  evolutionary calibration (E20) declared: p0 ∈ [0.03, 0.6],
  burn_duration ∈ [2, 20], containment τ ∈ [2, 200] days (200 ≈ off),
  station-wind multiplier ∈ [0, 4], moisture of extinction M_x ∈ [10, 200] %
  (200 ≈ off), grass:timber veg ratio ∈ [0.3, 4]. Objective: mean IoU over
  the series, 1 seed during search, 3-seed verification of the winner.
  Purpose is *learning which parameters transfer* between fires, not a
  per-fire score; per-fire optima are reported only next to the
  transferred (median) recipe on the holdout.
- v1.2 (2026-09-02): first holdout report (E16c). The containment decay
  `p0 × exp(−t/τ)` is recorded as a *calibrated suppression proxy* (τ and
  the p0 multiplier chosen on the calibration fires); its holdout result
  (Pier +0.14, Ferguson flat) is the number to quote. Observed station
  weather (`station_hourly.json`) is an optional input with its own
  `provenance.weather`; committed scenarios keep ERA5.
- v1.1 (2026-09-01): added §2.1 "Weather inputs: inspect before you
  compare" and process rule 6, after Round 2 showed the wind maths was
  correct but the ERA5 daily-mean wind input was too weak to act and, on
  Chimney 2016, pointed the wrong way on the run days. Also records the
  switch of `wind_dir_deg` (toward, 0° = +x, our own convention) to
  `wind_from_deg` (meteorological from-bearing) and scenario format v2;
  the conversion is exact and every reported score is unchanged.
- v1 (2026-08-14): initial plan. T0 split fixed (calibrate: Bear, Brattain,
  Buck, Chimney; holdout: Ferguson, Pier). Metrics, nulls, search space
  pre-registered. Starting-point record: uncalibrated model loses to the
  area-matched radial null on Bear 2020 (0.124 vs 0.524 final IoU).

## 10. Roadmap after v1

**Current (after Round 6, 2026-09-12) — in order:**

1. The full five-seed E30b on Arm B's configuration (4× clock, widened
   p0/burn_duration prior, the learned `wind_rot_deg` gene), plus E37b
   re-run at the same 4× clock.
2. The E30b mechanism ablations: Arm B with mutation σ = 0 on
   `wind_rot_deg` (diversity-only test), and Arm B with the gene's range
   narrowed to ±20° (learned-correction-only test).
3. Arrival + rear_focus + spotting genes (E43) together — not yet
   combined.
4. A finer angular neighbourhood, or a fitted template-LB correction, for
   the rear-focus hull overshoot that remains at high wind (LB > 1.5).
5. A fuel term in the containment operator, and the ICS-209 check's own
   follow-up (does ICS-209's containment-line lag scale with fire size or
   fuel type?) — carried from Round 4/5.
6. Pre-existing gap: crate coverage ≈ 98.3 % vs. the 99 % gate. (Two other
   items that used to sit here — `make clippy` not linting `cella_lib`'s
   own examples, and `wildfire_smc.rs`'s size — were fixed in Round 7
   Task 1: `make clippy` now also runs from `cella_lib/`, and
   `cella_lib/examples/wildfire_smc/` is a small module tree instead of
   one 2,420-line file.)

Full context for each item: `validation/experiments/round-6.md`, "Still
open after this round."

**Original v1 roadmap (2026-08-14), superseded in substance by each
round's own "Still open" section but kept here for the record:**

1. Calibration campaign on the T0 calibration set (§6), report on the T0
   holdout.
2. `rasterize_isochrones.py` (shared GDAL tool) → PT-FireSprd + GOFER truth;
   Dogrib inputs converter + observed-perimeter truth (Prometheus sample).
3. Arrival-time metrics against GOFER; growth-rate curves.
4. Input upgrades driven by measured failures, in whatever order the
   failures rank them: canopy-cover density, fuel moisture proxy, per-cell
   wind (WindNinja-downscaled), suppression masks.
