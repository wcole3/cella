# Glossary for the experiment log

Every experiment file links here. Terms are grouped, not alphabetical, so
that reading one group top to bottom teaches the idea. Each entry is one
to three sentences. If a term you meet in a file is missing here, that is
a bug in this page.

## The fires and the data

- **The six fires.** Six real US fires with daily satellite burn maps:
  Bear 2020, Brattain 2020, Buck 2017, Chimney 2016, Ferguson 2018, Pier
  2017. The table in [README.md](README.md) describes each one.
- **Calibration fires.** Bear, Brattain, Buck, Chimney. The four fires we
  are allowed to tune on. Fixed before any result was collected.
- **Holdout fires.** Ferguson and Pier. Never used to choose a setting.
  Their scores are the only unbiased test of anything we tuned. Marked
  with `*` or "(holdout)" in tables.
- **Truth / observed mask.** The satellite map of what had burned by a
  given day. One mask per observation day, cumulative (burned cells stay
  burned).
- **Observation day / window.** The daily times at which a mask exists.
  "Window" is the stretch between two of them. Day 0 is the ignition
  mask and is never scored (it would always score 1.0).
- **Arrival time.** For each cell, the hour it first burned. −1 means it
  never burned. All masks are derived from this one field.
- **Spatial accuracy.** The truth's own resolution, 375 m for this
  dataset. No score should be read as more precise than that.
- **ERA5.** A global weather reanalysis. Our wind input for every
  committed scenario: one daily average wind for the whole map. Gentle
  (0.1–3 m/s) and, on Chimney's run days, pointing the wrong way.
- **Station weather (NOAA ISD).** Hourly wind, temperature and humidity
  from the nearest airport weather station, 40–70 km from each fire.
  Loaded by `scripts/wind_station.py`. Optional input; scores stay on ERA5.
- **ICS-209 / percent contained.** The daily situation report a fire's
  incident team files, including "percent contained". Stored per fire as
  `containment.json`. Used as data and as a check, not as a driver.
- **Scenario.** The folder for one fire: `scenario.json` (identity, wind,
  provenance), `config.json` (the model set up on that landscape),
  `truth.json` (arrival times). Format in `../FORMATS.md`.

## Scores

- **IoU (overlap score).** Cells burned in both maps ÷ cells burned in
  either. 1.0 = identical, 0 = no overlap. Higher is better. The field
  standard, so we can compare with published models.
- **Mean IoU.** IoU on every observation day after day 0, averaged. Our
  headline number in Rounds 1–3. It rewards being right all along, not
  just at the end.
- **Final-day IoU.** IoU on the last observation day only. Noisier than
  mean IoU (one day instead of an average).
- **Sørensen.** A cousin of IoU (2 × overlap ÷ sum of areas). Rises and
  falls with IoU. Reported by the harness, rarely quoted.
- **Area ratio.** Simulated burned area ÷ observed burned area on the
  final day. Written `(×3.4)` after a score. ×1.0 is the right amount;
  ×3 is burning three times too much; ×0.3 is dying out. It tells you
  whether a score changed because the fire burned less or because it
  burned in better places.
- **Miss rate / false-alarm rate.** Of what really burned, the share the
  model missed; of what the model burned, the share that never did. The
  two failure directions that IoU folds into one number.
