# E38 — immigrants start fresh · KEPT as an option — repairs the lock-in E33 found; a tie elsewhere, with a small Brier cost

_Round 5 (2026-09-05, after E33) · 5 seeds matched to E33 · all six fires incl. holdout · runner `exp_r5_immreset.py` (`SMC_IMM_RESET=1`) · results `exp38_immreset.json` · pre-registered TEST_PLAN v1.7 addendum · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** E33's one outlier (Buck seed 3, 0.527 against 0.585–0.628 for
the other seeds) was a population in which every member had stopped by
day 12 while the real fire kept growing, and E34 showed every one of its 54
runs ends fully stopped. Immigrants get fresh knobs but inherit their parent's
state, stopped flag included, so once all members have stopped nothing
can start again. The fix is one option: an immigrant starts with a fresh
state, uncontained. Prediction, written before the run: a large gain on
the locked Buck seed, ties elsewhere. It held: +0.105 on that seed,
Buck's seed-to-seed spread halved, ties on four fires, a small loss on
Pier and a small Brier cost where the fire really had stopped.

**Question.** Can a fully contained population recover if immigrants
start fresh?

**What we ran.** The base configuration with `immigrant_reset` on, seeds
0–4 matched to E33's, all six fires; each seed compared with its own E33
twin.

**How we scored it.** Per-seed change in mean one-window-ahead consensus
IoU against the E33 twin; mean ± sd with and without the reset; Brier;
share of members still contained at the end.

![Dot strip per fire: change in mean forecast IoU per matched seed when immigrants start fresh](figures/e38-immreset.svg)

**Result.**

| Fire | seed 0 | seed 1 | seed 2 | seed 3 | seed 4 | mean Δ | mean ± sd, reset | mean ± sd, E33 |
|---|---|---|---|---|---|---|---|---|
| Bear | +0.001 | +0.018 | −0.030 | +0.042 | −0.006 | +0.005 | 0.484 ± 0.014 | 0.479 ± 0.015 |
| Brattain | +0.005 | +0.001 | −0.012 | 0.000 | −0.007 | −0.003 | 0.413 ± 0.006 | 0.416 ± 0.004 |
| Buck | −0.026 | +0.021 | −0.006 | **+0.105** | −0.018 | +0.015 | 0.606 ± **0.021** | 0.590 ± 0.039 |
| Chimney | −0.011 | −0.003 | +0.004 | +0.008 | +0.018 | +0.003 | 0.437 ± 0.003 | 0.434 ± 0.012 |
| Ferguson* | −0.010 | +0.001 | −0.022 | +0.010 | −0.006 | −0.005 | 0.338 ± 0.009 | 0.344 ± 0.007 |
| Pier* | −0.015 | −0.006 | −0.014 | −0.008 | +0.004 | **−0.008** | 0.527 ± 0.007 | 0.535 ± 0.003 |

How to read it: each seed cell is (reset − E33) for that seed; positive is
better; then the mean change and the five-seed mean ± sd with and without
the reset. Bold marks the predicted gain, the halved spread, and the one
mean loss beyond its sd. `*` is the holdout pair. Brier, reset vs E33
mean: Bear 0.0513/0.0510, Brattain 0.1122/0.1090, Buck 0.0448/0.0468,
Chimney 0.1315/0.1276, Ferguson 0.1369/0.1380, Pier 0.1157/0.1093.
Members still contained at the end: 58–100 % with reset, 100 % without.

- **The prediction held.** Buck seed 3 goes from 0.527 to 0.632 (+0.105):
  its trace, identical to the E33 twin for ten days, climbs to 0.69 from
  day 8 instead of sinking to 0.42. Buck's seed-to-seed sd halves (0.039
  → 0.021): the lock-in was the outlier, and it is gone.
- **Everywhere else the means tie.** Bear +0.005, Brattain −0.003,
  Chimney +0.003, Ferguson −0.005 are inside their sd. But the per-seed
  deltas on Bear (−0.030 … +0.042) are wider than E33's own spread: the
  reset does not remove the containment dice, it re-rolls them.
- **Pier pays a little** (−0.008, worse on four of five seeds, sd 0.003)
  and Brier is worse by 0.003–0.006 on four fires. Fresh immigrants late
  in a fire that has genuinely stopped spread probability where nothing
  will burn. Pier is the fire the containment operator describes best,
  so re-igniting members there is pure noise.
- Chimney ends with only 58 % of members contained under reset (100 %
  without) and its sd falls to 0.003: on the fast fire the live members
  are the ones tracking the truth.

**What it means.** The reset fixes the one failure mode the filter had no
answer to and costs about one sd of Brier where the fire really has
stopped. The right version is gated on evidence: reset immigrants only
while the consensus *under-predicts* the observed area (area ratio < 1 at
the last assimilation), which is exactly the signal that the population
has stopped and the fire has not.

**Questions this raises.**

- Does an area-ratio-gated reset keep the Buck gain without the Pier
  cost? Open: **E39**, one line in the runner plus a config field.

**Verdict.** Kept as an option, default off. Until E39: switch the reset
on for fires still growing after their first week, off for fires that
have plateaued.

**Later.** E39 (not yet run).
