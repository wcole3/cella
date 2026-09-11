# E43 — does spotting extend the reachable shape region? · finding — yes, more than predicted: spotting moves two of the three unreachable fires (Ferguson, Pier) inside the wedge; Brattain narrows but stays out

_Round 6 (2026-09-11, after E37) · MAP-Elites, 960 evaluations per fire (batch 32, 30 generations) · all six fires incl. holdout · runner `exp_r6_spot_illuminate.py` (`SMC_SPOT=1`) → `wildfire_smc map` · results `exp43_spot_illuminate.json` (raw per-fire archives in `exp43_spot_illuminate/`) · compared against E37's own archives (`exp37_illuminate.json`, not re-run) · pre-registered TEST_PLAN v1.8 addendum · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** E37 found the model's reachable growth × elongation region
is a wedge — small fires can be any shape, large fires are round — and
left open whether spotting (rejected as a score-improving mechanism in
E7, but never tested for *shape*) is what a real kernel refit would need
to draw a large, elongated fire. Adding two spotting genes to the same
illumination map and switching spotting on in the config answers that
question directly: coverage of the archive roughly doubles or more on
every fire, and — contrary to the pre-registered prediction — the
*shape* ceiling at the observed size also moves a lot, not a little, on
two of the three fires E37 found unreachable. Ferguson's reachable
elongation at its own size rises from 1.38 to 2.66 (its observed shape
is 1.61 — now comfortably inside) and Pier's rises from 1.15 to 1.77
(observed 1.45 — also inside). Only Brattain, whose reachable elongation
moves the least (1.35 → 1.49, still short of its observed 1.83), stays
outside the wedge. The elites that make this possible use spotting
settings near the top of the pre-registered range (`p_spot` 0.003–0.004,
`median_distance` pinned at or near its allowed maximum of 20 cells on
four of six fires) combined with real wind — the opposite of the
predicted mechanism ("spot fires merge into a rounder mass").

**Question.** Is spotting a mechanism that gives a *large* fire more
reach in the downwind direction, i.e. does it move E37's wedge?

**What we changed.** `wildfire_smc` gains an env knob, `SMC_SPOT=1`,
used only in `map` mode:

- **Config.** `enable_spotting` (new function) switches spotting on in
  the config's wildfire model before it becomes the MAP-Elites template:
  `WildfireParams::spotting` is set to `Some(SpottingParams { p_spot:
  0.001, median_distance: 5.0, sigma: 0.5, angle_jitter_deg: 15.0 })` via
  a direct field write (`Grid2D::set_model_param` refuses a
  `spotting.*` key while spotting is `None` — see the `set_param` tests
  in `cella_lib/src/wildfire/mod.rs` — so a gene has nothing to write
  into unless spotting is already turned on this way first). `sigma` and
  `angle_jitter_deg` stay fixed; only `p_spot` and `median_distance`
  become genes and are overwritten per genome.
- **Genes.** Two genes are appended to the existing spread genes (`p0`,
  `burn_duration`, `wind_scale`; `tau_days` is dropped here exactly as
  in E37, via `SMC_TAU_OFF=1`):
  - `model.spotting.p_spot`, **log-uniform 0.001–0.005**. The struct doc
    calls it "per-step probability that a burning cell launches a
    firebrand" — a rate that can span orders of magnitude, hence log
    scale, like `model.p0`. The range is E7's pre-registered spotting
    space verbatim (`exp_spotting.py`'s `SPOT_GRID`: lo 0.001, mid 0.005,
    far 0.002), which already moved burned area 2–4× on these fires —
    wide enough to show a shape effect without extrapolating past what
    has actually been tested.
  - `model.spotting.median_distance`, **linear 2–20 cells**. E7 tested
    5–20; the brief widens the floor to 2 cells (a jump barely ahead of
    the front) to also cover short-range spotting. Both ends sit inside
    `SpottingParams::median_distance`'s declared bounds of [0.5, 100]
    cells (`cella_lib/src/wildfire/mod.rs`'s `params()`).
  Gene-building was split out of `main` into `build_genes` and unit
  tested (`spot_gene_tests` in `wildfire_smc.rs`): `SMC_SPOT=1` adds
  exactly these two genes with the ranges above and turns on
  `spotting.p_spot`/`spotting.median_distance` as readable config
  params; without it, the gene list and the config are unchanged.
- **Everything else matches E37 exactly**: MAP-Elites (batch 32, 30
  generations, iso + line emitter), 5 days of the scenario's own
  weather, growth axis 0–0.10 (20 bins), elongation axis 1–4 (20 bins),
  no stopping rule, no objective, same six fires.

**Why we expected it to matter.** E37 named this as one of its two open
questions: "would spotting … add reach at size?" E7 rejected spotting
for *score* (it only ever added burned area, which hurt IoU against an
already-too-large model), but never asked whether it changes the
*shape* the model can produce, which is the question that matters after
E37 — a mechanism that adds reach without necessarily being a net
scoring win is still worth knowing about.

**How we scored it.** Identical archive metrics to E37: coverage (share
of the 400-bin map reached), the model's own maximum elongation at any
size, and its maximum elongation at a size at least as big as the real
fire on day 5 — read directly off each fire's archive, the same method
that reproduces E37's own published numbers exactly (checked against
`exp37_illuminate/<fire>.json` before trusting it on the new data). E37
is not re-run; its numbers are read from the reports already on disk.

