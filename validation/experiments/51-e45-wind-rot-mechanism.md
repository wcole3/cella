# E45 — mechanism ablations on the `wind_rot_deg` gene · RESULTS PENDING

_Round 7 (2026-09-24) · two batches, one arm per batch, run one at a
time, plus an optional third small batch: Arm B-σ0 (`SMC_WIND_ROT_
SIGMA=0`, 18 runs), Arm B-20 (`SMC_WIND_ROT_GENE=20`, 18 runs), and —
only if the two above finish within budget — Arm B itself re-run with
`SMC_DIAG=1` to recover the per-window `wind_rot_deg` IQR series E44
did not capture (18 runs) · `assim` mode, 32 members, seeds 0–2 (three
seeds — see "Seed count," below), six fires, 2 workers per batch · env
knobs: Arm B-σ0 = `r7_common.ARM_B` + `SMC_WIND_ROT_SIGMA=0`
+ `SMC_DIAG=1`; Arm B-20 = `r7_common.ARM_B` with `SMC_WIND_ROT_GENE=20`
in place of Arm B's own `90` + `SMC_DIAG=1`; Arm B diag (optional) =
`r7_common.ARM_B` unchanged + `SMC_DIAG=1` · runner `exp_r7_e45.py`
(`--arm {sigma0,20,armb_diag} --seeds 3`) → `exp45_wind_rot_mechanism_
sigma0.json`, `exp45_wind_rot_mechanism_20.json`,
`exp45_arm_b_diag.json` (+ raw reports under each name's own directory)
· compared against Arm B's own five-seed mean and sd from E44
(`exp44_arm_b_5seed_summary.json`'s `arm_b_sd` block,
`r7_common.arm_b_baseline()`), not E33 · pre-registered TEST_PLAN v1.9,
§9 · σ0 batch: `binary_git 7246387` (clean HEAD, verified in all 18
summary rows and all 18 raw reports), load(1 min) 1.46 at launch → 8.43
at finish, wall time 15012.2 s ≈ 4.17 h at 2 workers (pre-registered
estimate ≈ 3.5 h) · ±20° / Arm B diag batches: `binary_git`/load/wall
[PENDING] (**every wall time here: shared box, 2 workers, do not
compare against each other or against E44's**) · terms:
[GLOSSARY.md](GLOSSARY.md)_

**In short.** The σ0 half is in; the ±20° half is not yet. Consensus
IoU: four of six fires tie Arm B within 1 sd (Bear, Brattain, Ferguson,
Pier), exactly clearing the prediction's own "≥ 4 fires" bar, not
comfortably above it. The other two split — Chimney gains beyond 1 sd
(+1.13), Buck loses beyond 2 sd (−2.92), the clearest single loss in
this batch. The IQR series is the more striking result, and complicates
the experiment's own framing: `wind_rot_deg`'s per-window spread
**narrows sharply on every one of the six fires even with mutation
frozen** (from a birth spread of ≈ 97° down to 23–43° on Bear/Pier and
under 3° on Brattain/Buck/Chimney/Ferguson) — so "the IQR narrows"
cannot, on its own, distinguish a *learned* bearing from resampling
alone thinning out birth draws that were never allowed to change. See
Result 1 below and the "What it means" section once the ±20° batch is
in. _[This section is extended after the ±20° batch (Phase 3) and
finalised once the optional Arm B diag batch (if run) is in, with the
prediction checked clause by clause and the one-sentence mechanism
stated plainly.]_

**Question.** Does the `wind_rot_deg` gene (E30b, ±90° per-member wind-
direction offset, part of Arm B — see Arm B in the glossary) help the
forecast because per-member angular *diversity* lets the ensemble cover
a range of bearings and fit whichever one the day's actual wind needs,
or because the filter *learns* one correct bearing and the ensemble
converges on it? These predict opposite things about what happens when
mutation is switched off for this one gene (diversity should survive;
learning should not) and about what happens when the gene's range is
narrowed to ±20° (a diversity story should lose the most on the fires
where Arm B's own learned medians sit near or past that ±20° edge —
Bear and Pier, at ±43° — since narrowing removes bearings the ensemble
was actually using; a pure-learning story would only be hurt if the
*correct* bearing itself lies outside ±20°).

**What we changed.** Two things, one gene, tested separately (never in
the same run):

1. **Arm B-σ0** — the Arm B preset (`r7_common.ARM_B`: arrival kernel,
   rear-focus wind law, 4× clock, the `arrival_x4` prior, `wind_rot_deg`
   at ±90°) plus a new per-gene mutation-size override,
   `SMC_WIND_ROT_SIGMA=0`, applied to `wind_rot_deg` only (every other
   gene keeps mutating at the engine's own sigma, `SMC_SIGMA=0.2` by
   default). Each member still *draws* its own birth rotation from the
   ±90° prior and resampling still copies whichever members survive
   selection — only the mutation step that would nudge a member's own
   rotation after birth is switched off. If diversity alone does the
   work, this arm should track Arm B closely; if learning does the
   work, this arm should lose ground once the population can no longer
   correct a bad birth draw.
2. **Arm B-20** — the Arm B preset with `SMC_WIND_ROT_GENE=20` in place
   of Arm B's own `90`, i.e. the gene's range narrows to ±20° (mutation
   stays on, at the engine's own sigma). If the filter only ever needed
   a narrow correction, this should barely move the score; if it needed
   the wider ±90° range Arm B actually used, this should lose ground
   specifically on the fires whose Arm B posterior sits near or past
   ±20°.

**`SMC_WIND_ROT_SIGMA` did not exist before this task** (the Round 7
Task 2 skeleton only declared the arm's *name*). It is now a genuine
per-gene mutation-size override, implemented as a one-line change to
`cella_lib::explore::genome::GeneSpace::resolve`'s per-gene `sigma`
validation (`s > 0.0` → `s >= 0.0`, error text "sigma must be a positive
number" → "sigma must be a non-negative number"). The brief's own
evidence for "the library rejects 0" was a GUI slider floored at 0.01;
checked directly, that premise does not quite hold as stated — the GUI
does not currently set a *per-gene* `sigma` override at all
(`gui::explore::gene_specs` always leaves `GeneSpec.sigma` at `None` for
every gene row), so no GUI control actually exercises the validation
this task fixed. The GUI's sigma sliders that do exist are two
*engine-wide* sigma controls (`edit.rs`'s "Mutation size," 0.01–1.0, the
scalar passed straight to `mutate_one`; `panels/explore.rs`'s Monte
Carlo "Sigma," 0.01–1.0, and its Evolve "Sigma," 0.0–1.0) — none of them
go through `GeneSpace::resolve`, and the engine-wide sigma they set
(`EnsembleConfig`/`EvolveConfig`'s own `sigma` field) is validated
separately, by `ensemble.rs::check_selection_settings`, which does and
should keep requiring `sigma > 0` there (an ensemble that never mutates
*any* gene is a real config error) — that check was not touched by this
task. The validation this task actually needed to relax lives only in
the per-gene override path a config file (or `SMC_WIND_ROT_SIGMA`) can
set directly; a *per-gene* override freezing one gene while every other
gene keeps mutating is a different, legitimate state from freezing the
whole ensemble (`cella_lib`'s own module doc for genes already described
`sigma` as "a fraction of the gene's size" without saying it could not
be zero — the validation, not the semantics, was what stood in the
way). `mutate_value`'s own arithmetic already
handles `sigma = 0.0` as a no-op for every gene kind it supports (a
Gaussian step of sd 0, a flip/redraw probability of 0, `⌈8 × 0⌉` bit
flips) — the fix only had to stop rejecting the value at the door. A
new library-level test
(`a_per_gene_sigma_of_zero_is_accepted_and_freezes_that_gene_only`,
`cella_lib/src/explore/genome.rs`) resolves a two-gene space with one
gene's sigma pinned to `0.0`, mutates it 200 times at a large engine
sigma, and asserts that gene never moves while the other (unpinned) gene
does — this is the acceptance check for exactly the semantics E45 needs
("each member keeps its birth draw, selection still acts on it").
Resampling was not touched by this task and needed no change: a
resampled child is a clone of its selected parent's genome (`Ensemble`'s
existing resampling step, unchanged since before E45), so a frozen
`wind_rot_deg` is copied along with every other gene exactly as before —
selection can still favour members whose birth draw happened to fit the
day's wind, it just can no longer *improve* a draw after the fact.

**Arm B-σ0 unset leaves every report byte-identical** to before this
task (`SMC_WIND_ROT_SIGMA` unset ⇒ `GeneSpec.sigma` stays `None` ⇒ the
gene mutates at the engine's own sigma exactly as E44 ran it) — the
Task 1 acceptance property this task was told to keep. Unit tests for
this and the sigma-override wiring live in
`cella_lib/examples/wildfire_smc/priors.rs`
(`e30b_wind_rot_gene_tests` module): unset leaves the gene spec
unchanged, `SMC_WIND_ROT_SIGMA=0` sets a per-gene override on
`wind_rot_deg` only (every other gene's `sigma` stays `None`), and the
knob has no effect when `SMC_WIND_ROT_GENE` itself is unset (nothing to
attach an override to).

**Per-window IQR, the new diagnostic field this task adds.** Task 3
(E48) added `SMC_DIAG=1` per-window diagnostics with the ensemble's
per-window *median* `wind_rot_deg`, but medians alone cannot tell "the
population converged on one value" apart from "the population is spread
out but happens to be centred near the same place" — this task adds
`wind_rot_deg_iqr` (interquartile range: Q3 − Q1 of the gene over the
window's members, linear-interpolation quantiles — see IQR in the
glossary) beside the existing median, behind the same `SMC_DIAG=1`
switch, no new knob. Computed in `diag::iqr_gene`
(`cella_lib/examples/wildfire_smc/diag.rs`), sharing its sorted-values
helper with the existing `median_gene`; `None` under the same "gene not
part of this run's list" rule the median already uses. Unit tests: a
known 10-value vector (IQR = 4.5, worked by hand in the test's own
comment), a single-member window (IQR = 0.0, not `None` — there is a
gene, it just has nothing to spread over), the "absent gene" case, and a
serialisation check that `None` omits the JSON key entirely rather than
writing `null` (same convention as every other opt-in diagnostic field
in this report). `modes::open::run` now computes it beside the median,
from the same per-window member genomes `median_gene` already reads —
this task's per-window member genomes were accessible exactly where the
median already lives, no plumbing gap. Every raw report from this
task's own batches sets `SMC_DIAG=1`, so all three arms (Arm B-σ0,
Arm B-20, and Arm B itself if the optional third batch runs) carry the
series; TEST_PLAN v1.9's own design asks for Arm B's series too, but
E44's forecast batch did not set `SMC_DIAG` — see "Seed count and the
optional third batch," below, for how that gap is handled.

**How we scored it.** Per fire, mean and sd of one-window-ahead
consensus IoU across the run's seeds (`r7_common.fire_stats`), compared
against Arm B's own five-seed mean and sd from E44
(`exp44_arm_b_5seed_summary.json`'s `arm_b_sd` block,
`r7_common.arm_b_baseline()` — a new function this task adds to
`r7_common.py`, the "baseline option" the summariser needed so this
experiment is judged against Arm B's own noise floor instead of E33's),
with the same delta-in-sd verdict column every Round 7 write-up uses
("tie" = within 1 sd, "beyond 1 sd (gain|loss)",
"**beyond 2 sd (gain|loss)**" — `r7_common.verdict`/`summary_table`,
unedited). The per-window `wind_rot_deg_iqr` and `wind_rot_deg_median`
series (read directly from each raw report's `scores[*].diag`) are
reported as the IQR trend: whether the interquartile range narrows,
widens, or stays flat from the first scored window to the last, for
each arm this task has a series for.

**Seed count (pre-registered branch) and the optional third batch.**
TEST_PLAN v1.9's E45 entry pre-registers two seed-count branches and
lets E44's own result pick one: "seeds 0–4 **if** E44's Arm B five-seed
sd is ≤ the E33 sd on ≥ 4 fires; **otherwise** seeds 0–2, and the
write-up must say which branch applied." Checking Arm B's sd
(`exp44_arm_b_5seed_summary.json`) against E33's sd
(`r7_common.e33_baseline()`), fire by fire:

| Fire | E33 sd | Arm B sd (E44) | Arm B sd ≤ E33 sd? |
|---|---|---|---|
| Bear | 0.015 | 0.005 | yes |
| Brattain | 0.004 | 0.034 | no |
| Buck | 0.039 | 0.005 | yes |
| Chimney | 0.012 | 0.031 | no |
| Ferguson* | 0.007 | 0.014 | no |
| Pier* | 0.003 | 0.023 | no |

Only **two** of six fires (Bear, Buck) clear the bar — below the ≥ 4
threshold the "seeds 0–4" branch needs — so **this experiment runs
seeds 0–2 (three seeds)**, the "otherwise" branch, six fires, both arms:
18 runs per batch. This is a smaller seed count than E44's own five, so
every mean/sd this write-up reports carries a 3-seed sd, noisier than
the 5-seed Arm B baseline it is compared against — read accordingly, not
smoothed over.

There is no pre-registered stop rule for this experiment (TEST_PLAN
v1.9, verbatim: "none — this experiment runs regardless of E44's stop
rule; its write-up must name the mechanism in one sentence or say it
cannot"), so both batches run and are reported regardless of what either
one finds.

E44's own forecast batch did not set `SMC_DIAG=1`, so Arm B's own
per-window `wind_rot_deg` IQR series does not exist in any report on
disk — TEST_PLAN v1.9's design asks for Arm B's series alongside the two
ablation arms', but obtaining it means re-running Arm B, not reading an
existing report. The pre-registered ruling for this task (round-7
planning, Task 5 brief): run Arm B itself, seeds 0–2, `SMC_DIAG=1`, as a
**third, optional, 18-run batch, launched only if the σ0 and ±20°
batches above finish within this task's time budget**; otherwise this
write-up reports Arm B's IQR series as unavailable and says why (this
paragraph). This batch does not touch, overwrite or reinterpret E44's
own `exp44_arm_b_5seed.json`/`_summary.json` — it is a separate output
(`exp45_arm_b_diag.json`) used only for the IQR series, and its own
consensus-IoU mean/sd is not a fourth arm to compare against Arm B (that
comparison already exists, at five seeds, in E44).

**Prediction, written before the run (TEST_PLAN v1.9, §9, quoted
verbatim).** "Diversity wins — Arm B-σ0 within 1 sd of Arm B on ≥ 4
fires; Arm B-20 loses to Arm B beyond 1 sd on Bear and Pier (Arm B's own
learned medians there are ±43°)." Checked clause by clause once both
batches are in (Phase 4 of this task).

## Result 1 — Arm B-σ0 (three-seed forecast)

Provenance: all 18 summary rows (`exp45_wind_rot_mechanism_sigma0.json`)
and all 18 raw reports
(`exp45_wind_rot_mechanism_sigma0/*.json`) carry `binary_git 7246387`,
matching the clean HEAD this batch was built and launched from. Batch:
18 jobs (3 seeds × 6 fires), 2 workers, load(1 min) 1.46 → 8.43
(the box picked up other load during the run — **shared box, this run's
own numbers only**), wall time 15012.2 s ≈ 4.17 h — close to the
pre-registered ≈ 3.5 h estimate.

Mean one-window-ahead consensus IoU, three seeds, against Arm B's own
five-seed mean and sd from E44 (`exp44_arm_b_5seed_summary.json`'s
`arm_b_sd` block — this experiment's noise floor from here on, not
E33's):

| Fire | Arm B mean (5-seed) | Arm B sd (5-seed) | σ0 mean (3-seed) | σ0 sd (3-seed) | Delta σ0 (Arm B-sd) | verdict σ0 |
|---|---|---|---|---|---|---|
| Bear | 0.473 | 0.005 | 0.476 | 0.009 | +0.003 (+0.54 sd) | tie |
| Brattain | 0.437 | 0.034 | 0.424 | 0.019 | −0.013 (−0.40 sd) | tie |
| Buck | 0.640 | 0.005 | 0.624 | 0.017 | −0.016 (−2.92 sd) | **beyond 2 sd (loss)** |
| Chimney | 0.489 | 0.031 | 0.524 | 0.015 | +0.035 (+1.13 sd) | beyond 1 sd (gain) |
| Ferguson* | 0.386 | 0.014 | 0.379 | 0.009 | −0.007 (−0.54 sd) | tie |
| Pier* | 0.521 | 0.023 | 0.520 | 0.008 | −0.000 (−0.02 sd) | tie |

Table generated by `r7_common.summary_table()` off `r7_common.fire_stats()`
on the raw rows, baseline `r7_common.arm_b_baseline()` — verdict wording
unedited. `*` = holdout pair. Every σ0 3-seed sd here is smaller than
Arm B's own 5-seed sd (expected at a smaller n, but also plausibly real:
see the IQR result below — a frozen gene cannot introduce fresh
seed-to-seed scatter through the mutation channel, only through which
birth draws each seed's ensemble happens to keep).

Brier (ensemble mean) and final contained fraction (three-seed min–max),
next to Arm B's own five-seed Brier from E44's Result 1 table (quoted,
not re-derived):

| Fire | Brier Arm B (5-seed) | Brier σ0 (3-seed mean, sd) | Contained σ0 (3-seed min–max) |
|---|---|---|---|
| Bear | 0.0507 | 0.0518 (0.0013) | 0.875–1.000 |
| Brattain | 0.1020 | 0.1054 (0.0078) | 1.000–1.000 |
| Buck | 0.0396 | 0.0411 (0.0020) | 1.000–1.000 |
| Chimney | 0.0947 | 0.0840 (0.0081) | 0.812–1.000 |
| Ferguson* | 0.1356 | 0.1369 (0.0037) | 1.000–1.000 |
| Pier* | 0.1074 | 0.1121 (0.0076) | 1.000–1.000 |

Brier moves in the same direction as consensus IoU on every fire here
(worse on Bear/Brattain/Buck/Ferguson/Pier, better on Chimney) — no
split the way E44 found for Pier's own Brier vs IoU.

**Learned `wind_rot_deg`, three-seed median of each seed's own final
population median (min–max across the three seeds), next to Arm B's own
five-seed figures from E44 (quoted):**

| Fire | Arm B med. (range, °, 5-seed) | σ0 med. (range, °, 3-seed) |
|---|---|---|
| Bear | −7.6 (−35.8 to +43.8) | −13.4 (−46.3 to +66.6) |
| Brattain | −12.3 (−29.9 to +46.2) | +27.1 (−1.2 to +87.3) |
| Buck | −11.1 (−17.5 to +46.2) | +66.7 (+63.9 to +71.4) |
| Chimney | −29.3 (−47.6 to −8.6) | −70.3 (−73.1 to −68.7) |
| Ferguson* | +19.3 (−6.4 to +42.2) | −0.8 (−14.0 to +25.4) |
| Pier* | −18.8 (−43.1 to −3.4) | −42.5 (−42.6 to −26.9) |

No consistent pattern: σ0's seed-to-seed range is *wider* than Arm B's
own on Bear and Brattain (a frozen gene, with nothing to correct a bad
population of birth draws, drifts to wherever selection happens to leave
it), *much narrower and internally consistent* on Buck, Chimney and Pier
(three seeds landing within a few degrees of each other, all one sign),
and similar on Ferguson. Three of six medians (Brattain, Buck, Chimney)
also land at a different *sign* from Arm B's own median on that fire.
This is not the signature of one gene quietly tracking a stable
"correct" bearing under either configuration — both arms show real
seed-to-seed disagreement, σ0's just distributed differently.

**Per-window `wind_rot_deg` IQR, three-seed mean at each window, first →
last:**

| Fire | Windows | First | 2nd | Mid | Last | Min | Max |
|---|---|---|---|---|---|---|---|
| Bear | 22 | 96.6° | 92.9° | 56.3° | 23.2° | 23.2° | 96.6° |
| Brattain | 21 | 96.6° | 85.2° | 21.3° | 2.7° | 2.7° | 96.6° |
| Buck | 29 | 96.6° | 97.6° | 34.0° | 1.5° | 1.4° | 97.6° |
| Chimney | 15 | 96.6° | 94.8° | 3.9° | 0.7° | 0.7° | 96.6° |
| Ferguson* | 29 | 96.6° | 82.1° | 0.0° | 0.0° | 0.0° | 96.6° |
| Pier* | 30 | 96.6° | 84.3° | 58.1° | 42.6° | 42.6° | 96.9° |

("First" is the first *scored* window: ≈ 96.6° on every fire, matching
the ±90° uniform birth draw's own theoretical IQR of 90° almost exactly
— the population has not yet been through a resampling step by the time
it is first scored, so this is close to a pure birth-draw spread, not
already narrowed.) **Every fire narrows, substantially, from there, with
mutation frozen the entire rest of the run.** Ferguson and, by the last
window, Brattain, Buck and Chimney collapse to (near) zero spread (2.7°,
1.5°, 0.7° respectively, Ferguson exactly 0.0°) — the surviving
population has been winnowed, by repeated resampling alone, down to
members that all trace back to very similar (or, on Ferguson, the exact
same) birth draws. Bear and Pier narrow the least in absolute terms but
still lose more than two-thirds (Bear) or over half (Pier) of their
starting spread. `SMC_IMMIGRANTS=0.2` is still in effect for this arm
(Arm B does not turn immigrants off, and this ablation only froze
`wind_rot_deg`'s own mutation) — roughly a fifth of each window's
children are fresh immigrants drawn from the full ±90° prior, so this
narrowing is not simply "no new draws are ever introduced"; it is that
selection (`SMC_BETA=10`) prunes those fresh immigrants, and everything
else, fast enough that the survivors' spread still collapses window over
window. This is the standard particle-filter effect known elsewhere in
this ensemble as low effective sample size / degeneracy, here visible in
one gene's own spread rather than in `ess` — **not investigated further
in this batch** (no seed-by-seed ESS/immigrant-survival trace was
pulled); flagged as a mechanism candidate for the "What it means"
section once the ±20° batch is in, not a settled explanation.

**σ0's own prediction clause, checked now (the full prediction is
checked clause by clause in Phase 4, once the ±20° batch is in).**
"Arm B-σ0 within 1 sd of Arm B on ≥ 4 fires" — **holds, exactly at the
bar**: four fires tie (Bear +0.54, Brattain −0.40, Ferguson −0.54, Pier
−0.02 sd, all within 1 sd), not more. Buck loses beyond 2 sd (−2.92) and
Chimney *gains* beyond 1 sd (+1.13) — the prediction did not say
anything about a possible gain, only bounded the *losses* it was willing
to call "diversity wins"; Chimney's gain does not break the clause as
written but is also not something "within 1 sd of Arm B on ≥ 4 fires"
predicted.

**Later.** Not yet revisited.
