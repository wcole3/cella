# E45 — mechanism ablations on the `wind_rot_deg` gene · UNDETERMINED — fire-specific, not one mechanism; optional third batch not run

_Round 7 (2026-09-24 – 2026-09-25) · two batches, one arm per batch, run
one at a time: Arm B-σ0 (`SMC_WIND_ROT_SIGMA=0`, 18 runs), Arm B-20
(`SMC_WIND_ROT_GENE=20`, 18 runs) · a pre-registered optional third
batch (Arm B itself re-run with `SMC_DIAG=1`, 18 runs) was judged not to
change this write-up's answer and was **not run** — see "How we scored
it" and "Later," below · `assim` mode, 32 members, seeds 0–2 (three
seeds — see "Seed count," below), six fires, 2 workers per batch · env
knobs: Arm B-σ0 = `r7_common.ARM_B` + `SMC_WIND_ROT_SIGMA=0`
+ `SMC_DIAG=1`; Arm B-20 = `r7_common.ARM_B` with `SMC_WIND_ROT_GENE=20`
in place of Arm B's own `90` + `SMC_DIAG=1` · runner `exp_r7_e45.py`
(`--arm {sigma0,20,armb_diag} --seeds 3`) → `exp45_wind_rot_mechanism_
sigma0.json`, `exp45_wind_rot_mechanism_20.json` (+ raw reports under
each name's own directory) · compared against Arm B's own five-seed
mean and sd from E44 (`exp44_arm_b_5seed_summary.json`'s `arm_b_sd`
block, `r7_common.arm_b_baseline()`), not E33 · pre-registered TEST_PLAN
v1.9, §9 · σ0 batch: `binary_git 7246387` (clean HEAD, verified in all
18 summary rows and all 18 raw reports), load(1 min) 1.46 at launch →
8.43 at finish, wall time 15012.2 s ≈ 4.17 h at 2 workers · ±20° batch:
`binary_git 0cb2994` (clean HEAD, verified in all 18 summary rows and
all 18 raw reports), load(1 min) 4.62 at launch → 8.02 at finish, wall
time 13190.3 s ≈ 3.66 h at 2 workers (**both wall times: shared box, 2
workers, do not compare against each other or against E44's**) · terms:
[GLOSSARY.md](GLOSSARY.md)_

**In short.** Both batches are in. Consensus IoU: σ0 (diversity kept,
learning removed) ties Arm B within 1 sd on four of six fires (Bear,
Brattain, Ferguson, Pier — exactly the prediction's own bar), gains on
Chimney (+1.13 sd), and loses on Buck (−2.92 sd, its one clear miss).
±20° (learning kept, range narrowed) ties on three (Brattain, Ferguson,
Pier) and loses beyond 2 sd on three (Bear, Buck, Chimney — Chimney's
−2.82 sd is the largest single move in this experiment). The
prediction's own two named fires split one-for-two: Bear's ±20° loss
holds as predicted, but Pier ties rather than losing, and the clause
named neither Buck nor Chimney, the two fires that actually lost most.
The per-window IQR series is the more consequential result: `wind_rot_
deg`'s spread **collapses sharply on every fire under σ0** (birth-level
≈ 97° down to 23–43° on Bear/Pier, under 3° elsewhere) purely through
resampling narrowing which birth draws survive, while under ±20° **it
never collapses at all**, sitting close to that arm's own birth level
(≈ 21°) for the whole run — because mutation, still on, keeps
re-introducing spread fast enough to offset the same degeneracy
pressure. That means an IQR trend cannot, on its own, tell "the filter
learned a bearing" apart from "resampling thinned the birth draws" —
the reason the optional Arm B-diag batch was judged not worth running.
The mechanism itself is **undetermined as one sentence**: Chimney and
Bear make opposite, equally clean cases (Chimney needs a specific
learned value outside ±20°; Bear needs width that σ0 alone does not
remove), so no single "diversity" or "learning" story fits both. Full
detail in Result 1, Result 2 and "What it means," below.

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
task's own batches sets `SMC_DIAG=1`, so both arms this task ran
(Arm B-σ0, Arm B-20) carry the series; TEST_PLAN v1.9's own design asks
for Arm B's own series too, but E44's forecast batch did not set
`SMC_DIAG`, and a third batch to recover it was ultimately ruled out —
see "Seed count and the optional third batch," below, for how that gap
is handled.

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

**Post-hoc note, added after the σ0 batch's own result (before the ±20°
batch ran).** The σ0 result (Result 1, below) found that
`wind_rot_deg`'s per-window IQR narrows sharply on every fire *even with
mutation frozen* — driven by resampling alone thinning which birth
draws survive, not by anything resembling "learning." That finding
changes what an Arm B IQR series (the optional third batch below) could
have told us: if a *frozen* gene's IQR narrows through resampling
degeneracy on its own, then Arm B's own (mutating) IQR narrowing, however
it turned out, could not by itself be read as "the filter is learning
one bearing" either — the same degeneracy would be present in Arm B's
own series and confound the comparison the same way. Read together with
the ±20° result (Result 2, below — mutation *on*, IQR stays flat rather
than collapsing), the IQR trend by itself cannot cleanly separate
"diversity" from "learning" in this design, with or without a third
reference series from Arm B. The controller ruled the optional batch out
on those grounds; see "Seed count (pre-registered branch) and the
optional third batch" and "Later," below, for the ruling and its cost.

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
third, optional, 18-run batch, launched only if the σ0 and ±20° batches
above finish within this task's time budget.

**Both batches finished within budget** (σ0: 15012.2 s ≈ 4.17 h; ±20°:
13190.3 s ≈ 3.66 h — both at 2 workers), so the optional batch's budget
condition was met. It was **not run anyway**, on the controller's own
ruling after the σ0 result and the post-hoc note above: the σ0 batch
already showed that a frozen gene's IQR narrows through resampling
degeneracy alone, which means an Arm B IQR series could not have
discriminated "diversity" from "learning" either — it would sit
somewhere between σ0's collapsing series and ±20°'s flat one without
being able to say which effect (mutation's own re-diversifying pull, or
degeneracy despite it) dominates Arm B's own number, so it would not
have changed this write-up's verdict. **Arm B's own per-window
`wind_rot_deg` IQR series is therefore unavailable and is reported as
such**, not estimated or inferred from the two ablation arms. The batch
this write-up did not run would have cost 18 runs, ≈ 4 h at 2 workers
(by analogy to the two batches actually run, both close to that
figure) — recorded here so the trade-off is visible, not just the
decision. `exp45_arm_b_diag.json` was never created; this batch does
not touch, overwrite or reinterpret E44's own
`exp44_arm_b_5seed.json`/`_summary.json` either way.

**Prediction, written before the run (TEST_PLAN v1.9, §9, quoted
verbatim).** "Diversity wins — Arm B-σ0 within 1 sd of Arm B on ≥ 4
fires; Arm B-20 loses to Arm B beyond 1 sd on Bear and Pier (Arm B's own
learned medians there are ±43°)." Checked clause by clause below, in the
"Prediction checked clause by clause" section after both Results.

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
pulled); taken up as a mechanism candidate in "What it means," below,
once Result 2's own IQR series (no collapse, mutation on) gives it a
contrast to be read against — not a settled explanation on its own.

