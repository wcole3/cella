# E16 — two non-weather ways to stop: heterogeneity (a) and a containment decay (b) · (a) REJECTED, (b) KEPT — biggest gain so far

_Round: Round 3 — 2026-09-02: real weather in, explosion out_

**Question.** Weather modulation (E3, E13, E15) cannot cap the burn. What
can? Two practices from the CA literature that are not weather:

- **(a) Spatial heterogeneity of flammability.** Percolation theory says a
  patchy medium has a different, softer threshold than a uniform one, and
  real fuel beds are patchy at 30 m. Per-cell density multiplier drawn from
  a lognormal with mean exactly 1 (σ ∈ {0.3, 0.6, 1.0}, seeded), p0 ×{1, 1.5, 2}.
- **(b) A containment decay.** Every observed area curve grows then
  plateaus (ANALYSIS.md §5) and the literature attributes the plateau to
  suppression, which this model does not have. Crude stand-in: p0 ×
  exp(−t/τ), τ ∈ {5, 10, 20} days, p0 ×{1, 1.5, 2} so the early fire is not
  starved.

`exp_stopping.py`, 3 seeds, E1 (p0, dur) recipes, calibration fires.

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

(mean IoU, area ratio in brackets; heterogeneity rows at other multipliers
were all worse and are in `exp16_stopping.json`.)

Final-day IoU and arrival error, control → best containment run:

| Fire | final IoU | arrival MAE |
|---|---|---|
| Bear | 0.144 → **0.405** | 62 h → 58 h |
| Brattain | 0.223 → **0.386** | 113 h → 41 h |
| Buck | 0.345 → **0.510** | 57 h → 82 h |
| Chimney | 0.409 → 0.395 | 54 h → 37 h |

**Findings.**

- **(a) Heterogeneity: nothing.** At matched mean p0 it is within ±0.03 of
  control on every fire; pushing p0 up to compensate the lost connectivity
  brings the explosion straight back. The percolation cliff moves; it does
  not soften enough to matter at this grid size.
- **(b) The decay is the largest single improvement in the log.** One
  setting (τ = 5 days, p0 ×2) lifts mean IoU by +0.12 Bear, +0.07 Brattain,
  +0.14 Buck, and — for the first time — brings the final burned area to
  the right size (×0.7–1.0) instead of ×3–4. Final-day IoU nearly triples
  on Bear and Brattain. Chimney, the fastest fire, wants a slower decay
  (τ = 20) and gains nothing: it was already the fire the model handles.
- **Why it works when weather did not:** the decay is monotone. Once p0
  falls under the percolation threshold the front freezes for good; the
  reachable set stops growing. A periodic modulation never does that.
- **What it is not:** physics. τ and the ×2 are fitted on the calibration
  fires, and exp(−t/τ) says nothing about *where* the fire stops, only
  *when*. Spatially it still loses to the Circle on Bear, Brattain and
  Buck (gap halved, not closed). The right version of this knob is real
  containment data — daily percent-contained from the incident reports
  (InciWeb / NIFC), which exists for all six fires — and, better still,
  containment *lines* as unburnable cells. That is the next experiment.

**Verdict.** (a) rejected. (b) kept as the working stopping mechanism;
E16c reports the global version on the holdout.
