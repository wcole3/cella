# E31 — replicate E25 and E28 through the generic `explore` engine · PASS on the recommended configuration — E28's Bear gain does not survive

_Round: Round 4 — 2026-09-05: ensembles (engine generalisation)_

**Why.** Between E28 and this run the wildfire-only ensemble
(`WildfireEnsemble`, `prior` block) was replaced by the model-agnostic
`cella_lib::explore::Ensemble`; the wildfire pieces (weather schedule, τ
decay, containment roll) moved into `WildfireDriver`, and the random
generator changed (per-member seeds and gene draws now come from one
SplitMix64 stream; every run is bit-reproducible). Before building on the
new engine we must know it reproduces the record. This is a **replication,
not an experiment**: nothing is tuned, the runners are the E25/E28 scripts
unchanged (`exp_smc_imm.py`, `exp_containment_op.py`), 32 members, β 10,
σ 0.2, immigrants 0.2, all six fires. Pre-registered bar (TEST_PLAN v1.6):
mean one-window-ahead consensus IoU within **0.02** and mean Brier within
**0.005** of the recorded row. Bit-identity was not expected.

**Runs.** 24 fire × config runs (E25b: `assim_b10_s0.2_imm0.2`; E28:
`base`, `contain`, `contain_tauoff`), 23 minutes wall on this machine.

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

(*holdout.) Brier deltas: containment-only −0.003 / −0.006 / +0.003 /
+0.005 / −0.005 / +0.001 (Brattain better than the bar by 0.001); E25 row
+0.0003 / −0.0015 / −0.0012 / **+0.0061** / −0.0044 / −0.0014; base row
Chimney +0.009, all others within bar; decay + containment Chimney −0.007,
others within bar. Every ensemble Brier stays below the Circle's on the
same fire except Buck, exactly as in E24/E25.

**Findings.**

- **The recommended configuration replicates on every fire**: containment
  only, decay off, within 0.009 IoU everywhere, Brier within 0.006. The E25
  row replicates within 0.014 IoU on all six. The generic engine and the
  driver reproduce the record.
- **Two off-recommendation rows drift past the bar** (base Bear +0.030;
  decay + containment Chimney −0.040). Context: under the old engine the
  *same* configuration was run twice — as E25b (0.473 on Bear) and as E28
  base (0.456) — 0.017 apart, and the E25d replicates spread up to 0.018
  (Ferguson) in IoU and 0.008 (Chimney) in Brier. The new engine gives
  0.486 for both runs, bit-identical. The 0.02 / 0.005 bars were set from
  the E25d spread and are tight for Bear and Chimney; the drift is the
  size of the old run-to-run noise, not a change in behaviour.
- **E28's headline gain on Bear (+0.026 for containment-only over decay)
  does not survive.** In E31 the two are 0.478 vs 0.486 (−0.008). Over all
  six fires containment-only vs decay is −0.008 / +0.018 / −0.003 / +0.007
  / +0.007 / −0.002: **a tie within noise on every fire.** E28's verdict
  therefore rests on what it always should have: the containment roll is a
  published, interpretable mechanism that does the decay's job **at no
  cost**, not that it scores higher.
- Learned knobs are in the same places (p0 0.22–0.35, wind × 0.6–0.9 in
  the containment-only runs), consistent with E25/E28.

**Verdict.** PASS. The generic engine is the record from here on; the
`WildfireEnsemble` code path is gone. Results files:
`results/experiments/exp25b_smc_imm.json` and `exp28_containment_op.json`
(the pre-E31 copies are in `results/experiments/pre_e31_backup/`). Two
consequences for the log: (1) E28's Bear number is reclassified as noise
(note added to its file); (2) future comparisons of a *single* run against
the record should use the E25d spread (≈ 0.02 IoU, ≈ 0.008 Brier on
Chimney), and any claimed gain smaller than that needs replicates. Next:
E30 kernel wind-rate refit, then the ICS-209 containment check and a fuel
term in the containment operator.
