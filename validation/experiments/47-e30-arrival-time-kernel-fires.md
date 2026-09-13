# E30 — the arrival-time kernel on the six real fires · REJECTED (as tested) — the wedge does not widen (Brattain and Ferguson cannot even reach their observed *size* in 5 days), and the forecast gets worse than E33 by more than the noise floor on 5 of 6 fires

_Round 6 (2026-09-12, after E30a) · E37b: MAP-Elites illumination, identical to E37 (960 evaluations/fire) · E30 forecast: 5 seeds x 6 fires, matched to E33 · all six fires incl. holdout · env knobs `SMC_SPREAD=arrival SMC_WIND_LAW=rear_focus` (`configure_spread` in `wildfire_smc.rs`) · runners `exp_r6_arrival_illuminate.py` → `exp30_arrival_illuminate.json` (+ raw `exp30_arrival_illuminate/`), `exp_r6_arrival_fires.py` → `exp30_arrival_fires.json` (+ raw `exp30_arrival_fires/`), optional replay `exp_r6_arrival_replay.py` → `exp30_arrival_replay.json` · compared against `exp37_illuminate.json` (E37, not re-run) and `exp33_noise.json` (E33 twins, not re-run), with `exp40_observed_immigrants.json` (E40) as a second, not-combined reference row · pre-registered TEST_PLAN v1.8 addendum · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** E30a validated the arrival-time spread rule with the
rear-focus wind law for length-to-breadth ratios up to 1.5 — comfortably
covering the six fires' real ERA5 winds (0.5–0.7 m/s) times the
ensemble's own `wind_scale` gene ceiling (×1.5, giving LB ≤ ~1.3). Task 8
asked whether that validated kernel actually helps the two places E37
found broken: the reachable-shape wedge (E37b) and the forecast score
itself (E30). It does not, and the reason is the same mechanical fact in
both halves: **`model.p0` changes meaning under the arrival rule** — from
a per-tick, per-neighbour *probability* (which saturates fast when
several neighbours are burning) to a *rate* in cells per tick that does
not saturate at all. The gene ranges searched (illumination) and drawn
from (forecast) were left numerically unchanged, per the task's own
instructions, so the same numbers now describe much slower fires. E37b's
archives fill 2–4× fewer cells than E37's under the identical
30-generation/batch-32 budget, and on **Brattain and Ferguson not one of
the 960 evaluations reaches even the observed day-5 growth** — there is
no "at observed size" ceiling to compare at all, only "unreachable in
size," which is a strictly worse verdict than E37's own "unreachable in
shape." The forecast tells the same story from the other side: the
particle filter does learn a somewhat higher p0 on 5 of 6 fires (it has
nowhere else to go — arrival has no other speed knob), but not enough to
close the gap, and mean forecast IoU falls by more than the E33 noise
floor on five of six fires (only Buck improves, and only within its own
large noise band). Chimney and Brattain — the two fires the pre-registered
prediction expected might improve — instead take the two largest losses
(−0.144, −0.111).

**Question.** With the arrival rule and E30a's recommended rear-focus
wind law, do Brattain, Ferguson and Pier fall inside the reachable
wedge, and does the forecast ensemble improve?

**What we changed.** `wildfire_smc` gains four independent env knobs via
a new `configure_spread` function (same downcast-to-`WildfireModel`
pattern as `SMC_SPOT`/`enable_spotting`): `SMC_SPREAD=bernoulli|arrival`,
`SMC_WIND_LAW=exponential|rear_focus`, `SMC_C2=<f64>`,
`SMC_ARRIVAL_JITTER=<f64>`. Any left unset keeps the scenario config's
own default; setting none of them is a complete no-op (checked by a unit
test that never even downcasts the config in that case). Two runs, both
pre-registered:

1. **E37b acceptance** — E37's illumination re-run exactly (`map` mode,
   batch 32, 30 generations, 5 days of the scenario's own weather, growth
   axis 0–0.10, elongation axis 1–4, no objective, no stopping rule),
   with `SMC_SPREAD=arrival SMC_WIND_LAW=rear_focus` added. The wind ×
   gene range is unchanged (0–1.5). `c2` is unchanged from its default
   (0.131) — `rear_focus` has no `c2` knob at all (`c2` only shapes the
   `exponential` law), so there is nothing to set there; `arrival_jitter`
   is likewise left at its default (0.2).
2. **E30 forecast** — five seeds (0–4) × six fires, `r5_common.BASE_ENV`
   (assim, β 10, σ 0.2, immigrants 0.2, containment-only stopping) plus
   `SMC_SPREAD=arrival SMC_WIND_LAW=rear_focus`. E39's area-ratio gate
   (Task 4 finding: inert, left off) and E40/E40b's state correction
   (Task 5/6 finding: a different score family, left at `none`) are both
   off — `BASE_ENV` sets neither — so the kernel change is isolated and
   every run is matched seed-for-seed to its `exp33_noise.json` twin.
   Spotting (E43) is a separate mechanism and is not combined with the
   kernel change here (see "Questions this raises").

**Why we expected it to matter.** E37 found the model's reachable
growth × elongation region is a wedge (small fires: any shape; large
fires: round) because Bernoulli's saturating per-tick probability erases
direction differences once enough neighbours are burning. E30a fixed
that mechanism directly — the arrival rule keeps direction information as
travel *time*, which never saturates — and validated the rear-focus wind
law's magnitude for LB ≤ 1.5, the regime these six fires actually sit in.
The natural next question is whether fixing the mechanism at the flat-grid
level actually moves the needle on real terrain, real fuel and real
weather.

