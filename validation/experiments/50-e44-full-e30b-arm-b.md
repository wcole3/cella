# E44 — five-seed E30b Arm B and E37b at the 4× clock (the promotion test) · REJECTED as tested — stop rule trips on Pier; E37b-at-4× recovers most of the arrival kernel's lost coverage and Ferguson barely enters the wedge, but Brattain and Pier stay outside it

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
TEST_PLAN v1.9, §9 · forecast batch: `binary_git b60c032` (clean HEAD,
verified in all 30 summary rows and all 30 raw reports), load(1 min)
3.16 at launch → 3.44 at finish, wall time 21466.4 s ≈ 5.96 h at 2
workers · map batch: `binary_git f5da768` (clean HEAD, verified in all 6
summary rows and all 6 raw reports), load(1 min) 0.65 at launch → 5.73
at finish, wall time 51313.0 s ≈ 14.25 h at 2 workers (**both wall times:
shared box, 2 workers, do not compare against each other or against any
other batch's**) · terms:
[GLOSSARY.md](GLOSSARY.md)_

**In short.** Both halves of the promotion test are in, and neither is a
clean pass. Forecast: four of six fires do exactly what the prediction
said — Brattain, Chimney and Ferguson beat E33 beyond 2 sd (all three by
more than 4.5 sd), Bear ties — but Pier, predicted to tie, instead loses
to E33 by 4.60 sd (E33's own sd only 0.003), and Buck, predicted to land
"within its own sd," lands at +1.27 sd, a real gain but not a tie. The
pre-registered stop rule ("loses to E33 beyond 1 sd on any fire") trips
on Pier, more than four times over — **Arm B is not promoted as
tested**; E45 and E46 still run, E47 does not. Illumination: the 4×
clock recovers most of the coverage the arrival kernel lost at 1× (on
three of six fires the 4× archive fills *more* cells than E37's own base
model; a fourth, Bear, lands just under it), and Brattain and Ferguson
stop being "unreachable in size" the
way they were at 1×. But of the three fires E37/E37b's wedge excluded,
only **Ferguson** now sits inside it, and barely (its model reaches an
elongation of 1.62 against an observed 1.61, a 0.01 margin at the exact
size threshold). Brattain and Pier remain outside — Brattain can now
reach the *size* but not the *shape* (max elongation at that size 1.34
against an observed 1.83); Pier's reach barely moved at all. Arm B's own
five-seed sd, the new noise floor for later experiments, is also
markedly larger than E33's on the two fires with the widest per-seed
disagreement in the learned `wind_rot_deg` gene — Brattain (8.86×) and
Pier (7.45×) — with Chimney elevated too but far more modestly (2.63×).

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

## Result 1 — five-seed forecast (Arm B vs E33)

Provenance: all 30 summary rows (`exp44_arm_b_5seed.json`) and all 30
raw reports (`exp44_arm_b_5seed/*.json`) carry `binary_git b60c032`,
matching the clean HEAD this batch was launched from — no report from a
dirty or stale build. Batch: 30 jobs (5 seeds × 6 fires), 2 workers,
load(1 min) 3.16 → 3.44, wall time 21466.4 s (≈ 5.96 h — the
pre-registered design estimated ≈ 10 h; actual was faster, **shared
box, not a claim about anything but this run**).

Mean one-window-ahead consensus IoU, five seeds, against the E33
five-seed baseline (`exp33_noise.json`), with Arm B's own five-seed sd
reported in its own column — this is the noise floor every later
arrival-kernel experiment (E45, E46, E47) is judged against from here
on, also written into `exp44_arm_b_5seed_summary.json`'s `arm_b_sd`
block:

| Fire | Baseline mean | Baseline sd | Arm B (5-seed) mean | Arm B (5-seed) sd | Delta Arm B (5-seed) (sd) | verdict Arm B (5-seed) |
|---|---|---|---|---|---|---|
| Bear | 0.479 | 0.015 | 0.473 | 0.005 | −0.006 (−0.42 sd) | tie |
| Brattain | 0.416 | 0.004 | 0.437 | 0.034 | +0.022 (+5.73 sd) | **beyond 2 sd (gain)** |
| Buck | 0.590 | 0.039 | 0.640 | 0.005 | +0.049 (+1.27 sd) | beyond 1 sd (gain) |
| Chimney | 0.434 | 0.012 | 0.489 | 0.031 | +0.055 (+4.70 sd) | **beyond 2 sd (gain)** |
| Ferguson* | 0.344 | 0.007 | 0.386 | 0.014 | +0.042 (+5.69 sd) | **beyond 2 sd (gain)** |
| Pier* | 0.535 | 0.003 | 0.521 | 0.023 | −0.014 (−4.60 sd) | **beyond 2 sd (loss)** |

"Baseline sd" is E33's own five-seed sd (the pre-registered tie bar for
*this* experiment — TEST_PLAN v1.9's E44 entry names it explicitly as
the noise floor for E44 itself; later experiments switch to Arm B's own
sd, above). `*` = holdout pair. Table generated directly by
`r7_common.summary_table()` off `r7_common.fire_stats()` on the raw
rows — the verdict wording ("tie" / "beyond 1 sd" / "**beyond 2 sd**")
is `r7_common.verdict()`'s, unedited. Pier's own five seeds, plainly:
0.534, 0.524, 0.526, 0.538, 0.481 (seeds 0–4) — four of the five sit
close together (0.524–0.538); seed 4 (0.481) pulls the mean down and
alone accounts for most of the loss, but even without it the other four
average 0.5305, still 1.42 E33-sd below E33's 0.535 — beyond the stop
rule's own 1 sd bar on its own, just not beyond 2 sd. This is not one
bad seed rescuing an otherwise-tied result.

Brier, the four nulls (persistence, Circle, Ellipse, lagged), and final
contained fraction. Circle, Ellipse, plain persistence and both lagged
nulls are **identical to four decimals across all five Arm B seeds on
every fire** (confirmed directly from the raw reports, not assumed) —
they depend only on the truth mask, the scenario's ERA5 wind and the
observation cadence, none of which vary with the ensemble's RNG seed,
the same finding `48-e30b-...md` made for Circle/Ellipse at the pilot's
single seed. Contained fraction is the one field here that *does* vary
by seed:

| Fire | Brier E33 (5-seed mean) | Brier Arm B (5-seed mean) | Circle | Ellipse | Persistence | Lagged persistence | Lagged Circle | Contained, 5 seeds (min–max) |
|---|---|---|---|---|---|---|---|---|
| Bear | 0.0510 | 0.0507 | 0.541 | 0.513 | 0.091 | 0.902 | 0.912 | 0.969–1.000 |
| Brattain | 0.1090 | 0.1020 | 0.450 | 0.469 | 0.017 | 0.889 | 0.900 | 1.000–1.000 |
| Buck | 0.0468 | 0.0396 | 0.670 | 0.701 | 0.205 | 0.957 | 0.944 | 1.000–1.000 |
| Chimney | 0.1276 | 0.0947 | 0.372 | 0.247 | 0.119 | 0.882 | 0.867 | 0.531–1.000 |
| Ferguson* | 0.1380 | 0.1356 | 0.373 | 0.503 | 0.007 | 0.919 | 0.915 | 0.844–1.000 |
| Pier* | 0.1093 | 0.1074 | 0.559 | 0.566 | 0.199 | 0.953 | 0.946 | 1.000–1.000 |

Brier (lower is better) improves on all six fires, Pier included
(0.1074 vs E33's 0.1093) — a reminder that Brier and consensus IoU do
not always move together; Pier's *ranking* of the ensemble's own
probability estimate is slightly better calibrated even though the
consensus mask's overlap with truth got worse. Both lagged nulls sit
far above every arm's own score on every fire (e.g. Bear: lagged
persistence 0.902, lagged Circle 0.912, vs Arm B's own 0.473) — expected
for a slow-growing nested mask, where "yesterday's exact mask" is
already most of today's answer; this is a from-ignition forecast, not a
state-corrected one, so the gap is not a fair one-to-one comparison, it
is context for how much a from-ignition forecast is giving up by not
using yesterday's true mask. Two fires do not fully contain every
member by the run's end in every seed: **Chimney** (three of five seeds
short of full containment — 53.1%, 59.4% and 84.4%; the other two reach
100%) and **Ferguson** (one seed at 84.4%, full in the other four) —
reported plainly, not smoothed into a mean.

Learned gene medians (five-seed median of each seed's own final-ensemble
median) and spread (min–max across the five seeds' own medians):

| Fire | `model.p0` med. (range) | `model.burn_duration` med. (range, hours) | `wind_scale` med. (range) | `wind_rot_deg` med. (range, °) |
|---|---|---|---|---|
| Bear | 0.086 (0.068–0.160) | 55.0 (40.5–60.0) | 0.747 (0.614–1.040) | −7.6 (−35.8–+43.8) |
| Brattain | 0.384 (0.177–0.412) | 50.0 (46.0–53.5) | 0.671 (0.532–1.123) | −12.3 (−29.9–+46.2) |
| Buck | 0.060 (0.034–0.122) | 49.5 (41.5–54.5) | 0.937 (0.722–1.060) | −11.1 (−17.5–+46.2) |
| Chimney | 0.266 (0.181–0.297) | 49.0 (36.0–55.5) | 0.782 (0.685–1.028) | −29.3 (−47.6–−8.6) |
| Ferguson* | 0.160 (0.092–0.333) | 52.0 (43.5–60.5) | 0.821 (0.713–0.939) | +19.3 (−6.4–+42.2) |
| Pier* | 0.122 (0.053–0.241) | 49.5 (46.5–57.5) | 0.708 (0.294–1.033) | −18.8 (−43.1–−3.4) |

None of the thirty `p0` medians sit near either prior edge (0.02 or
0.6), matching the correction already on record in
`47-e30-...md`'s "Later" section. The `wind_rot_deg` spread is wide on
every fire — Pier alone ranges −43.1° to −3.4° across its five seeds,
all five negative but far from agreeing on a value — which reads the
same way `48-e30b-...md` read the pilot's single-seed medians: not "the
filter found and locked onto one correct bearing," but per-member
angular diversity doing the work, seed to seed as well as member to
member.

**Prediction checked clause by clause.** "Five-seed mean beats E33
beyond 2 sd on Brattain, Chimney, Ferguson" — **holds**, all three
(+5.73, +4.70, +5.69 sd). "Ties Bear" — **holds** (−0.42 sd). "Ties ...
Pier" — **fails**: Pier is −4.60 sd, `r7_common.verdict()` calls it
"**beyond 2 sd (loss)**," not a tie, by the same rule every other cell
in this table is read by. "Buck within its own sd" — **does not hold as
written**: Buck's delta is +1.27 sd, `verdict()` calls it "beyond 1 sd
(gain)" — a real, modest gain, not a tie; it clears the 1 sd tie band
that "within its own sd" would need to sit inside, though it comes
nowhere near the 2 sd bar the three winning fires clear.

**Stop rule, evaluated exactly as written.** "If the five-seed mean
loses to E33 beyond 1 sd on any fire, Arm B is not promoted." Pier's
five-seed mean loses to E33 by 4.60 sd — more than four times the 1 sd
bar, not a borderline call. **The stop rule has tripped.** Per
TEST_PLAN v1.9's own consequence text: E45 and E46 still run (they
explain the pilot's mechanism regardless of whether it is ultimately
promoted); E47 does not run. Arm B, as tested here, is **not
promoted**. The E37b-at-4× map batch is pre-registered independently of
this stop rule (TEST_PLAN v1.9 requires it regardless of the forecast
outcome) and is launched below; its own result is reported in Result 2.

## Result 2 — E37b at the 4× clock

Provenance: all 6 summary rows (`exp44_e37b_4x_illuminate.json`) and all
6 raw reports (`exp44_e37b_4x_illuminate/*.json`) carry `binary_git
f5da768`, matching the clean HEAD the map batch was launched from (a
different, later commit than the forecast batch's `b60c032` — the
Result 1 write-up in between moved `binary_git`, see the "Runner fix"
provenance note above). Batch: 6 jobs (one per fire), 2 workers,
load(1 min) 0.65 → 5.73, wall time 51313.0 s ≈ 14.25 h — **far past the
≈ 1–2 h the pre-registered design estimated**, the opposite direction of
Result 1's own surprise (the forecast batch ran *faster* than its
estimate). The 4× clock quadruples the tick count of every one of the
960 evaluations per fire the same way it does a forecast's ticks, and
illumination has no cheaper way to sample the gene space than running
all 960 to completion (shared box, 2 workers — this number is not
comparable to Result 1's wall time or to E37/E37b's own, which ran at
the 1× clock).

