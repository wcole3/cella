# E16c — the containment decay as one global recipe, reported on all six fires · KEPT with a caveat

_Round: Round 3 — 2026-09-02: real weather in, explosion out_

**Why.** E16b chose τ = 5 days and p0 ×2 while looking at the calibration
fires with their per-fire E1 recipes. The honest headline is one setting
for every fire, and the only unbiased test is the holdout pair (Ferguson,
Pier), which no parameter was ever chosen on. `exp_contain_holdout.py`,
3 seeds, the Round-1 global recipe (p0 0.22, dur 5) with and without the
decay.

| Fire | global control | + decay τ5, p0 ×2 | + decay τ10, p0 ×1.5 | Circle |
|---|---|---|---|---|
| Bear | 0.220 (×4.0) | **0.319** (×1.5) | 0.275 (×2.0) | 0.541 |
| Brattain | 0.308 (×3.3) | 0.381 (×0.6) | **0.390** (×0.9) | 0.450 |
| Buck | 0.369 (×2.4) | **0.455** (×1.5) | 0.445 (×1.5) | 0.670 |
| Chimney | **0.421** (×1.5) | 0.393 (×0.6) | 0.397 (×0.9) | 0.372 |
| **Ferguson (holdout)** | 0.145 (×0.9) | 0.156 (×0.1) | 0.148 (×0.1) | 0.373 |
| **Pier (holdout)** | 0.321 (×4.3) | **0.463** (×1.6) | 0.388 (×2.3) | 0.559 |

(mean IoU, area ratio in brackets. Final-day IoU with the τ5 recipe: Bear
0.107 → 0.315, Brattain 0.232 → 0.352, Buck 0.303 → 0.509, Pier
0.229 → 0.456.)

**Findings.**

- **The holdout confirms the effect on the fire it can help.** Pier, never
  used for tuning, gains +0.14 mean IoU and doubles its final-day IoU; the
  area ratio drops from ×4.3 to ×1.6. Same magnitude as the calibration
  fires. This is not a memorised result.
- **It cannot help a fire the model already under-burns.** Ferguson's
  global control already burns only ×0.9 of the observed area (the model
  is too slow there — see the front-speed note in Round 2); the decay
  starves it to ×0.1 and the score stays at 0.15. Chimney, the other fast
  fire, loses 0.03 for the same reason. **A time decay fixes over-burning
  only; it makes under-burning worse.** The two failure directions need
  two different mechanisms.
- With the decay, the global recipe now beats the Round-1 *per-fire*
  recipes on Bear/Brattain/Buck without the decay (0.32/0.38/0.46 vs
  0.32/0.34/0.41) — one honest setting has caught up with four tuned ones.
- Still loses to the Circle on five of six. The gap is now 0.07–0.22
  instead of 0.13–0.32. Area is roughly right; *where* the fire stops is
  still wrong, which is exactly what a spatially blind exp(−t/τ) cannot
  know.

**Verdict.** Kept as the working global stopping mechanism, labelled as a
calibrated suppression proxy, not physics. Next: replace the fitted τ with
observed daily percent-contained from the incident reports, and test
containment lines as unburnable cells.