**How we scored it.** E37b: coverage, the model's own maximum elongation
at any size, and its maximum elongation at a size at least as big as the
real fire on day 5 (elites with `descriptor[0] >= observed_growth_day5`,
the same filter `wildfire_smc replay`'s elite selection uses) — read
directly from each fire's archive, the identical method E43 used to
reproduce E37's own published numbers exactly (checked again here: this
file's own recomputation of E37's "at observed size" column from
`exp37_illuminate/<fire>.json` reproduces the published table digit for
digit). E30: per-seed change in mean one-window-ahead consensus IoU
against the E33 twin, five seeds; mean ± sd for E33/E30; Brier
(ensemble); Circle (`mean_radial_iou`) and Ellipse (`ellipse_iou`,
averaged over the score series — the `Report` struct has no top-level
mean field for it yet, so this file computes it directly from each raw
report's `scores`, the same way `mean_radial_iou` itself averages);
final contained fraction; and the learned p0 / wind × medians (of each
seed's own final ensemble mean) across the five seeds. The `assim`
report's lagged nulls (`lagged_persistence_iou`, `lagged_circle_iou`,
Task 5) are not reported in the headline table: they answer "how much
does seeing yesterday's mask help," which is not the question a
from-ignition forecast (no state correction here) is being asked; E33's
own raw reports predate the Ellipse null and the lagged nulls entirely
(no `binary_git` field even), so there is no E33-side Ellipse or
lagged-null number to put beside these anyway. `exp40_observed_immigrants.json`
(E40, Task 5's own headline number, not re-run) is carried as a second
reference row per fire, unmodified and not combined with the arrival
kernel, exactly as the task brief asked.

![Top: six MAP-Elites archives under the arrival/rear_focus kernel, E37's own wedge boundary overlaid as a dashed line, the observed fire's daily position as dots. Bottom: a dot strip per fire, one seed at a time, of the E30 forecast minus its E33 twin, against the E33 noise band.](figures/e30-arrival-fires.svg)

## Result 1 — E37b acceptance

| Fire | E37 cells filled | E37 any size | E37 at-size | E37b cells filled | E37b any size | E37b at-size | observed (day 5) growth / elong. | E37 reachable? | E37b reachable? (E37 → E37b) |
|---|---|---|---|---|---|---|---|---|---|
| Bear | 125 (31%) | 3.19 | 2.03 | 49 (12%) | 2.08 | 1.40 | 0.047 / 1.98 | yes | **no** (yes → no) |
| Brattain | 112 (28%) | 5.52 | 1.35 | 32 (8%) | 4.21 | — | 0.110 / 1.83 | no | **no — unreachable in size** (no → no, worse) |
| Buck | 60 (15%) | 1.95 | 1.49 | 29 (7%) | 1.61 | 1.36 | 0.058 / 1.50 | borderline | no (borderline → no) |
| Chimney | 48 (12%) | 1.54 | 1.35 | 22 (6%) | 1.19 | 1.04 | 0.067 / 1.22 | yes | **no** (yes → no) |
| Ferguson* | 52 (13%) | 2.06 | 1.38 | 12 (3%) | 1.61 | — | 0.080 / 1.61 | no | **no — unreachable in size** (no → no, worse) |
| Pier* | 34 (9%) | 1.39 | 1.15 | 21 (5%) | 1.22 | 1.16 | 0.102 / 1.45 | no | no (no → no) |

How to read it: "cells filled" is coverage of the 400-bin map; the two
elongation columns per run are the most stretched fire the model made at
any size, and at a size at least as big as the real fire on day 5; an
em-dash (—) under "E37b at-size" means **no elite in the entire
960-evaluation search reached the observed growth at all** — there is
nothing to take a maximum of. `*` is the holdout pair. Bold marks a fire
whose verdict got strictly worse under E37b.

- **Every fire's archive shrinks under the arrival kernel.** Coverage
  falls 2–4× on all six fires (e.g. Ferguson 52 → 12 elites, Bear
  125 → 49). The maximum growth *any* elite reached in the whole search
  also falls hard: Ferguson's best elite reaches only 2.0 % burned area
  in 5 days, versus an observed 8.0 % — the model's fastest possible
  setting, at these gene ranges, is still slower than the real fire.
  Brattain's best reaches 3.7 % against an observed 11.0 %.
- **Two fires move from "wrong shape" to "wrong size" — a strictly worse
  failure mode.** E37 could at least draw a fire as *big* as Brattain or
  Ferguson (just not as elongated); E37b cannot draw one that big at all.
  "Reachable?" is not merely "no" for these two, it is "not applicable":
  there is no candidate shape to judge.
- **Bear and Chimney, E37's two reachable fires, both flip to
  unreachable.** Bear's at-size ceiling falls from 2.03 to 1.40 (observed
  1.98); Chimney's falls from 1.35 to 1.04 (observed 1.22). Both moves are
  driven by the same growth shortfall as Brattain/Ferguson, just not
  severe enough to make the observed size completely unreachable — only
  severe enough to make the model's own fires round *before* they reach
  it.
- **The "any size" ceiling also falls on every fire**, including the ones
  E37 found most elongated purely from terrain/fuel noise at near-zero
  wind (Brattain 5.52 → 4.21). This rules out "the wind law dilutes an
  otherwise-preserved terrain effect" as the explanation — the arrival
  rule's minimum-travel-time relaxation is simply less able to produce
  extreme elongation from stochastic terrain/fuel heterogeneity alone
  than Bernoulli's per-tick dice rolls were, on top of (not instead of)
  the growth shortfall above.

**Replay diagnostic (optional, done — cheap in the event).** The task
asked for a largest-component-elongation check on Brattain/Ferguson/Pier
if `SMC_MAP_REPLAY` is cheap. It was run for all three (`exp_r6_arrival_replay.py`,
top 5 elites at/above observed size, 3 fresh seeds each): **14.5 seconds
total**, because Brattain and Ferguson have **zero** elites at or above
their observed growth to replay — the same "unreachable in size" finding
above, now visible as an empty candidate list rather than an inferred
one. Pier has exactly **one** qualifying elite; replayed across 3 seeds
it reaches largest-component elongation **1.16** (largest-component
fraction **1.000** — a single connected piece every time, no scatter),
essentially identical to the archive's own at-size ceiling and still
well short of Pier's observed 1.45. So Pier's shortfall, at least, is
confirmed to be a genuine shape limit and not a connected-component
artefact of the kind E43's replay found on Buck — there was simply
nothing to check for Brattain or Ferguson.

## Result 2 — E30 forecast

Per-seed change from the E33 twin, mean consensus IoU (E30 − E33):

| Fire | seed 0 | seed 1 | seed 2 | seed 3 | seed 4 | mean Δ | beyond E33 sd? |
|---|---|---|---|---|---|---|---|
| Bear | −0.013 | −0.023 | −0.025 | +0.009 | −0.031 | **−0.016** | yes (sd 0.015) |
| Brattain | −0.095 | −0.135 | −0.131 | −0.091 | −0.104 | **−0.111** | yes (sd 0.004) |
| Buck | +0.029 | +0.042 | +0.011 | +0.087 | +0.004 | +0.035 | no (sd 0.039) |
| Chimney | −0.134 | −0.165 | −0.171 | −0.128 | −0.121 | **−0.144** | yes (sd 0.012) |
| Ferguson* | −0.074 | −0.095 | −0.117 | −0.060 | −0.062 | **−0.081** | yes (sd 0.007) |
| Pier* | −0.035 | −0.041 | −0.038 | −0.067 | −0.075 | **−0.051** | yes (sd 0.003) |

Bold marks a fire whose mean change is beyond its own E33 five-seed
noise floor; every bold entry here is a **loss**, not a gain. `*` is the
holdout pair.

Mean ± sd, E33 / E30, Brier, Circle, Ellipse, final contained fraction,
and the E40 reference row (unmodified, not combined with the arrival
kernel):

| Fire | E33 | E30 | Brier E33 | Brier E30 | Circle (both) | Ellipse (E30) | Contained E33 | Contained E30 | E40 (reference) |
|---|---|---|---|---|---|---|---|---|---|
| Bear | 0.479 ± 0.013 | 0.463 ± 0.005 | 0.0510 | 0.0525 | 0.541 | 0.513 | 1.00 | 0.98 | 0.542 |
| Brattain | 0.416 ± 0.003 | 0.304 ± 0.018 | 0.1090 | 0.1087 | 0.450 | 0.469 | 1.00 | 0.93 | 0.479 |
| Buck | 0.590 ± 0.035 | 0.625 ± 0.007 | 0.0468 | 0.0412 | 0.670 | 0.701 | 1.00 | 1.00 | 0.653 |
| Chimney | 0.434 ± 0.010 | 0.291 ± 0.018 | 0.1276 | 0.1245 | 0.372 | 0.247 | 0.81 | 0.56 | 0.576 |
| Ferguson* | 0.344 ± 0.007 | 0.262 ± 0.019 | 0.1380 | 0.1434 | 0.373 | 0.503 | 1.00 | 0.62 | 0.604 |
| Pier* | 0.535 ± 0.003 | 0.483 ± 0.017 | 0.1093 | 0.1191 | 0.559 | 0.566 | 1.00 | 0.98 | 0.584 |

How to read it: Circle is identical (to 3 decimals) between E33 and E30
by construction — it is a deterministic null computed from the truth
mask alone, independent of which spread rule the ensemble uses; it sits
in one column here for that reason, not because it was checked twice and
happened to agree. Ellipse (`ellipse_iou`) has no E33-side number because
E33's raw reports predate the field (no `binary_git`, no `ellipse_iou`
key anywhere in `scores` — checked directly). "E40 (reference)" is
Task 5's own five-seed mean consensus IoU, run months before this task,
under `bernoulli`/`exponential` with `state_correction: immigrants`, not
re-run and not combined with `SMC_SPREAD=arrival`; it is here only so a
reader can see where E30 sits relative to a different, larger change to
the same base configuration.

