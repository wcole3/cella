# E44 — five-seed E30b Arm B and E37b at the 4× clock (the promotion test) · RESULTS PENDING

_Round 7 (2026-09-23) · two batches: forecast — `ARM_B` preset, seeds 0–4,
six fires, `assim` mode, 30 runs at 2 workers; map — `ARM_B` preset, six
fires, `map` mode (MAP-Elites illumination, 960 evaluations/fire:
`SMC_GENERATIONS=30` × `SMC_POP=32`, identical to E37/E37b), 6 runs, one
batch after the forecast batch · env knobs (`r7_common.ARM_B`)
`SMC_SPREAD=arrival SMC_WIND_LAW=rear_focus SMC_STEPS_SCALE=4
SMC_PRIOR=priors/arrival_x4.json SMC_WIND_ROT_GENE=90` · runner
`exp_r7_e44.py` → `exp44_arm_b_5seed.json` (+ raw
`exp44_arm_b_5seed/`, forecast), `exp44_e37b_4x_illuminate.json` (+ raw
`exp44_e37b_4x_illuminate/`, map) · compared against `exp33_noise.json`
(E33 five-seed baseline, `r7_common.e33_baseline()`) for the forecast
table and `exp37_illuminate.json` (E37) / `exp30_arrival_illuminate.json`
(E37b at the 1× clock) for the map table, none re-run · pre-registered
TEST_PLAN v1.9, §9 · `binary_git` [PENDING — filled in Result 1/2] ·
load(1 min) and wall time [PENDING — filled in Result 1/2, shared box,
do not compare across batches] · terms:
[GLOSSARY.md](GLOSSARY.md)_

**In short.** _[Placeholder — this section is written before either
batch has run. It is filled in after the forecast batch (Result 1,
Phase 2 of this task) and again after the map batch (Result 2, Phase 3),
with the prediction below checked clause by clause and the stop rule's
outcome stated plainly.]_

**Question.** Does the full, multi-seed Arm B configuration (arrival
kernel, rear-focus wind law, 4× clock, the widened `arrival_x4` prior, a
learned per-member `wind_rot_deg` gene at ±90°) beat E33, and does it
reach shapes E37/E37b's illumination said were previously unreachable
(Brattain, Ferguson and Pier all sit outside the reachable growth ×
elongation wedge under both the base model, E37, and the flat 1× arrival
kernel, E37b — `39-e37-illuminate-the-fire-model.md`,
`47-e30-arrival-time-kernel-fires.md`)?

**What we changed.** Nothing in the model or the driver — this
experiment is the full-scale re-run of a configuration E30b already
validated as a one-seed pilot
(`48-e30b-uncapped-clock-direction-gene-pilot.md`). Two batches, run one
at a time (shared-machine rule):

1. **Forecast** — `ARM_B` preset (`r7_common.ARM_B`), seeds 0–4, all six
   fires, `assim` mode, 32 members each, 30 runs total at 2 workers
   (`r7_common.run_all`, `nice -n 10` on every child). Output
   `exp44_arm_b_5seed.json`.
2. **E37b at the 4× clock** — the same `ARM_B` preset, `map` mode
   (MAP-Elites illumination: batch 32, 30 generations = 960 evaluations
   per fire, 5 days of the scenario's own weather, growth × elongation
   axes, no objective, no stopping rule — identical settings to E37 and
   the 1× E37b), all six fires, one batch, launched only after the
   forecast batch finishes. Output `exp44_e37b_4x_illuminate.json`.

**Why we expected it to matter.** The E30b pilot (one seed, two arms)
found Arm B beats or ties E33 on all six fires — four of six beyond the
E33 sd, every move a gain, zero losses beyond 1 sd anywhere — and
explicitly recommended running the full five-seed E30b, with E37b, on
Arm B's configuration; that recommendation is what this experiment
executes. Two things the pilot could not answer on its own: (1) it used
E33's sd, measured at a *different* configuration (1× clock, the old
prior, no rotation gene), as its tie bar, and flagged this in its own
caveat as a stand-in, not a like-for-like noise floor — a five-seed run
at Arm B's own configuration is needed to measure Arm B's own sd, which
becomes the noise floor for every later arrival-kernel arm (E45, E46,
E47); (2) the pilot did not re-run E37b, so it is still an open question
whether raising the clock from 1× (E30/E30a's clock, which E37b showed
*shrinks* the reachable wedge on every fire and makes Brattain and
Ferguson unreachable even in *size*, not just shape —
`47-e30-arrival-time-kernel-fires.md`, Result 1) to 4× (Arm B's own
clock, which the pilot showed recovers the forecast) also widens the
wedge enough to bring Brattain, Ferguson and Pier inside it.

