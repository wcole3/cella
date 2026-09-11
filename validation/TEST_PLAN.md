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

1. Calibration campaign on the T0 calibration set (§6), report on the T0
   holdout.
2. `rasterize_isochrones.py` (shared GDAL tool) → PT-FireSprd + GOFER truth;
   Dogrib inputs converter + observed-perimeter truth (Prometheus sample).
3. Arrival-time metrics against GOFER; growth-rate curves.
4. Input upgrades driven by measured failures, in whatever order the
   failures rank them: canopy-cover density, fuel moisture proxy, per-cell
   wind (WindNinja-downscaled), suppression masks.