- **Head/flank miss decomposition.** Splits the consensus forecast's miss
  and false-alarm cells (the same two failure directions as above) each
  into two further counts by where they sit relative to the ignition
  centroid and the window's wind: `downwind_miss`/`crosswind_miss`
  (really burned, model didn't say so) and `downwind_false_positive`/
  `crosswind_false_positive` (model said so, didn't really burn) — see
  **Ignition centroid** below for what "downwind" is measured from.
  Answers a narrower question than a plain miss rate: is the model wrong
  about *how much* burned, or about *which way* it went? `SMC_DIAG=1`.
  E48.
- **Arrival MAE.** For cells burned in both maps, the mean absolute error
  in arrival time, in hours. Only meaningful when many cells burned in
  both, so read it beside the area ratio.
- **Brier score.** For a probability map: mean over cells of
  (probability − outcome)², outcome 1 if burned else 0. 0 is perfect,
  0.25 is "50 % everywhere". **Lower is better.** It rewards being
  confident only when right. Used from Round 4 on.
- **Consensus IoU.** For an ensemble: mark a cell burned when at least
  half the members burned it, then score that map with IoU. Our headline
  number in Rounds 4–5.
- **Mean member IoU.** The average IoU of the individual members. Usually
  far below the consensus; the crowd is better than its members.
- **Best-threshold IoU.** Consensus IoU with the threshold (0.1–0.9)
  chosen to maximise the score. It peeks at the truth, so it is a
  diagnostic, never a result.
- **The Circle (area-matched radial null).** A dumb forecaster: a disc
  grown from the ignition, sized each day to the observed burned area
  exactly. It knows how much burned but nothing about where. Beating it
  means the model knows *where*. Printed beside every score.
- **Persistence.** A dumber forecaster: the ignition never grows. A model
  below persistence is destroying information.
- **The Ellipse (wind-oriented, area-matched null).** A third dumb
  forecaster, next to the Circle: instead of growing a plain disc, it
  grows an ellipse stretched along the window's wind, sized each day to
  the observed burned area exactly, same as the Circle. It answers a
  narrower question than the fire model: does wind *direction*, in the
  inputs this campaign has, carry any shape signal at all? Three
  variants — `ellipse_era5` (the scenario's ERA5 wind), `ellipse_station`
  (the station log's wind), `ellipse_era5x3` (ERA5 speed × 3, a
  sensitivity probe). E41.
- **Rear focus vs. centred (Ellipse control).** The Ellipse's ignition
  sits at the ellipse's *rear focus* by default, which makes it faster
  downwind than upwind *even at a small LB* — a front/back "sign" a
  fire's true growth can match almost for free. `ellipse_era5_centred`
  (E41, post-hoc, not pre-registered) puts the ignition at the *centre*
  instead, so front and back are equal and only long-axis-vs-short-axis
  stretch remains; comparing the two separates "which way the fire runs"
  from "how stretched it is".
- **Two score families.** Rounds 1–3 quote single-run mean IoU. Rounds
  4–5 quote consensus IoU of a 32-member ensemble that learns from each
  day's mask before forecasting the next. They are not comparable: a
  single run is a lower bound on what the same model gives as an
  ensemble, and a forecast made after seeing yesterday is a different
  task from a run that saw nothing. Tables that mix them say so.

## Noise and honesty

- **Seed.** The number that starts the model's dice. Same seed, same
  picture, bit for bit. Different seeds show how much is luck.
- **Scan grade / verified.** One seed is scan grade: good enough to rank
  settings. Anything we keep is re-run with 3 seeds (Rounds 1–3) or
  judged against the 5-seed noise floor (Round 5).
- **Noise floor / sd / the bar / tie.** E33 ran the recommended ensemble
  with five seeds per fire. The standard deviation (sd) of those five is
  the noise floor. A change smaller than the fire's sd is a **tie**. The
  sd is "the bar" a gain must clear, on several fires at once.
- **Pre-registration.** Metrics, baselines and search spaces are written
  in `../TEST_PLAN.md` before the runs. Changing them needs a new plan
  version with the reason logged.
- **Verdicts.** KEPT: the change stays in the working configuration.
  REJECTED: it does not. FINDING: it was a measurement, not a lever.
  PASS: a replication reproduced the record.
- **Replication.** Re-running a recorded configuration through changed
  code to check the numbers come back. E31 is one.

## The fire model's knobs

- **p0.** Base chance per tick that fire jumps to a neighbouring cell.
  The main speed and size knob. Textbook value 0.58; our fires want
  0.1–0.4. **Only true under `spread: "bernoulli"`.** Under
  `spread: "arrival"` the same number is read as a *rate* in cells per
  tick (no saturation, so it never "tops out" the way a probability
  does) — the same numeric gene range (0.08–0.6) therefore describes
  much slower fires under arrival than under Bernoulli (E30a's flat-grid
  table: 0.256 vs 0.474 cells/tick at p0 = 0.12), a units mismatch E30
  found was not corrected before testing the arrival kernel on the six
  real fires.
- **Burn duration (dur).** Ticks a cell stays burning and can ignite its
  neighbours. Too short and the front outruns its fuel and dies.
- **veg_factor.** Per fuel class multiplier on p0 (grass burns easier
  than timber litter).
- **Density.** Per cell multiplier on p0. Used for canopy cover (E2),
  random patchiness (E16a) and painted retardant (E27).
- **Wind kernel (c1, c2).** The rule that scales spread by wind: roughly
  `exp(c1 × speed)` in the downwind direction, with c1 = 0.045 and
  c2 = 0.131 from Alexandridis 2008. At 1 m/s it is a 5 % nudge. It
  changes speed a little and shape almost not at all (E19, E37).
- **Spread rule (`model.spread`).** Which mechanism decides *when* a
  fuel cell catches fire. `"bernoulli"` (default): every tick, every
  unburned neighbour of a burning cell rolls independent dice, one per
  burning neighbour, at a *probability*. A probability saturates at 1,
  so a cell with enough burning neighbours ignites almost immediately no
  matter which direction they are in — the mechanical reason E37's large
  fires come out round. `"arrival"` (the **arrival-time rule**): the
  standard fire-CA minimum-travel-time idea — direction sets ignition
  *time*, not chance, via each cell's own **arrival tick**. E30a. (v1 of
  this rule, a per-cell "heat" accumulator, was superseded after fix
  round 1 found it had a death threshold and flattened direction ratios
  — see the E30a experiment file's "v1, superseded" section.)
- **Arrival tick (arrival rule).** Not to be confused with **Arrival
  time** above (the satellite ground-truth burn hour) — this is the
  arrival rule's own per-cell simulation state: the tick number a cell
  is scheduled to catch fire, `+inf` until a path to it exists, `0` for
  a cell that starts already burning or burned. Each tick, a
  still-unburned fuel cell with a burning-or-burned neighbour `j` (a
  burnt-out cell is still a known source) computes
  `arrival[cell] = min(arrival[cell], min_j(arrival[j] + cost_j))`,
  `cost_j = jitter / (p_base × dir[j] × slope[cell, j])`, clamped to
  `>= 1` tick, and ignites the first tick its own tick number reaches
  that value — one tick of local Dijkstra/eikonal relaxation. `dir[j]`
  is read as a *speed* instead of a probability here, and already
  carries its own built-in `1/norm_j` diagonal-distance correction
  (`norm_j` = 1 cardinal, `√2` diagonal), which is why `cost_j` needs no
  separate distance factor — dividing by a speed that already has
  distance divided out once gives exactly `travel time = distance /
  speed` (fix round 4 found and removed an earlier, extra `× norm_j`
  that had squared the diagonal-vs-cardinal cost ratio). `arrival_jitter`
  (default 0.2) is a per-cell log-normal multiplier on cost, one draw
  for the whole run (not per tick), keeping ensembles diverse without
  breaking reproducibility. Stored as `AtomicU32` bit patterns internally
  so the chunk-parallel stepper can write it behind a shared reference —
  each chunk only ever touches the cells inside its own contiguous range
  and every neighbour used as a source was already burning/burned
  *before* this tick, so there is no race. E30a v2b.
- **Wind law (`model.wind_law`).** Which formula produces the eight
  per-direction wind factors the spread rule (Bernoulli or arrival)
  reads. `"exponential"` (default): the wind kernel above; its head:back
  ratio is only `exp(2 × c2 × v)`, 1.17 at 0.6 m/s — too weak to
  reproduce the front/back rate skew E41 found in the six fires' real
  wind. `"rear_focus"`: an Anderson-1983 ellipse template — not the same
  object as the Ellipse *null forecaster*'s own rear-focus/centred pair
  above, though it is the same idea (ignition at the rear focus gives a
  faster head than tail even at a modest length-to-breadth ratio) now
  built into the fire model's own kernel instead of a diagnostic
  forecaster. At 0.6 m/s its head:back ratio is already ≈ 2.4–2.6. E30a,
  added per the controller ruling made after E41 showed this front/back
  rate skew was the signal actually carrying the wind-direction shape.
- **Wind multiplier (wind ×).** Scales the input wind speed before the
  kernel. ×0 = wind off.
- **Wind from-bearing.** Where the wind comes from, 0° = north, clockwise.
  The weather-report convention, used everywhere since Round 2.
- **ERA5-vs-station disagreement (angle).** How far apart, in degrees,
  ERA5's and the station log's wind "toward" bearings are for the same
  window — the *circular* difference, so 350° and 10° count as 20°
  apart, not 340°. A small angle means the two inputs agree on which way
  the wind blew that day; a large one flags a window where trusting the
  coarse ERA5 daily average alone is a real risk, not a hypothetical
  one — found on some of Brattain's biggest-growth windows. `SMC_DIAG=1`
  reports both bearings per window so this can be computed. E48.
- **Wind source (`SMC_WIND_SOURCE`).** Which weather log the driver reads
  its per-window wind vector from: `era5` (the default — the scenario's
  daily-average reanalysis wind, unchanged behaviour) or `station` — the
  hourly NOAA ISD log's vector mean over the same window
  (`station_vector_mean`, already computed for the Ellipse null's
  station variant, but not fed to the driver until this knob). A window
  whose station log has a gap falls back to ERA5 for that window; the
  report counts how many windows fell back. Distinct from the
  wind-direction offset gene below, which *corrects* whichever source is
  in use per member — this knob only changes which log is read. E46:
  separates "the input is coarse" from "the filter can't use the input
  it has."
- **Wind-direction offset gene (`wind_rot_deg`).** A free, per-member gene
  (`cella_lib::wildfire::driver::GENE_WIND_ROT_DEG`) added to the forcing's
  wind from-bearing before the driver writes it into the model, mod 360.
  Left out of a run's gene list (the default), it does nothing. Distinct
  from `SMC_WIND_ROT_DEG`, a fixed rotation of the *whole* weather
  schedule applied once for every member; this gene lets each member
  learn its *own* correction on top of that, uniform on [−h, h] where `h`
  is `SMC_WIND_ROT_GENE`'s half-width. E30b: tests whether the filter can
  learn its way out of a wrong ERA5 daily direction (E41) rather than
  needing the input fixed by hand.
- **Arm B (the E30b/E44 configuration).** The bundle of five knobs E30b's
  pilot found beat or tied E33 on every fire (`r7_common.ARM_B` in
  Round 7's code): `SMC_SPREAD=arrival` (the arrival-time spread rule,
  above), `SMC_WIND_LAW=rear_focus` (the rear-focus wind law, above),
  `SMC_STEPS_SCALE=4` (the 4× clock, raising the front-speed cap to
  6 km/day — see Clock cap, below), `SMC_PRIOR=arrival_x4.json` (a `p0`
  prior re-derived for that faster clock, log-uniform 0.02–0.6), and
  `SMC_WIND_ROT_GENE=90` (the wind-direction offset gene, above, at its
  ±90° half-width). "Arm A" is the same four non-gene knobs without the
  last one — E30b tested both, to isolate what the clock/prior fix alone
  bought from what the learned gene added on top. E44 re-ran Arm B at
  five seeds plus an E37b illumination re-run; the stop rule tripped on
  Pier, so Arm B is not promoted as of E44.
- **SMC_DIAG (per-window diagnostics).** Opt-in knob; off (default) adds
  nothing to the report. On, it adds one extra field, `diag`, to every
  scored window's report row in `open`/`assim`/`evolve` mode (all three
  share the same scoring loop, so all three carry it) — the ERA5 and
  station wind vectors for that window, the ensemble's per-window
  learned-gene medians, and the **head/flank miss decomposition** above.
  Nothing else in the report changes: a report from a run with
  `SMC_DIAG` unset is byte-identical to one from before this knob
  existed. E48.
- **Spotting.** Embers igniting cells far ahead of the front. Supported,
  off after E7; switched back on, as illumination genes only, in E43.
- **Spotting genes.** `model.spotting.p_spot` (per-step chance a burning
  cell throws a firebrand, log-uniform 0.001–0.005, E7's pre-registered
  range) and `model.spotting.median_distance` (typical landing distance
  in cells, linear 2–20). `SMC_SPOT=1` in `wildfire_smc` adds both to the
  gene list and switches spotting on in the config (it is otherwise
  disabled, so a `spotting.*` knob has nothing to write into). E43.
- **Tick / steps per day.** One model step. The scenarios declare 50
  ticks per day (28.8 minutes each). Fire can move one cell (30 m) per
  tick, so 1.5 km/day is the hard front-speed cap.
- **Front-speed cap.** The 1.5 km/day limit above. Real fires broke it on
  most of their big days.
- **Clock cap.** The front-speed cap, restated as a knob: at `N` ticks per
  day the head can move at most `N` cells/day (one cell/tick, both spread
  rules), so raising `N` — `SMC_STEPS_SCALE` in `wildfire_smc` — raises
  the cap without changing any spread gene. E30's 50 ticks/day capped the
  head at 1.5 km/day, below what Brattain's rear-focus head needed to
  reach its observed day-5 shape (≈ 1.9 cells/tick); E30b's
  `SMC_STEPS_SCALE=4` (200 ticks/day) raises the cap to 6 km/day, keeping
  one observation window at one day of forcing by scaling the driver's
  `steps_per_day` from the same field.
- **Percolation cliff.** Below a threshold p0 the fire fizzles; above it,
  it burns everything reachable. Real fires sit in the narrow band
  between. A small p0 change swings the burned area many-fold.
- **Recipe.** A named set of knob values. **E1 recipe**: each fire's best
  (p0, dur) from E1. **Global recipe**: one setting for all fires (Round 1:
  p0 0.22, dur 5, temperature schedule). **E17 recipe**: p0 ×4 E1, station
  wind, moisture damping, decay τ 5. **E20 transfer recipe**: p0 0.45,
  dur 15, τ 3.4 d, wind ×0.29.
- **p0 schedule.** p0 changed over time by a multiplier per wind window
  (daily) or per hour: temperature, rain, moisture, decay, containment.
- **Decay (τ).** p0 × exp(−t/τ), with τ in days. Makes the fire lose
  strength steadily and stop for good. Labelled a suppression proxy in
  E16, relabelled "early-days growth-rate decline" in E21, replaced by
  the containment operator in E28.
- **Moisture damping (η, M_x).** Rothermel's factor that lowers p0 when
  fuel moisture (from hourly humidity and temperature) approaches the
  moisture of extinction M_x. Slows the fire; cannot stop it.
- **Containment operator (a, b).** Once a day each ensemble member is
  "contained" with probability sigmoid(a + b × ln growth): slow-growing
  members get caught, fast ones do not. Contained members set p0 to 0.
  From FSim (Finney 2011). Learned by the filter like any other knob.
- **Growth floor (`contain_growth_floor`).** The floor the containment
  operator clamps a period's growth ratio to before taking its `ln`, so a
  flat or shrinking member never sends `ln growth` to `-∞`. Always
  `1e-4` before E49 (`SMC_CONTAIN_GROWTH_FLOOR` unset/default reproduces
  that byte-identically); E49 swept it on the four calibration fires as
  the operator's one genuine fixed-value "threshold" — `GENE_CONTAIN_A`/
  `GENE_CONTAIN_B` are free genes the filter fits per member, not a
  single value a sweep could hold constant.
- **Lock-in.** When every member of an ensemble is contained while the
  real fire still grows. Nothing can burn again, so the score freezes.
  Found in E33, repaired by `immigrant_reset` (E38).
- **Fire-line agent.** A rule that paints cells unburnable (or hard to
  burn) along the fire's edge during the run, standing in for crews.
  Three versions (E18, E23, E27), all either strangled or ignored.
- **Terrain wind field.** Per-cell wind from a mass-conserving downscaler
  (ridges speed up, valleys channel). `cella_lib::wildfire::wind_field`, E26.
- **Heterogeneity.** Random per-cell patchiness of flammability (E16a).

## Ensembles and learning

- **Ensemble / member / M.** Run the model M times (M = 32 by default),
  each member with its own knob values drawn from the prior and its own
  seed. The output is one map per cell: the share of members that burned
  it, a burn-probability map.
- **Prior.** The range each knob is drawn from before any data is seen.
  E25's broad prior: p0 0.08–0.6 (log), duration 5–20, wind × 0–1.5.
  "Narrow" and "very broad" variants in E35.
- **Posterior.** Where the knobs end up after learning. "Learned p0"
  means the population's median p0 at the end of a run.
- **Posterior trajectory.** The posterior tracked day by day instead of
  only at the end — the population's median (and cross-seed sd) of a
  knob at each observation, from day 1 through the final day. E42.
- **IQR (interquartile range) / IQR trend.** A gene's spread across the
  ensemble's members at one window: the middle value minus the bottom
  quarter's value (Q3 − Q1 of the members' values, linear-interpolation
  quantiles). Unlike sd, one outlier member cannot pull it far. Read
  window over window as the "IQR trend": if a gene is *learning* one
  correct value, its IQR should narrow as the filter converges; if a
  gene is carrying per-member *diversity* the filter keeps (see Angular
  diversity, below), its IQR should stay wide. `wind_rot_deg_iqr`,
  `SMC_DIAG=1` (per-window diagnostics, above), E45.
- **Hazard.** One day's own probability of something happening — here,
  the chance *that specific day* rolls a member "contained" — as
  opposed to the running total, see Cumulative probability. E42.
- **Cumulative probability (of containment, here).** The running total
  built from every day's hazard so far: the chance of having been
  contained *by* day d, the way "chance you've flipped heads at least
  once by the fifth flip" is built from five coin flips, not just the
  fifth one. `1 − Π(1 − hazard_i)` over the days so far, since
  containment only goes one way (once contained, always contained).
  E42.
- **Genome / gene.** A member's full set of knob values / one knob. GA
  vocabulary, used because the learning operators come from genetic
  algorithms.
- **Angular diversity.** A spread of *different* values a gene holds
  across the ensemble's members at once, as opposed to the whole
  population converging on one learned value. E30b's `wind_rot_deg`
  gene is the test case: each member draws its own rotation and, left
  to mutate normally, different members can keep different rotations
  through the run — so the ensemble as a whole is not betting on one
  corrected wind bearing but covering a *range* of bearings, which helps
  when the true direction itself varies day to day (sub-daily, or
  window to window) in a way one fixed correction cannot. `SMC_WIND_ROT_
  SIGMA=0` (E45) removes the *learning* half of the gene (no mutation
  after birth) while keeping diversity, isolating whether diversity
  alone — not a correct learned angle — is doing the work.
- **Open mode.** Members run independently start to finish. Pure Monte
  Carlo. E24.
- **Assim mode (particle filter).** At each observation day, score every
  member against the mask, keep the good ones (resample), nudge their
  knobs (mutate), add a few fresh draws (immigrants), keep simulating from
  where each member is. E25 onward. Also called "the filter" and
  "learning as it burns".
- **One-window-ahead forecast.** Every score in assim mode is made on
  day k using only what was learned up to day k−1. The mask for day k is
  used only after it has been scored. This is what makes the filter's
  numbers honest.
- **β (beta).** Selection sharpness. Weights are exp(β × IoU). β 10 is
  gentle, β 30 lets a few members take over.
- **Resampling.** Drawing the next population with probability
  proportional to weight. Children keep the parent's grid; you cannot
  redraw the past.
- **σ (sigma).** Mutation size, as a share of each knob's range.
- **Per-gene sigma.** A `sigma` set on one gene's own spec instead of the
  engine's shared default — it overrides the engine's σ for that gene
  only, every other gene still mutates at the engine's σ. `sigma: 0`
  freezes that one gene: each member keeps exactly the value it drew at
  birth for the rest of the run (resampling still copies it, so
  selection still acts on it — only mutation stops). `SMC_WIND_ROT_
  SIGMA` (E45) is this mechanism applied to `wind_rot_deg`, isolating
  angular diversity from learning (see Angular diversity, below).
- **Immigrants.** Share of children (20 % by default) that get fresh
  knobs from the prior instead of a parent's. Keeps the crowd from
  becoming clones. They inherit their parent's *state* (grid, contained
  flag) unless `immigrant_reset` is on.