Learned p0 and wind × (median of each seed's own final ensemble mean,
across 5 seeds), E33 vs E30:

| Fire | p0, E33 | p0, E30 | wind ×, E33 | wind ×, E30 |
|---|---|---|---|---|
| Bear | 0.19 | 0.31 | 0.68 | 0.61 |
| Brattain | 0.33 | 0.40 | 0.77 | 0.80 |
| Buck | 0.24 | 0.24 | 0.78 | 0.73 |
| Chimney | 0.35 | 0.40 | 0.69 | 0.47 |
| Ferguson* | 0.33 | 0.43 | 0.83 | 0.71 |
| Pier* | 0.24 | 0.32 | 0.76 | 0.73 |

**`p0` is not the same quantity in these two columns.** Under Bernoulli
it is a per-tick, per-burning-neighbour ignition *probability*, capped at
1 and effectively saturating well below that once several neighbours are
burning. Under arrival it is a *rate* in cells per tick with no
saturation at all — E30a's own flat-grid table found arrival's front
speed at a given p0 is roughly half Bernoulli's at low p0 (0.256 vs 0.474
cells/tick at p0 = 0.12) and only converges towards parity at the top of
the shared 0.08–0.6 range. So a rise in the learned p0 median (5 of 6
fires: Bear +0.12, Brattain +0.07, Chimney +0.05, Ferguson +0.10, Pier
+0.08; Buck flat) is exactly what the filter is expected to do to
compensate for a structurally slower rule — it is pushing p0 toward the
top of its range, the only speed knob arrival has — but the result table
above shows it is not enough: growth is still under-predicted on every
fire except Buck, and the wind × median *falls* on five of six fires
(every fire except Brattain), which is the opposite of what would help
close a growth shortfall and is consistent with the filter trading away
wind's (mild, at these speeds) shape contribution once the ensemble is
already struggling to match sheer area.