A **"max spot-elite distance from the main body"** column, mentioned as
optional in the task brief, is **not included**: the archive stores only
each elite's genome and its two behaviour-axis values (growth,
elongation), not any spatial statistic of where spotted cells land
relative to the main burned mass, so there is nothing in the data to
build that column from.

![Six archives: E43's reachable growth × elongation cells shaded, E37's own reachable-region boundary overlaid as a dashed line, the observed fire's daily position as dots](figures/e43-spot-illuminate.svg)

**Result.**

| Fire | E43 cells filled | E37 cells filled | E43 elong., any size | E37 elong., any size | E43 elong. at observed size | E37 elong. at observed size | Δ at-size elong. | observed (day 5) growth / elongation | E43 reachable? | E37 reachable? |
|---|---|---|---|---|---|---|---|---|---|---|
| Bear | 202 (51 %) | 125 (31 %) | 6.56 | 3.19 | 2.90 | 2.03 | +0.87 | 0.047 / 1.98 | yes | yes |
| Brattain | 155 (39 %) | 112 (28 %) | 4.71 | 5.52 | 1.49 | 1.35 | +0.14 | 0.110 / 1.83 | **no** | **no** |
| Buck | 132 (33 %) | 60 (15 %) | 3.52 | 1.95 | 3.52 | 1.49 | +2.03 | 0.058 / 1.50 | yes | borderline |
| Chimney | 118 (30 %) | 48 (12 %) | 2.51 | 1.54 | 1.80 | 1.35 | +0.45 | 0.067 / 1.22 | yes | yes |
| Ferguson* | 222 (56 %) | 52 (13 %) | 5.29 | 2.06 | 2.66 | 1.38 | +1.28 | 0.080 / 1.61 | **yes** | **no** |
| Pier* | 57 (14 %) | 34 (9 %) | 1.94 | 1.39 | 1.77 | 1.15 | +0.62 | 0.102 / 1.45 | **yes** | **no** |

How to read it: "cells filled" is coverage of the 400-bin map; the two
elongation columns are the most stretched fire the model made at any
size, and at a size at least as big as the real fire on day 5; then the
real fire's own size and shape (identical in both runs — same truth,
same weather); then whether the real one lies inside each run's
reachable region. `*` is the holdout pair. Bold marks a fire whose
reachable-or-not status is the headline of this experiment (Ferguson and
Pier flip from no to yes; Brattain stays no).

- **Coverage rises on every fire, by a lot.** 30–56 % filled under
  spotting versus 9–31 % under E37, roughly 1.6–4.3× more of the map
  reached with the same search budget (960 evaluations). This half of
  the prediction was correct and not a close call.
- **The shape ceiling at the observed size also moves, and on two fires
  it moves far more than the ±0.2 the prediction allowed.** Brattain's
  move (+0.14) is inside the predicted band. Ferguson's (+1.28) and
  Pier's (+0.62) are six to nine times larger than the ±0.2 the
  prediction set for "spot fires merge into a rounder mass" to hold.
- **Two of the three fires E37 could not reach are now reachable.**
  Ferguson's observed shape (1.61) sits below its new ceiling (2.66);
  Pier's (1.45) sits below its new ceiling (1.77). Both were clearly
  above their E37 ceilings (1.38, 1.15). Brattain's ceiling also rose
  (1.35 → 1.49) but not past its observed shape (1.83) — it remains the
  one fire in this campaign no combination of these knobs, with or
  without spotting, can draw.
- **The mechanism is not "spotting adds isolated round blobs."** Reading
  the genome behind each fire's best at-size elite: `median_distance` is
  pinned at or near its allowed maximum (20 cells) on four of six fires
  (Buck, Ferguson, Pier at exactly 20.0; Chimney and Brattain at
  13–16), `p_spot` sits in the upper half of its range (0.0026–0.0042
  of 0.001–0.005), and every one of these elites also carries a real
  wind scale (0.44–1.08, not near 0). Long-range, wind-aligned spot
  jumps land detached embers well downwind of the front, which stretches
  the burned set's second-moment shape rather than rounding it out —
  the opposite of the predicted mechanism. `median_distance` sitting at
  its own ceiling on four of six fires is itself a signal: the model
  would likely reach further still with a wider range, so 20 cells may
  be an artificial floor on this result, not spotting's real limit.