- **immigrant_reset.** Option (E38): immigrants start with a fresh driver
  state, uncontained. Repairs lock-in. Default off.
- **Area-ratio gate.** Option (E39): `immigrant_reset_gate`. Only resets
  an immigrant while the population's own Area ratio (see Scores) at the
  last learning step is below the gate value (1.0 pre-registered) —
  under-predicting the observed area, the lock-in signature — instead of
  resetting every time. `None` (the default) leaves the plain
  `immigrant_reset` in charge.
- **State correction (`state_correction`).** The standard particle-filter
  move (Rochoux et al. 2014; Xue, Gu & Hu 2012) of rebuilding a member's
  whole grid — not just its knobs — from the observation itself, instead
  of only carrying a parent's history forward. Option: `None` (default,
  the pre-E40 behaviour — every child's grid is a clone of a resampled
  parent), `Immigrants` (E40, applies it to the immigrants only) or `All`
  (E40b, post-hoc, applies it to every resampled child, which keeps its
  own learned/mutated genome, so learning continues while the grid is
  corrected every window). A corrected child's grid is rebuilt from the
  observed mask just scored (burned interior → burned; the rim of
  still-unburned fuel next to a burned cell → burning, age 0; everything
  else untouched) and it always gets a fresh, uncontained driver state —
  `immigrant_reset` and the area-ratio gate are not consulted for it.
  Renamed from `immigrant_source: Prior | Observed` when `All` was added;
  `Observed` is what `Immigrants` used to be called.