**How we scored it.** Forecast: per fire, mean and sd of one-window-ahead
consensus IoU across the five seeds (`r7_common.fire_stats`), compared
against `exp33_noise.json`'s own five-seed mean and sd
(`r7_common.e33_baseline()`) with a delta-in-sd verdict column worded
exactly as `48-e30b-...md`'s tables ("tie" = within 1 sd, "beyond 1 sd
(gain|loss)", "**beyond 2 sd (gain|loss)**" =the prediction's own
stronger bar — `r7_common.verdict`/`summary_table`); Brier (ensemble);
Circle (`mean_radial_iou`) and Ellipse (`ellipse_iou`, averaged over each
raw report's score series, same method `47-e30-...md` used); the
persistence, Circle, Ellipse and lagged (`lagged_persistence_iou`,
`lagged_circle_iou`) nulls already carried in every `assim` report; final
contained fraction; and the five-seed median and spread (min–max across
the five seeds' own final ensemble medians) of the learned genes
(`model.p0`, `wind_scale`, and Arm B's own `wind_rot_deg`). Arm B's own
five-seed sd per fire — the deliverable every later arrival-kernel
experiment is judged against — is reported in its own clearly labelled
row/column next to the E33 comparison, and is also written into
`exp44_arm_b_5seed_summary.json`'s new `arm_b_sd` block
(`{fire: {"mean", "sd", "n"}}`, added by `exp_r7_e44.py` right after the
forecast `run_all()` returns, off the same rows the write-up's own table
is built from — `r7_common.fire_stats`). Map: coverage of the 400-bin
map (20 growth bins × 20 elongation bins, cells filled and %), the
model's maximum elongation at any size and at a size at least as big as
the real fire's own day-5 growth (elite filter `descriptor[0] >=
observed_growth_day5`, the same filter `wildfire_smc replay`'s elite
selection uses — an em-dash means no elite reached that growth at all,
nothing to take a maximum of), and the observed fire's own day-5 growth
and elongation — read directly from each fire's archive
(`archive.stats.elites`/`.coverage`, `archive.elites[*].descriptor`,
`observed[-1]`), the identical method `39-e37-...md` and
`47-e30-...md` used, laid out next to E37's and the 1× E37b's own
published numbers so the wedge's movement (1× → 4×) is visible in one
table. (`r5_common.run()`, which every other Round 7 job goes through,
assumes an `assim`-shaped report and cannot parse a `map`-mode
`MapReport` at all — see "Runner fix" below; `r7_common.run()` now
branches on `mode` and parses the archive itself for `map` jobs, the
same fields the paragraph above lists.)

**Prediction, written before the run (TEST_PLAN v1.9, §9, quoted
verbatim).** "Five-seed mean beats E33 beyond 2 sd on Brattain, Chimney,
Ferguson; ties Bear and Pier; Buck within its own sd." **Stop rule
(verbatim):** "if the five-seed mean loses to E33 beyond 1 sd on any
fire, Arm B is not promoted — E45 and E46 still run (they explain the
pilot regardless of whether it is promoted), E47 does not run." There is
no pre-registered numeric prediction for the E37b-at-4× wedge coverage
— TEST_PLAN v1.9 only requires reporting reachable-wedge coverage per
fire next to E37/E37b and stating whether Brattain, Ferguson and Pier
now sit inside the wedge; that reporting requirement, not a scored
prediction, is what Result 2 below answers.

**Runner fix (found while preparing this batch).** `r7_common.run()`
(Task 2) forwarded every job, `map` mode included, to
`r5_common.run()`, which unconditionally parses an `assim`-shaped report
(`r["scores"][-1]`, `r["mean_consensus_iou"]`, ...) — a `map`-mode
`MapReport` (`cella_lib/examples/wildfire_smc/modes/map.rs`) has no
`scores` key at all, so the map batch would have crashed with a
`KeyError` on its first job. Confirmed with a tiny real `map`-mode run
(`SMC_MAP_DAYS=1 SMC_GENERATIONS=2 SMC_POP=8` on Bear) before this was
fixed. `r7_common.run()` now branches on `mode == "map"` to a new
`_run_map()` that parses the archive the same way every Round 6
illuminate script did by hand (`exp_r6_arrival_illuminate.py`,
`exp_r6_spot_illuminate.py`); `assim`/`nulls`/other modes are unchanged
(still routed through `r5_common.run()`). No Rust changes; `py_compile`
and a live sanity run both pass; `make clippy` and `cargo test` (both
roots) pass (unaffected — Python-only change).