- **Five of six fires lose more than their E33 noise floor; none of the
  three fires the prediction named as likely to improve actually did.**
  Chimney (−0.144) and Brattain (−0.111) — the two the prediction
  expected might move, if either moved at all — take the two *largest*
  losses in the whole table, not gains.
- **Buck is the only fire that improves, and only inside its own (large)
  noise band.** +0.035 against a 0.039 sd is a tie, not a confirmed win,
  though it is the one fire where Brier also improves (0.0468 → 0.0412).
- **The forecast falls further behind the Circle on every degraded
  fire.** Because Circle does not move, a falling ensemble score is a
  widening gap: Chimney goes from beating neither by much (0.434 vs
  0.372, +0.062 over Circle) to trailing further under (0.291 vs 0.372,
  −0.081) — E30's Chimney forecast is now *worse* than the trivial
  area-matched null it used to beat.
- **Containment collapses on the fires that lost the most.** Final
  contained fraction falls from 0.81–1.00 under E33 to 0.56–0.62 on
  Chimney and Ferguson under E30 — consistent with (not proof of) a
  slower-growing ensemble producing a lower observed growth *rate*
  signal for the containment operator to key off, since containment is
  learned from `sigmoid(a + b·ln growth)`.
- **Brier moves less than IoU and in mixed directions, and misses the
  prediction's own ± 0.005 bound on two fires, not zero.** Bear +0.0015
  (tie), Brattain −0.0003 (tie/slightly better), Buck −0.0056 (better),
  Chimney −0.0031 (better — Chimney's Brier actually improves even as its
  IoU collapses, because Brier rewards a well-calibrated *probability*
  map more than a binary threshold does, and a smaller, slower-growing
  ensemble can still be well-calibrated about *where* little has burned).
  **Ferguson +0.0054 and Pier +0.0098 both exceed the ± 0.005 bound**,
  Pier by nearly 2×. Pier's Brier move is in fact the single largest
  Brier degradation in the table, on the same fire whose IoU sd (0.003)
  is the tightest of all six — a fire where E33 was already very
  reproducible and E30 is reproducibly worse.

**The prediction, checked line by line.**

- *"E37b: the maximum elongation at the observed size rises above the
  observed value on at least two of Brattain, Ferguson, Pier ... if the
  scenario wind, scaled by the gene, reaches ≥ 1.0 m/s."* **Not tested on
  its own terms and moot.** The learned/searched wind × values do reach
  well past 1.0 in places (E37b elites use wind × up to 1.5, same range as
  E37), but the *growth* shortfall means Brattain and Ferguson never even
  reach the observed **size**, so there is no "maximum elongation at the
  observed size" to compare — the clause's premise (a shape ceiling to
  measure) does not hold.