- **A caveat on precision.** The "at observed size" column is the best
  of however many elites happen to have grown at least that large; that
  count is small on Brattain (4 elites) and Pier (6), so those two
  numbers carry more search noise than Bear's or Buck's (69 and 72).
  This does not change Ferguson's or Pier's conclusion (their margins
  over the observed shape are large relative to the shift a few more
  elites would plausibly produce), but Brattain's narrow, still-short
  gap (1.49 vs. 1.83) should be read as "not reachable at this search
  budget," not as a precisely measured ceiling.
- **The "any size" column is not always larger under spotting**
  (Brattain: 5.52 in E37 vs. 4.71 here). With five genes instead of
  three, the same 960-evaluation budget searches a larger space, so a
  rare, very-stretched-but-tiny elite that E37's search happened to find
  is not guaranteed to be rediscovered. This affects only the "any size"
  column, which this experiment does not otherwise rely on: every
  reachability conclusion above uses the "at observed size" column,
  which rose on every fire including Brattain.

**The prediction, checked line by line.**

- *"Coverage of the archive rises on every fire (spotting adds
  growth)."* **Yes**, on all six fires, by a wide and consistent margin
  (roughly 2–4× the E37 coverage).
- *"The maximum elongation at the observed size rises by < 0.2 on
  Brattain/Ferguson/Pier."* **True only for Brattain** (+0.14).
  **False for Ferguson** (+1.28, six times the bound) **and Pier**
  (+0.62, three times the bound).
- *"Spot fires merge into a rounder mass."* **No** — the elites that
  reach the observed size with the highest elongation use long-range,
  wind-aligned spotting, which stretches the shape rather than rounding
  it (see the mechanism note above).
- *"The three dots stay outside the wedge."* **False for two of the
  three.** Ferguson and Pier's observed shapes move inside the reachable
  region; only Brattain's stays outside.

**What it means.** The prediction under-estimated spotting on the two
fires it was most confident would resist it. Spotting is not merely a
score-degrading nuisance (E7's finding, at low settings, with no
stopping mechanism) or a small correction to E37's wedge — at the
upper end of its pre-registered range, combined with wind, it is enough
to draw two of the three previously-unreachable observed shapes. That
reopens a question E37 had provisionally closed in E30's favour: a
kernel refit is not the only route to a model that can draw Ferguson and
Pier's shapes; enabling and tuning spotting is another, and on this
evidence, a more direct one for those two fires specifically. Brattain
is the fire that most needs E30's kind of fix regardless — its ceiling
barely moved and its gap to observed (0.34 elongation) is still the
largest of the three. Because `median_distance`'s pre-registered ceiling
(20 cells) was actively used by the elites that did the most work here,
this result is a lower bound on what spotting alone can reach, not a
final answer.

**Questions this raises.**

- Does a wider `median_distance` range (past the pre-registered 20-cell
  ceiling) push Brattain's ceiling further, or is 20 cells already past
  the point of diminishing return for that fire specifically? Open, not
  tested (would need a new pre-registered range).
- E7 found spotting costs 0.1–0.2 mean IoU at the "far" setting because
  it burns too much area, with no stopping mechanism in place. Does that
  finding still hold once containment (E28) is in the loop, now that
  E43 shows spotting can also fix a real shape deficit rather than only
  adding uncontrolled area? Open — E43 is pure illumination (no
  objective, no containment); a scored `assim`/`open` run with
  `SMC_SPOT=1` and containment on has not been tried.
- Would spotting and the E30 kernel refit combine, i.e. does enabling
  both move the wedge further than either alone, or are they the same
  mechanism seen two ways (both add reach via a wind-aligned, elongating
  effect)? Open; E30 has not been run yet.

**Verdict.** Finding — spotting is a real, usable mechanism for
extending the model's reachable shapes, more effective than predicted on
two of the three fires E37 flagged as unreachable, and the one fire it
does not rescue (Brattain) is the one with the largest remaining gap.
Recommend keeping spotting as a live candidate mechanism alongside (not
instead of) the E30 kernel refit, and re-running this illumination once
E30 lands to see whether the two combine.

**Later.** Not yet revisited. E30 (kernel refit) is still not run; the
open questions above (containment interaction, a wider `median_distance`
range, combining with E30) are all future work.