**σ0's own prediction clause.** "Arm B-σ0 within 1 sd of Arm B on ≥ 4
fires" — **holds, exactly at the bar**: four fires tie (Bear +0.54,
Brattain −0.40, Ferguson −0.54, Pier −0.02 sd, all within 1 sd), not
more. Buck loses beyond 2 sd (−2.92) and Chimney *gains* beyond 1 sd
(+1.13) — the prediction did not say anything about a possible gain,
only bounded the *losses* it was willing to call "diversity wins";
Chimney's gain does not break the clause as written but is also not
something "within 1 sd of Arm B on ≥ 4 fires" predicted.

## Result 2 — Arm B-20 (three-seed forecast)

Provenance: all 18 summary rows (`exp45_wind_rot_mechanism_20.json`) and
all 18 raw reports (`exp45_wind_rot_mechanism_20/*.json`) carry
`binary_git 0cb2994`, matching the clean HEAD this batch was built and
launched from (a later commit than the σ0 batch's `7246387` — the
Result 1 write-up commit in between moved `binary_git`, same pattern
E44's own two-batch write-up used). Batch: 18 jobs (3 seeds × 6 fires),
2 workers, load(1 min) 4.62 → 8.02, wall time 13190.3 s ≈ 3.66 h — both
figures **shared box, this run's own numbers only, not compared against
the σ0 batch's or E44's**.

Mean one-window-ahead consensus IoU, three seeds, against Arm B's own
five-seed mean and sd from E44:

| Fire | Arm B mean (5-seed) | Arm B sd (5-seed) | ±20° mean (3-seed) | ±20° sd (3-seed) | Delta ±20° (Arm B-sd) | verdict ±20° |
|---|---|---|---|---|---|---|
| Bear | 0.473 | 0.005 | 0.461 | 0.006 | −0.012 (−2.24 sd) | **beyond 2 sd (loss)** |
| Brattain | 0.437 | 0.034 | 0.417 | 0.023 | −0.020 (−0.59 sd) | tie |
| Buck | 0.640 | 0.005 | 0.626 | 0.008 | −0.014 (−2.62 sd) | **beyond 2 sd (loss)** |
| Chimney | 0.489 | 0.031 | 0.402 | 0.007 | −0.087 (−2.82 sd) | **beyond 2 sd (loss)** |
| Ferguson* | 0.386 | 0.014 | 0.399 | 0.025 | +0.013 (+0.95 sd) | tie |
| Pier* | 0.521 | 0.023 | 0.526 | 0.005 | +0.005 (+0.24 sd) | tie |

![Six per-fire bar triples, Arm B's own five-seed mean against the sigma0 (diversity, no learning) and +/-20 degree (learning, narrow range) ablations, each three-seed, against a shaded band one Arm B sd wide — Chimney and Bear show the two ablations pulling in opposite directions.](figures/e45-ablations.svg)

Table generated by `r7_common.summary_table()`/`fire_stats()`, same
method as Result 1. `*` = holdout pair. **Three fires lose beyond 2 sd
(Bear, Buck, Chimney), not the two the prediction named** — Chimney's
loss (−2.82 sd, −0.087 absolute) is the single largest movement, in
either direction, anywhere in this experiment.

Brier (ensemble mean) and final contained fraction (three-seed
min–max), next to Arm B's own five-seed Brier from E44 (quoted):

| Fire | Brier Arm B (5-seed) | Brier ±20° (3-seed mean, sd) | Contained ±20° (3-seed min–max) |
|---|---|---|---|
| Bear | 0.0507 | 0.0523 (0.0002) | 0.938–1.000 |
| Brattain | 0.1020 | 0.1093 (0.0079) | 1.000–1.000 |
| Buck | 0.0396 | 0.0407 (0.0013) | 1.000–1.000 |
| Chimney | 0.0947 | 0.1345 (0.0059) | 0.719–1.000 |
| Ferguson* | 0.1356 | 0.1311 (0.0035) | 0.969–1.000 |
| Pier* | 0.1074 | 0.1085 (0.0012) | 1.000–1.000 |

Brier moves with consensus IoU on every fire (Chimney worst on both,
Ferguson the one fire that improves on both — a small Brier gain
alongside its tied IoU). Chimney's contained fraction (0.719–1.000)
is the widest spread of either batch's own worst fire.

**Learned `wind_rot_deg`, three-seed median of each seed's own final
population median (min–max), next to Arm B's own five-seed figures from
E44 (quoted) — every ±20° value is, by construction, inside [−20°,
+20°]:**

| Fire | Arm B med. (range, °, 5-seed) | ±20° med. (range, °, 3-seed) |
|---|---|---|
| Bear | −7.6 (−35.8 to +43.8) | +2.1 (−2.5 to +4.6) |
| Brattain | −12.3 (−29.9 to +46.2) | +0.6 (−5.2 to +5.1) |
| Buck | −11.1 (−17.5 to +46.2) | −1.7 (−8.6 to +3.8) |
| Chimney | −29.3 (−47.6 to −8.6) | −7.8 (−10.0 to +8.1) |
| Ferguson* | +19.3 (−6.4 to +42.2) | +1.3 (−2.0 to +6.0) |
| Pier* | −18.8 (−43.1 to −3.4) | −6.7 (−9.2 to +6.1) |

**Chimney is the one fire where Arm B's own five-seed median (−29.3°)
sits entirely outside the ±20° range this arm allows** — the ±20°
ensemble cannot represent Arm B's own preferred correction at all, only
approximate its sign; every other fire's Arm B median sits inside
±20° already (Pier's, at −18.8°, is the closest to the edge). This
matches Chimney being both the arm's worst loss and the fire where
Arm B's own posterior most clearly needed room this arm does not have.

**Per-window `wind_rot_deg` IQR, three-seed mean at each window, first →
last:**

| Fire | Windows | First | 2nd | Mid | Last | Min | Max |
|---|---|---|---|---|---|---|---|
| Bear | 22 | 21.5° | 19.5° | 21.6° | 20.7° | 19.1° | 26.1° |
| Brattain | 21 | 21.5° | 18.4° | 18.9° | 19.6° | 15.7° | 25.3° |
| Buck | 29 | 21.5° | 19.2° | 19.4° | 21.1° | 19.2° | 25.8° |
| Chimney | 15 | 21.5° | 18.7° | 21.1° | 17.8° | 14.9° | 22.0° |
| Ferguson* | 29 | 21.5° | 17.9° | 20.8° | 22.2° | 16.7° | 25.0° |
| Pier* | 30 | 21.5° | 20.0° | 21.0° | 21.2° | 18.3° | 24.4° |

**No collapse anywhere — the IQR stays close to the birth-draw level
(≈ 21.5° at the first scored window, matching Uniform(−20, 20)'s own
theoretical IQR of 20° almost exactly) for the entire run, on every
fire**, fluctuating in a narrow band (roughly 15–26°) rather than
trending down. This is the plainest possible contrast with Result 1's
σ0 series (which collapsed toward 0–43° from the same ≈ 97° starting
point): with mutation still on (`SMC_WIND_ROT_SIGMA` unset for this
arm), the Gaussian nudge every generation applies keeps re-introducing
spread fast enough to offset whatever resampling alone would have
collapsed — the same degeneracy pressure Result 1 diagnosed is
presumably still acting here (nothing in this design turns it off), it
is just being counteracted continuously rather than winning by the end
of the run.

**±20°'s own prediction clause.** "Arm B-20 loses to Arm B beyond 1 sd
on Bear and Pier" — **holds for Bear** (beyond 2 sd, −2.24), **fails for
Pier** (+0.24 sd, a tie — not a loss in either direction, let alone
beyond 1 sd). The clause also did not predict Buck (−2.62 sd) or
Chimney (−2.82 sd, the batch's worst), both real losses beyond 2 sd it
said nothing about. As written, this clause is **half right**: it
correctly flagged Bear as vulnerable to narrowing, wrongly flagged Pier,
and missed the two fires (Buck, Chimney) that actually lost the most.

## Prediction checked clause by clause (both arms, TEST_PLAN v1.9 §9, quoted verbatim)

"Diversity wins — Arm B-σ0 within 1 sd of Arm B on ≥ 4 fires" —
**holds**, exactly at the bar (4 of 6: Bear, Brattain, Ferguson, Pier).

"Arm B-20 loses to Arm B beyond 1 sd on Bear and Pier (Arm B's own
learned medians there are ±43°)" — **holds for Bear, fails for Pier**.
Pier ties (+0.24 sd); the two fires that *do* lose beyond 1 sd beyond
Bear (Buck −2.62 sd, Chimney −2.82 sd) are not named in the clause at
all. Read plainly, the prediction's headline claim — "diversity wins" —
is not what either arm's own record shows cleanly: σ0 (diversity kept,
learning removed) is the *milder* miss (one clear loss, Buck, against
four ties and a gain), while ±20° (learning kept, diversity narrowed)
is the *larger* miss (three losses beyond 2 sd against three ties) —
which on its face reads as diversity mattering *more* than the specific
value learned, consistent with "diversity wins" in aggregate even
though the clause's own two named fires split one-for-two.

**The mechanism, in one sentence, or why it cannot be one.**
**Undetermined as a single sentence — the evidence splits by fire, not
by one universal explanation.** Two fires make opposite, equally clean
cases: on **Chimney**, σ0 (diversity, no learning) *gains* (+1.13 sd)
while ±20° (learning, narrow range) *loses the worst of anywhere in this
experiment* (−2.82 sd) — and Arm B's own learned median there (−29.3°)
sits entirely outside the ±20° window, so this fire's story is "the
filter needs a large, specific, learned correction that a narrow range
cannot represent," a *learning* story. On **Bear**, the pattern
inverts: σ0 ties (+0.54 sd, no real cost to freezing mutation) while
±20° loses beyond 2 sd (−2.24 sd) — even though Arm B's own median
there (−7.6°) sits comfortably inside ±20°, so narrowing the range
should not have mattered if only the *median* were doing the work; this
fire's story is "a wide simultaneous spread across members, not a
specific value, is doing the work," a *diversity* story. **Buck** loses
under both ablations (σ0 −2.92 sd, ±20° −2.62 sd), suggesting it needs
both; **Brattain, Ferguson and Pier** tie under both, suggesting the
gene is not doing much heavy lifting on those three either way (Pier in
particular already lost to E33 under full Arm B in E44 — this gene was
plausibly never the fix there). No one mechanism (pure diversity, or
pure learning) accounts for Bear and Chimney simultaneously with the
same story, so a single one-sentence mechanism would misdescribe at
least one of them.

**What it means.** The gene's contribution is fire-specific, not one
thing. Where it matters most (Chimney), the *value* being outside a
narrow range is what breaks the ±20° arm, while the *frozen* arm is
unaffected — a straightforward "needs a correction this large" story.
Where it matters on a different fire (Bear), the value stays inside the
narrow range yet the narrow arm still loses while the frozen arm does
not — the more surprising result, and the one that argues for something
beyond "the filter converges on the right number": a wide simultaneous
spread across members, present at birth and only slowly eroded by
resampling under σ0 (Result 1), evidently helps Bear's forecast on its
own, independent of where the population's *median* ends up. The IQR
series adds a caveat that cuts across both stories: because resampling
alone narrows a *frozen* gene's spread substantially (Result 1), a
narrowing IQR under a *mutating* configuration (Arm B itself, or ±20°)
cannot be read as proof of "learning" without controlling for that
degeneracy — which is exactly why the optional Arm B-diag batch was
judged not to add discriminating power (see "How we scored it," above)
and was not run.

**Questions this raises.**

- Would giving the ±20° arm's population *more* mutation pressure (a
  larger `SMC_SIGMA` for this one run, or a per-gene sigma override in
  the other direction — larger than the engine default rather than
  frozen at zero) recover Chimney by letting the population drift its
  values faster within the narrow range, or is ±20° simply too narrow
  regardless of how fast it is searched, because ±20° cannot reach a
  −29° correction no matter how much it mutates within its own bounds?
  This experiment cannot distinguish "too narrow" from "too slow" and
  did not test a wider-sigma, narrow-range arm. Open.
- Is Bear's diversity-looking result actually about sub-daily or
  window-to-window wind variability that a single per-member value
  cannot track (the Angular diversity story in the glossary), or could
  it be an artifact of Bear's own gene interactions (`model.p0`,
  `wind_scale`) shifting to compensate differently under each ablation,
  not tested here? A `SMC_DIAG=1` read of Bear's other per-window gene
  medians (already collected in this same batch's raw reports, not
  analysed in this write-up) would be a cheap next step. Open.