- *"With ERA5 winds of 0.5–0.7 m/s the head:flank ratio is still < 1.2,
  so only Chimney and Brattain move, and the input wind is the remaining
  blocker."* **Refuted, and by a wide margin.** Neither fire "moves" in
  the intended (positive) direction under E37b; both lose ground (Bear
  and Chimney flip from reachable to unreachable; Brattain's own
  "unreachable" gets worse, not better). The identified blocker (ERA5
  wind speed) was correct as far as it goes, but a second, larger
  blocker (the p0-unit change) was not anticipated and dominates the
  result.
- *"Forecast: Chimney up by > sd; Brattain, Ferguson up by ≥ 0.02."*
  **Refuted, in the opposite direction.** Chimney is down 0.144 (12× its
  own sd); Brattain is down 0.111; Ferguson is down 0.081. All three are
  losses, not gains, and all three exceed the *magnitude* named in the
  prediction, just with the sign flipped.
- *"Bear, Buck, Pier ties."* **True only for Buck** (+0.035, inside its
  0.039 sd). Bear (−0.016) and Pier (−0.051) both move beyond their own
  (much tighter) sd bands — Pier's sd is only 0.003, so even a modest
  absolute loss there is a large multiple of its noise floor.
- *"Brier not worse than E33 by more than 0.005 anywhere."* **Refuted on
  two of six fires.** Ferguson misses by a hair (0.1380 → 0.1434,
  +0.0054); Pier misses badly (0.1093 → 0.1191, +0.0098, nearly twice
  the bound). The other four fires hold (Bear +0.0015, Brattain −0.0003,
  Buck −0.0056, Chimney −0.0031).

**What it means.** E30a's flat-grid validation was correct on its own
terms — the arrival rule with rear-focus is shape-stable and magnitude-
accurate for LB ≤ 1.5, and nothing in this task's results contradicts
that regime claim. What E30a's flat-grid setting could not surface is
that **the same numeric p0 gene range means something different under
the two rules**, and every real scenario file, every illumination gene
list, and every forecast prior in this codebase was written assuming
Bernoulli's saturating-probability p0. Swapping the rule without
re-deriving the gene range for the new units is not a fair like-for-like
test of the *kernel* — it is a test of "the kernel, plus an
under-calibrated speed prior," and the results here (a shrinking wedge,
a forecast that loses to its own E33 twin on 5 of 6 fires and now loses
to the Circle on fires it used to beat) are consistent with that
explanation end to end: the growth shortfall in the illumination and the
growth shortfall implied by the forecast's own falling scores and rising
p0 posteriors are the same effect measured two different ways. This is
not evidence against the arrival rule's mechanism (E30a already showed
that holds); it is evidence that **this specific experiment did not
re-tune the one knob (`p0`'s range) whose meaning the mechanism change
invalidated**, and the task's own instructions (E37 settings *exactly*,
gene ranges numerically unchanged) intentionally ruled that re-tuning
out of scope for this task.

**Questions this raises.**

- What p0 range under arrival reproduces Bernoulli's *speed* distribution
  on these six fires (e.g. by matching the median front speed the E33
  posterior implies, then solving E30a's own closed-form rate formula for
  the equivalent arrival p0)? Open — the natural next step before
  re-running either E37b or the forecast; re-tuning p0 without touching
  the wind law would isolate the kernel's shape effect from the units
  problem this file found.
- Does spotting (E43), which was shown to add real reach on Ferguson and
  a fragile margin on Pier under the *unmodified* Bernoulli kernel,
  combine with a properly re-tuned arrival kernel, or are the two
  mechanisms redundant (both ultimately add reach via a
  wind-aligned effect)? Open; E43 and this task were deliberately kept
  isolated from each other (per Task 8's own scope) and neither combined
  run has been tried.
- Is the wind × posterior's fall on four of six fires (Bear, Buck,
  Chimney, Ferguson) a real signal that rear-focus's shape effect is
  being traded against a growth deficit, or is it within the ensemble's
  ordinary seed-to-seed noise for that gene? Open — this task did not
  measure a noise floor for the wind × posterior itself, only for
  `mean_consensus_iou`.