Coverage of the 400-bin map (20 growth bins × 20 elongation bins), the
model's maximum elongation at any size and at a size at least as big as
the real fire's own day-5 growth, next to E37 (base model,
`39-e37-illuminate-the-fire-model.md`) and E37b at the 1× clock
(`47-e30-arrival-time-kernel-fires.md`, Result 1) — none re-run, both
quoted from their own published tables:

| Fire | E37 cells (%) | E37 any | E37 at-size | E37b·1× cells (%) | E37b·1× any | E37b·1× at-size | E37b·4× cells (%) | E37b·4× any | E37b·4× at-size | observed g/e (day 5) | E37 reachable? | E37b·1× reachable? | **E37b·4× reachable?** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Bear | 125 (31%) | 3.19 | 2.03 | 49 (12%) | 2.08 | 1.40 | 116 (29%) | 2.43 | 1.86 | 0.047 / 1.98 | yes | no | **no** |
| Brattain | 112 (28%) | 5.52 | 1.35 | 32 (8%) | 4.21 | — | 125 (31%) | 4.02 | 1.34 | 0.110 / 1.83 | no | no — unreachable in size | **no** (reachable in size again, still wrong shape) |
| Buck | 60 (15%) | 1.95 | 1.49 | 29 (7%) | 1.61 | 1.36 | 47 (12%) | 1.96 | 1.49 | 0.058 / 1.50 | borderline | no | **borderline** |
| Chimney | 48 (12%) | 1.54 | 1.35 | 22 (6%) | 1.19 | 1.04 | 57 (14%) | 1.73 | 1.73 | 0.067 / 1.22 | yes | no | **yes** |
| Ferguson* | 52 (13%) | 2.06 | 1.38 | 12 (3%) | 1.61 | — | 68 (17%) | 1.91 | 1.62 | 0.080 / 1.61 | no | no — unreachable in size | **borderline — yes, by 0.01** |
| Pier* | 34 (9%) | 1.39 | 1.15 | 21 (5%) | 1.22 | 1.16 | 32 (8%) | 1.31 | 1.31 | 0.102 / 1.45 | no | no | **no** |