- Buck loses under both ablations by a similar margin (σ0 −2.92 sd,
  ±20° −2.62 sd) — is that because Buck genuinely needs *both* wide
  diversity and continued learning, or because both ablations are
  independently sufficient to disrupt whatever narrower thing Buck
  actually needs (e.g. a particular window's correction, not a
  steady-state one)? Not distinguishable from two single-knob
  ablations alone; a factorial design (both restrictions at once, or
  neither) was outside this task's scope. Open.
- The degeneracy this experiment found in σ0 (resampling alone narrows
  a frozen gene's spread, on some fires to exactly 0) was read off the
  IQR series after the fact, not predicted or investigated at the level
  of individual member ancestry or `ess`. Does it happen at the same
  rate to other genes with no mutation-freeze at all (e.g. would a
  σ0-style freeze on `model.p0` show the same collapse, confirming this
  is a general property of this ensemble's selection pressure rather
  than something specific to `wind_rot_deg`)? Open, would need a
  separate ablation this task did not run.

**Verdict.** **Fire-specific, not one mechanism — the prediction's
"diversity wins" headline is directionally supported in aggregate (σ0's
one clear miss, Buck, against ±20°'s three) but its own two named fires
split one-for-two (Bear holds, Pier fails), and it named neither of the
two fires (Buck, Chimney) that actually lost the most under narrowing.**
σ0 (diversity kept, learning removed): four of six fires tie Arm B
within 1 sd (Bear, Brattain, Ferguson, Pier — exactly clearing the
prediction's own bar), Chimney gains beyond 1 sd, Buck is the one clear
loss (beyond 2 sd). ±20° (learning kept, range narrowed): three fires
tie (Brattain, Ferguson, Pier), three lose beyond 2 sd (Bear, Buck,
Chimney — Chimney's the largest single movement in this experiment).
The per-window IQR series shows mutation being switched off (σ0)
collapses the population's spread through resampling alone, on every
fire, while a narrowed range under continued mutation (±20°) does not
collapse at all — so an IQR trend, on its own and without a
degeneracy-free reference, cannot separate "the filter learned a
bearing" from "resampling thinned the birth draws," a caveat this
write-up applies to its own σ0 series as much as to any hypothetical
Arm B series. The optional third batch (Arm B itself under
`SMC_DIAG=1`) was judged, after the σ0 result, not to resolve that
caveat and was not run — an 18-run, ≈ 4 h cost avoided for a series
that would not have changed this verdict. Per TEST_PLAN v1.9: this
experiment has no stop rule and both arms are reported regardless of
outcome, as done here.

**Later.** The optional Arm B-diag batch (`SMC_DIAG=1` on Arm B
unchanged, seeds 0–2, six fires, 18 runs, ≈ 4 h at 2 workers by analogy
to the two batches actually run) was not run — ruled out after the σ0
result showed resampling alone narrows a frozen gene's IQR, which means
an Arm B series could not have discriminated diversity from learning
either (see "How we scored it" and the Verdict, above). If a later
experiment finds a way to measure degeneracy independent of mutation
state (e.g. tracking distinct ancestor count per window, not just
value spread), revisiting Arm B's own IQR series under that better
instrument — rather than under `wind_rot_deg_iqr` alone — would be
better spent than simply running the batch this task skipped. The
"Questions this raises" items above (wider-sigma narrow-range arm for
Chimney; Bear's other gene medians; a factorial Buck design; whether
the degeneracy is `wind_rot_deg`-specific) are otherwise open.