- Does the growth shortfall shrink or vanish on a *smaller* fire (shorter
  distance to travel in 5 days), suggesting the units problem is mostly a
  large-fire artefact, or is it uniform across sizes the way E30a's own
  flat-grid speed table suggests it should be? Open, not tested on real
  terrain here.

**Verdict.** REJECTED as tested. Neither half of the acceptance test
passed: E37b's reachable region does not widen (it shrinks on every
fire, and two fires that were merely "wrong shape" under E37 become
"can't reach this size at all" under E37b), and the forecast loses to
its own E33 twin by more than the noise floor on five of six fires,
including both fires the pre-registered prediction expected to improve.
The mechanism identified — `p0`'s meaning changing from a saturating
probability to an unsaturating rate, with the gene range left numerically
unchanged per this task's own instructions — is a well-supported,
specific explanation (traceable through the illumination's growth
ceilings, the forecast's rising p0 posteriors, and E30a's own flat-grid
speed table) rather than an unexplained regression, and points directly
at the fix: re-derive the p0 range for the arrival rule's units before
judging the kernel itself on real fires again.

**Later.** Not yet revisited at the time this file's verdict was written.
A re-tuned-p0 re-run of both halves is the obvious next step (see
"Questions this raises"); E43's spotting combination remains open and
untried under either kernel.

**Later (2026-09-12, correcting the root-cause framing with numbers, before
E30b): the prior ceiling was not the forecast's own binding constraint.**
The verdict above traces the forecast's losses to "`p0`'s meaning
changing... with the gene range left numerically unchanged," which reads
as if the learned posteriors were pinned at the prior's own top (0.6) and
straining against it. They were not: the E30 learned p0 **means** (final
window, five seeds) were Bear 0.302, Brattain 0.373, Buck 0.237, Chimney
0.397, Ferguson 0.429, Pier 0.292 — every one comfortably inside the
log-uniform [0.08, 0.6] prior (log-midpoint ≈ 0.22) and nowhere near its
0.6 edge. The prior *ceiling* did not bind in the forecasts. Two things did,
and they are different mechanisms in the two halves of this file:

- **In the E37b illumination**, the one-cell-per-tick clock cap *combined
  with* the prior's top did bind, because illumination samples the whole
  prior range rather than letting a filter settle on a posterior: at 50
  ticks/day the cap is 1.5 km/day, and Brattain's day-5 shape needs its
  rear-focus head to cover roughly 480 cells in 250 ticks (≈ 1.9
  cells/tick) — above what even p0 = 0.6 can give under arrival (≈ 0.62
  cells/tick), so no elite in the search could reach it, prior width or
  not.
- **In the forecasts**, with the posteriors sitting mid-prior rather than
  pinned at an edge, the more likely binding constraint is the direction
  *input*: E41 found the ERA5 daily wind direction is wrong on Chimney and
  Bear and right on Ferguson and Brattain, and a directional kernel
  (arrival + rear_focus) is punished by a wrong direction in a way the
  round, direction-blind Bernoulli blob never was. That is consistent with
  (though this file did not test it directly) Chimney's and Brattain's
  losses being partly a direction-input problem, not purely a speed-prior
  problem, on the forecast side.

This does not overturn the verdict above (REJECTED as tested, and the
prior's *range* is still the wrong units for arrival's cells/tick — that
part is unaffected), but it changes what "re-derive the p0 range" was
expected to buy: since the forecasts were not prior-ceiling-limited, widening
the ceiling alone is not guaranteed to fix them the way it plausibly fixes
E37b's growth-shortfall problem. **E30b tests both fixes explicitly**: a
4x clock (raising the illumination's own cap, independent of the prior)
together with a wider prior, in one arm (Arm A), and the same plus a
learned per-member wind-direction offset gene, in a second arm (Arm B) —
so the clock/prior fix and the direction fix are each given their own arm
to succeed or fail on, rather than being bundled into one change and
credited or blamed together.
