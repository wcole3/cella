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
  0.1–0.4.
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
- **Wind multiplier (wind ×).** Scales the input wind speed before the
  kernel. ×0 = wind off.
- **Wind from-bearing.** Where the wind comes from, 0° = north, clockwise.
  The weather-report convention, used everywhere since Round 2.
- **Spotting.** Embers igniting cells far ahead of the front. Supported,
  off after E7.
- **Tick / steps per day.** One model step. The scenarios declare 50
  ticks per day (28.8 minutes each). Fire can move one cell (30 m) per
  tick, so 1.5 km/day is the hard front-speed cap.
- **Front-speed cap.** The 1.5 km/day limit above. Real fires broke it on
  most of their big days.
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
- **State correction.** The standard particle-filter move (Rochoux et al.
  2014; Xue, Gu & Hu 2012) of rebuilding a member's whole state — not
  just its knobs — from the observation itself, instead of only carrying
  a parent's history forward. E40's `immigrant_source: Observed` is this,
  applied to immigrants only.
- **Immigrant source.** Option (E40): `immigrant_source`, `Prior`
  (default) or `Observed`. `Prior` is the pre-E40 behaviour — an
  immigrant's grid is a clone of a resampled parent, like any other
  child. `Observed` rebuilds an immigrant's grid from the observed mask
  just scored (burned interior → burned; the rim of still-unburned fuel
  next to a burned cell → burning, age 0; everything else untouched) and
  always gives it a fresh, uncontained driver state — `immigrant_reset`
  and the area-ratio gate are not consulted for these immigrants, only
  for `Prior` ones.
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
- **Growth direction.** Direction from the previous day's burned set to
  the centroid of the new burn. Compared with the wind direction to ask
  whether the fire followed the wind.
- **Wedge.** E37's finding: the reachable region is small-and-any-shape
  or big-and-round. Big and elongated is unreachable.
