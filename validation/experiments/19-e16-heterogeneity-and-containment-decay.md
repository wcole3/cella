# E16 — two non-weather ways to stop: heterogeneity (a) and a containment decay (b) · (a) REJECTED, (b) KEPT — biggest gain so far

_Round 3 (2026-09-02) · 3 seeds · calibration fires · runner `exp_stopping.py` · results `exp16_stopping.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** Weather could not cap the burn (E3, E13, E15). Two ideas
from the literature that are not weather: (a) make the fuel patchy, so
the fire has a harder time percolating; (b) make p0 shrink steadily over
days, standing in for firefighters gaining ground. Patchiness did nothing.
The decay is the largest single gain in the log: +0.12 on Bear, +0.07 on
Brattain, +0.14 on Buck, and for the first time the model burns about the
right *amount*. It is a fitted knob, not physics: it says when the fire
stops, not where, so the Circle still wins on three of four fires.

**Question.** Can something other than weather make the model stop at
the right size?

**What we changed.**

- **(a) Spatial heterogeneity.** Per-cell density multiplier drawn from
  a lognormal with mean exactly 1 (σ ∈ {0.3, 0.6, 1.0}, seeded), p0
  ×{1, 1.5, 2}. Percolation theory says a patchy medium has a softer
  threshold than a uniform one, and real fuel beds are patchy at 30 m.
- **(b) A containment decay.** p0 × exp(−t/τ), τ ∈ {5, 10, 20} days, p0
  ×{1, 1.5, 2} so the early fire is not starved. Every observed area
  curve grows then plateaus (ANALYSIS §5), and the literature attributes
  the plateau to suppression, which this model does not have.

E1 recipe otherwise.

**How we scored it.** Mean IoU with area ratio in brackets, 3 seeds, four
calibration fires; also final-day IoU and arrival error for the best
decay run.

**Result.**

| variant | Bear | Brattain | Buck | Chimney |
|---|---|---|---|---|
| ctrl | 0.311 (×3.4) | 0.336 (×3.7) | 0.403 (×0.6) | 0.441 (×2.1) |
| hetero σ0.3 ×1 | 0.314 (×3.2) | 0.335 (×3.6) | 0.388 (×0.4) | 0.444 (×2.1) |
| hetero σ0.6 ×1 | 0.302 (×2.7) | 0.333 (×3.4) | 0.374 (×0.4) | 0.446 (×1.8) |
| hetero σ1.0 ×1.5 | 0.275 (×3.5) | 0.331 (×3.9) | 0.380 (×0.4) | 0.442 (×2.3) |
| contain τ5 ×1 | 0.265 (×0.2) | 0.215 (×0.2) | 0.411 (×0.3) | 0.351 (×0.4) |
| contain τ5 ×1.5 | 0.391 (×0.4) | 0.364 (×0.5) | 0.491 (×0.6) | 0.393 (×0.7) |
| **contain τ5 ×2** | **0.427 (×0.7)** | **0.407 (×0.8)** | **0.541 (×1.0)** | 0.393 (×1.0) |
| contain τ10 ×1.5 | 0.385 (×1.1) | 0.377 (×1.7) | 0.504 (×0.8) | 0.431 (×1.5) |
| contain τ10 ×2 | 0.343 (×2.0) | 0.352 (×2.6) | 0.468 (×1.4) | 0.441 (×2.1) |
| contain τ20 ×1.5 | 0.323 (×2.8) | 0.350 (×3.3) | 0.470 (×1.0) | **0.444 (×2.3)** |
| Circle | 0.541 | 0.450 | 0.670 | 0.372 |

How to read it: mean IoU with the area ratio in brackets; "×2" in a
variant name is the p0 multiplier. Bold is the best row per fire.
Heterogeneity rows at other multipliers were all worse and are in the
results file.

Final-day IoU and arrival error, control → best containment run:

| Fire | final IoU | arrival MAE |
|---|---|---|
| Bear | 0.144 → **0.405** | 62 h → 58 h |
| Brattain | 0.223 → **0.386** | 113 h → 41 h |
| Buck | 0.345 → **0.510** | 57 h → 82 h |
| Chimney | 0.409 → 0.395 | 54 h → 37 h |

- **(a) Heterogeneity: nothing.** At matched mean p0 it is within ±0.03
  of control on every fire; pushing p0 up to compensate the lost
  connectivity brings the explosion straight back.
- **(b) The decay is the largest single improvement in the log.** One
  setting (τ = 5 days, p0 ×2) lifts mean IoU by +0.12 Bear, +0.07
  Brattain, +0.14 Buck, and brings the final burned area to ×0.7–1.0
  instead of ×3–4. Final-day IoU nearly triples on Bear and Brattain.
- Chimney, the fastest fire, wants a slower decay (τ = 20) and gains
  nothing: it was already the fire the model handles.

**What it means.** The decay works where weather did not because it is
monotone: once p0 falls under the percolation threshold the front freezes
for good. A periodic modulation never does that. What the decay is *not*
is physics. τ and the ×2 are fitted on the calibration fires, and
exp(−t/τ) says nothing about *where* the fire stops, only *when*. The gap
to the Circle is halved, not closed.

**Questions this raises.**

- Does one global decay setting hold on the holdout fires? → E16c: Pier
  yes (+0.14), Ferguson no (the model under-burns it, and a decay makes
  under-burning worse).
- Is the decay really suppression? → E21: no. Real percent-contained
  rises far too slowly to do this job; the decay is an early-days
  growth-rate decline whose cause is still open.
- Is there a published mechanism that does the same job? → E28: a daily
  containment probability by growth rate, learned by the filter, matches
  it with the decay switched off (E31: a tie everywhere).

**Verdict.** (a) rejected. (b) kept as the working stopping mechanism,
labelled a calibrated suppression proxy. E16c reports the global version
on the holdout.

**Later.** E16c, E17 (moisture × decay), E20 (the optimiser chose τ 2.5–
3.7 d), E21 (relabelled), E25 (the filter learns τ 5–20 d), E28 and E31
(replaced by the containment operator).