How to read it: "cells (%)" is coverage of the 400-bin map (elites /
400); the two elongation columns per run are the most stretched fire
the model made at any size, and at a size at least as big as the real
fire's own day-5 growth; an em-dash means no elite in the whole
960-evaluation search reached the observed growth at all — nothing to
take a maximum of. "Reachable?" follows the same rule
`39-e37-...md`/`47-e30-...md` used: yes if the at-size elongation meets
or beats the observed elongation, no if it falls short, "borderline"
when the two are within about 0.02 of each other (both Buck's E37
borderline call, 1.49 vs 1.50, and Ferguson's 4× call, 1.62 vs 1.61, are
0.01 apart). `*` = holdout pair.

**Coverage recovers past E37's own baseline on three of six fires, and
close to it on a fourth.** The 4× clock does not just partially undo the
collapse E37b's 1× illumination found — on Brattain, Chimney and
Ferguson the 4× archive fills *more* cells than E37's own base-model
archive did (Brattain 125 vs E37's 112; Chimney 57 vs 48; Ferguson 68 vs
52). Bear (116 vs E37's 125) lands just under it; Buck (47 vs 60) and
Pier (32 vs 34) recover much less of the gap. Brattain and Ferguson also
stop being **unreachable in size**: at 1× no elite reached their
observed day-5 growth at all (the em-dashes above); at 4× an elite does,
on both.

**But only Ferguson crosses into the wedge, and by the thinnest possible
margin.** Its at-size elongation ceiling (1.62) beats its observed
elongation (1.61) by 0.01 — the same margin E37's own "borderline" call
for Buck used, so this is read the same way: a real but fragile pass,
not a decisive one. **Brattain** recovers the *size* (no longer an
em-dash) but not the *shape*: its at-size elongation ceiling is 1.34,
barely changed from the 1× number, still less than half its observed
1.83. **Pier** barely moves at all (any-size max 1.31 at 4× vs 1.22 at
1×, at-size 1.31 vs 1.16) and stays well short of its observed 1.45.
Answering the brief's question directly: **of Brattain, Ferguson and
Pier, only Ferguson now sits inside the reachable wedge — and that pass
is a 0.01 margin at the exact size threshold, not a comfortable clear.**

There is no pre-registered numeric prediction for this half of the
experiment to check clause by clause (TEST_PLAN v1.9 only requires
reporting coverage next to E37/E37b and stating whether Brattain,
Ferguson and Pier now sit inside the wedge); that reporting requirement
is answered above.

**What it means.** The 4× clock was diagnosed, in
`47-e30-...md`'s corrected root-cause note, as fixing the clock cap that
made the *illumination* growth-limited (Brattain's day-5 shape needs
about 1.9 cells/tick, unreachable at 50 ticks/day even at the prior's
own edge) without necessarily fixing the forecast's own binding
constraint (more likely the direction input, per E41). This result is
consistent with exactly that split: the clock fix does what it was
diagnosed to do for illumination — coverage recovers, often past E37's
own baseline, and two fires stop being unreachable in size — but
recovering *reach* is not the same as recovering the *right shape at the
right size*, and only one of the three excluded fires (Ferguson) crosses
that second, harder bar, narrowly. Brattain's own case is the clearest
illustration: it has the *widest* elongation range of any fire in the
whole table (any-size max 4.02, nearly a third of the full 1–4 axis) —
the model can draw very stretched shapes somewhere in its gene space —
but not at the specific size Brattain's own fire reached by day 5. The
forecast side tells a related but not identical story: Brattain's
five-seed forecast is a strong win (+5.73 sd), so the filter can fit
Brattain well when it is allowed to search over multiple days and
correct with observations, even though a single 5-day, no-feedback
illumination run cannot find a matching elite at that exact size and
shape. Pier is the harder case for the "clock was the fix" story: its
forecast is the stop rule's own trigger (−4.60 sd) and its illumination
barely moved, so whatever is wrong with Pier looks less like a clock-cap
problem and more like the pilot's original, still-unresolved direction
question.

**Questions this raises.**

- Does Ferguson's 0.01-margin pass hold up under any noise at all — a
  second seed for the illumination search (MAP-Elites here used a single
  seed, unlike the forecast's five), or a slightly different gene
  discretisation? This experiment cannot tell a real pass from a
  coin-flip at that margin; a repeat run (different seed, mechanism
  otherwise unchanged) would settle it either way. Open.
- Is Arm B's own elevated forecast sd on Brattain (8.86× E33's) and Pier
  (7.45×) actually driven by the wide, sign-disagreeing per-seed spread
  in the learned `wind_rot_deg` gene reported in Result 1 (Brattain's
  five-seed medians range −29.9° to +46.2°, crossing zero; Pier's range
  −43.1° to −3.4°, all negative but far apart)? The correlation is
  suggestive — the two fires with the widest, most seed-disagreeing
  rotation medians are also the two with the largest sd inflation — but
  this experiment did not hold the gene fixed and re-run to test it
  directly, and Chimney (also a wide, one-sided range, −47.6° to −8.6°)
  has a far more modest sd ratio (2.63×), so the relationship is not
  clean. Open; E45's own design (Arm B-σ0, mutation sigma 0 on
  `wind_rot_deg`) is close to a direct test of this, though it was
  pre-registered for a different question (mechanism, not variance).