- **Lagged persistence.** Null: the observed mask at t_{k−1}, unchanged,
  scored as the forecast for t_k (as opposed to plain persistence, which
  freezes the *ignition* mask forever). The fair dummy competitor for any
  state-corrected mode, since both see exactly the same thing — the mask
  one window back — and no more. On nested (monotonically growing) masks
  its IoU is exactly |A_{k−1}| / |A_k|. `None` at the very first scored
  window (there is no earlier *observed* mask to lag from yet). E40.
- **Lagged Circle.** Null: the Circle's own construction (chamfer growth,
  area-matched) but re-seeded from the observed mask at t_{k−1} instead
  of the fixed ignition mask, every window — "if you already knew
  yesterday's exact perimeter, grow it the Circle's way to today's true
  area." The other fair dummy competitor for a state-corrected mode. E40.
- **Crossover.** A child takes each knob from one of two parents.
  Standard in genetic algorithms; added to the filter as an option in
  E34.
- **Effective sample size (ESS).** How many members really carried
  weight after a learning step. Near M is healthy; near 1 is collapse.
  Ours stays at 28–31 of 32.
- **Offline fit / GA / evolution.** A genetic algorithm that searches the
  knobs to maximise a score over some observed days, then stops. E20
  (whole series, differential evolution), E36 (first three days only,
  library `Evolution`).
