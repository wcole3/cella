# E31 — replicate E25 and E28 through the generic `explore` engine · PASS on the recommended configuration — E28's Bear gain does not survive

_Round 4 (2026-09-05) · 32 members, one seed · all six fires incl. holdout · runners `exp_smc_imm.py`, `exp_containment_op.py` unchanged · results `exp25b_smc_imm.json`, `exp28_containment_op.json` (pre-E31 copies in `pre_e31_backup/`) · bar TEST_PLAN v1.6 · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** Between E28 and this run the wildfire-only ensemble code
was replaced by a model-agnostic engine, and the random-number generator
changed. Before building on the new engine we re-ran the recorded E25 and
E28 configurations through it, unchanged, and checked the numbers came
back. The recommended configuration replicates on every fire within
0.009 IoU. Two off-recommendation rows drift by the size of the old
run-to-run noise. And E28's headline gain on Bear disappears: the
containment operator and the decay are a tie within noise on every fire.
This is a replication, not an experiment.

**Question.** Does the new engine reproduce the record?

**What changed in the code.** `WildfireEnsemble` and its `prior` block
became `cella_lib::explore::Ensemble`; the wildfire pieces (weather
schedule, τ decay, containment roll) moved into `WildfireDriver`; member
seeds and gene draws now come from one SplitMix64 stream, so every run is
bit-reproducible. Nothing tuned: 32 members, β 10, σ 0.2, immigrants 0.2, all
six fires; 24 fire × config runs, 23 minutes wall.

**How we scored it.** Mean one-window-ahead consensus IoU and Brier
against the recorded rows. Pre-registered bar: IoU within **0.02**, Brier
within **0.005**. Bit-identity was not expected.

**Result.**

| config | | Bear | Brattain | Buck | Chimney | Ferguson* | Pier* |
|---|---|---|---|---|---|---|---|
| E25 assim + immigrants | record | 0.473 | 0.400 | 0.616 | 0.426 | 0.345 | 0.533 |
| | **E31** | 0.486 | 0.397 | 0.603 | 0.439 | 0.346 | 0.537 |
| | Δ | +0.014 | −0.003 | −0.014 | +0.013 | +0.001 | +0.004 |
| E28 base (decay only) | record | 0.456 | 0.403 | 0.602 | 0.431 | 0.338 | 0.537 |
| | **E31** | 0.486 | 0.397 | 0.603 | 0.439 | 0.346 | 0.537 |
| | Δ | **+0.030** | −0.006 | +0.001 | +0.008 | +0.008 | 0.000 |
| E28 decay + containment | record | 0.462 | 0.409 | 0.601 | 0.438 | 0.336 | 0.534 |
| | **E31** | 0.495 | 0.414 | 0.612 | 0.398 | 0.342 | 0.534 |
| | Δ | **+0.033** | +0.005 | +0.011 | **−0.040** | +0.006 | 0.000 |
| **E28 containment only (recommended)** | record | 0.482 | 0.413 | 0.599 | 0.438 | 0.344 | 0.529 |
| | **E31** | 0.478 | 0.415 | 0.600 | 0.446 | 0.353 | 0.535 |
| | Δ | −0.004 | +0.002 | 0.000 | +0.009 | +0.009 | +0.006 |
| Circle | | 0.541 | 0.450 | 0.670 | 0.372 | 0.373 | 0.559 |

How to read it: for each configuration, the recorded row, the new
engine's row, and their difference. Bold Δ is past the 0.02 bar. `*` is
the holdout pair. Brier deltas: containment-only −0.003 / −0.006 / +0.003
/ +0.005 / −0.005 / +0.001 (Brattain over the bar by 0.001); E25 row
+0.0003 / −0.0015 / −0.0012 / **+0.0061** / −0.0044 / −0.0014; base row
Chimney +0.009, others within bar; decay + containment Chimney −0.007,
others within bar. Every ensemble Brier stays below the Circle's except
on Buck, as in E24/E25.

- **The recommended configuration replicates on every fire**: within
  0.009 IoU, Brier within 0.006. The E25 row replicates within 0.014.
- **Two off-recommendation rows drift past the bar** (base Bear +0.030;
  decay + containment Chimney −0.040). Under the old engine the *same*
  configuration had been run twice, as E25b (0.473 on Bear) and as E28
  base (0.456), 0.017 apart, and the E25d replicates spread up to 0.018.
  The new engine gives 0.486 for both, bit-identical. The drift is the
  size of the old run-to-run noise, not a change in behaviour.
- **E28's headline gain on Bear (+0.026) does not survive.** In E31 the
  two are 0.478 vs 0.486 (−0.008). Over all six fires containment-only
  vs decay is −0.008 / +0.018 / −0.003 / +0.007 / +0.007 / −0.002: **a
  tie within noise on every fire.**
- Learned knobs are in the same places (p0 0.22–0.35, wind × 0.6–0.9).

**What it means.** The generic engine is the record from here on. Two
consequences for the log: E28's Bear number is reclassified as noise
(note added to its file); and any future gain smaller than the
run-to-run spread needs replicates before it is believed. The 0.02 /
0.005 bars borrowed from three replicates were about right but tight.

**Questions this raises.**

- What is the real noise floor, per fire, from enough seeds? → E33: five
  seeds; sd ≤ 0.015 on five fires, 0.039 on Buck.

**Verdict.** PASS.

**Later.** E33 (noise floor), Round 5 (every experiment judged against
it). Next in the plan at the time: E30 kernel wind-rate refit, then the
ICS-209 containment check and a fuel term in the containment operator.