- Brattain's illumination can reach its observed *size* at 4× but not
  its *shape*, while its *forecast* wins by 5.73 sd — what does the
  filter's five-day, observation-corrected fit find that the single
  5-day blind illumination search does not? Open; a `replay` of
  Brattain's own forecast posterior genome against the illumination's
  behaviour axes (the same technique E43's fix used) would show directly
  whether the filter is finding an elite the search itself missed, or
  succeeding by a different route (e.g. leaning on days 1–4 more than
  day 5's exact shape).
- Pier's illumination barely moved between 1× and 4× (any-size 1.22 →
  1.31) while five of the other six fires moved substantially — is Pier
  specifically direction-limited (E41-style) rather than speed/clock-
  limited, consistent with its forecast being the stop rule's own
  trigger? Open, not tested directly here.
- The map batch took ≈ 14.25 h against a ≈ 1–2 h estimate — should the
  pre-registered design for any later map-mode batch (E47's
  illumination, if the stop rule permitted it; it does not run here) be
  re-estimated at roughly the 4× clock's real cost, not the 1× E37/E37b
  wall time the original estimate was implicitly built on? Open,
  practical.

**Verdict.** **REJECTED as tested — Arm B is not promoted.** The
pre-registered stop rule trips on Pier (five-seed mean −4.60 sd against
E33, more than four times the 1 sd bar, robust even excluding Pier's
single worst seed). What held: three of six fires beat E33 beyond 2 sd
exactly as predicted (Brattain, Chimney, Ferguson), Bear ties as
predicted, and the 4× clock does what `47-e30-...md`'s diagnosis said it
should for illumination — coverage recovers, on three of six fires past
E37's own base-model coverage, and two fires (Brattain, Ferguson) stop
being unreachable in size. What did not hold: Pier meets neither the
forecast prediction nor the illumination — its reach barely moved and
its forecast lost badly enough to trigger the stop rule; Buck's forecast
gain (+1.27 sd) is real but is not "within its own sd" as predicted; and
of the three fires the wedge excluded, only Ferguson now sits inside it,
by a 0.01 margin that this experiment cannot itself distinguish from
noise. Per TEST_PLAN v1.9's consequence text: E45 and E46 still run —
they explain the pilot's mechanism regardless of promotion, and E45's
`wind_rot_deg` ablation in particular bears directly on this file's own
open question about Brattain's and Pier's elevated forecast sd; E47 does
not run.

**Later.** Not yet revisited.
