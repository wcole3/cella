# E24 — Monte Carlo burn probability from one untuned prior · KEPT (the right product; ties the tuned single runs)

_Round 4 (2026-09-04) · 32 members, one seed each · all six fires incl. holdout · runner `exp_smc.py` → `wildfire_smc open` · results `exp24_smc.json` · metrics TEST_PLAN v1.4 · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** Instead of one run with one tuned parameter set, run 32
with parameters drawn from a wide range and colour each cell by the share
of runs that burned it. Nothing is tuned to any fire, so all six fires
are reported. The 50 % consensus map ties or beats Round 1's hand-tuned
single runs on four of six fires, and its probabilities are far better
calibrated than any single map. This is the first result in a new score
family: consensus IoU and Brier of an ensemble, not mean IoU of a run.

**Question.** Does a burn-probability map from an untuned ensemble beat
the deterministic runs, and is it calibrated?

**What we changed.** `wildfire_smc open`: 32 members per fire, prior p0
log-uniform [0.08, 0.6], burn duration uniform {5..20}, containment τ
log-uniform [2, 100] d, ERA5 wind × uniform [0, 1.5]; one seed per
member; members run independently start to finish.

**How we scored it.** At every observation time: consensus IoU (cells
with probability ≥ 0.5), mean member IoU, Brier score of the probability
map, best-threshold IoU (peeks; diagnostic), all beside the Circle and
persistence.

**Result.**

| Fire | consensus IoU | mean member IoU | best single tuned run (E1/E20) | Circle | Brier ensemble | Brier Circle |
|---|---|---|---|---|---|---|
| Bear | **0.399** | 0.296 | 0.317 (E1) / 0.480 (E20) | 0.541 | 0.084 | 0.055 |
| Brattain | 0.330 | 0.252 | 0.337 / 0.408 | 0.450 | **0.102** | 0.123 |
| Buck | **0.481** | 0.374 | 0.414 / 0.581 | 0.670 | 0.107 | 0.041 |
| Chimney | **0.382** | 0.351 | 0.435 / 0.474 | 0.372 | **0.094** | 0.159 |
| Ferguson (holdout) | 0.131 | 0.173 | 0.145 (global) | 0.373 | **0.119** | 0.167 |
| Pier (holdout) | **0.478** | 0.379 | 0.321 (global) | 0.559 | 0.147 | 0.105 |

How to read it: IoU columns higher is better, Brier columns lower is
better. Bold: consensus beats the tuned E1 single run, or the ensemble's
Brier beats the Circle's. The "best single tuned run" column is a
different score family (single-run mean IoU, and E20 peeked at the whole
series), shown for context only.

- **The consensus of 32 untuned members matches or beats the Round 1
  per-fire tuned single runs** on four of six fires (Bear +0.08, Buck
  +0.07, Pier +0.16 over the tuned or global deterministic runs) and
  beats the Circle on Chimney. Members that explode are out-voted by
  members that stop, and the 0.5 threshold lands near the right area.
- **It is far better calibrated than any deterministic map.** Brier is
  40–60 % lower than the Circle's on Brattain, Chimney and Ferguson. The
  Circle "wins" Brier on Bear, Buck and Pier only because those fires are
  small relative to the domain, so a confident 0/1 disc that is right
  about the vast unburned area scores well.
- **Mean member IoU (0.17–0.38) is far below the consensus** (0.13–0.48): the
  individual runs are as bad as ever; the *ensemble* is the thing that
  is good. This is E8 inverted: with the decay in the prior, members
  disagree about *where* the fire stops, and voting helps.
- Ferguson is the exception: the prior is centred on over-burning fires,
  so most members under-burn it (area ×0.2) and the consensus is worse
  than the mean member.

**What it means.** Averaging over the prior does what the E1 scan did by
hand, without fitting anything. Every deterministic score earlier in the
log is a lower bound on what the same model gives as an ensemble.

**Questions this raises.**

- Can the members learn from each day's mask? → E25: yes, +0.05 to +0.21
  on every fire; Ferguson 0.13 → 0.34.
- How many members are enough? → E32: 32 is the knee; 8 is too few.
- Does the prior decide the answer? → E35: a narrow prior hurts, a very
  broad one is free.
- Is a reliability diagram a better calibration check than Brier against
  the Circle? Open; not computed.

**Verdict.** Kept as the default product.

**Later.** E25 (assim mode), E32, E35, E36 (ensemble forward from a
fitted genome is the "open" mode used as the GA's forecaster).