- **MAP-Elites / illumination.** A search that fills a grid of behaviour
  bins with the best genome found for each bin, instead of finding one
  best. With no objective it maps what the model can do at all. E37.
- **Descriptors (growth, elongation).** The two behaviour axes of E37's
  map: growth = burned share of the grid after five days; elongation =
  how stretched the burned shape is.
- **Coverage.** Share of the map's bins that any genome reached.
- **Reachable.** A size-and-shape the model can produce with some knob
  setting. If the real fire's size-and-shape is not reachable, no tuning
  will find it.

## Shape

- **Elongation.** √(λ₁/λ₂) of the burned set's second-moment matrix.
  1.0 = a disc, 2.7 = nearly three times longer than wide. E12, E37.
  Computed over *every* tracked cell with no notion of connectivity, so a
  round core plus a few cells scattered far away reads as elongated the
  same as a genuinely stretched single blob would (E43).
- **Largest-component elongation.** Elongation (same formula), computed
  from the largest 8-connected component of the tracked set alone,
  instead of every tracked cell together. Reported next to the largest
  component's own share of the total (`largest_fraction`; 1.0 = one
  connected piece) so "one stretched shape" can be told from "a round
  core plus scattered outliers." `cella_lib::explore::metrics::
  largest_component_stats`. E43 fix round 1.
- **Growth direction.** Direction from the previous day's burned set to
  the centroid of the new burn. Compared with the wind direction to ask
  whether the fire followed the wind.
- **Ignition centroid.** The mean (x, y) position of the cells burning at
  day 0 — one fixed point for the whole run, computed once from the
  ignition mask, not recomputed window by window. Used as the reference
  point the **head/flank miss decomposition** measures "downwind" from
  (a cell counts as downwind if going from the centroid toward that cell
  is also going the way the window's wind blows), so a cell's
  downwind/cross-wind label doesn't drift as the fire itself moves.
  `SMC_DIAG=1`. E48.
- **Wedge.** E37's finding: the reachable region is small-and-any-shape
  or big-and-round. Big and elongated is unreachable.
- **LB(U), length-to-breadth ratio.** Anderson (1983)'s empirical fire
  ellipse aspect ratio as a function of 10 m wind speed `U`: `LB =
  0.936·e^0.2566U + 0.461·e^-0.1548U − 0.397`, clamped to `[1, 8]`
  (`LB(0) = 1` exactly — a circle at no wind). Used by the Ellipse null
  (E41, TEST_PLAN v1.8) and, independently, by the fire model's own
  `rear_focus` wind law (E30a): `cella_lib::wildfire::anderson_lb`.
