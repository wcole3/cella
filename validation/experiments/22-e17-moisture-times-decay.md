# E17 — periodic × monotone: hourly fuel-moisture damping × containment decay · KEPT (physics retained at no cost)

_Round 3 (2026-09-02) · 3 seeds · calibration fires · runner `exp_combined.py` · results `exp17_combined.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** E16's decay stops the fire but has no day/night physics.
E15's moisture damping has the physics but cannot stop the fire. Put
together, at a matched burned area, the pair equals the decay alone on
mean IoU and beats it on the final-day map on all four fires. Adding the
moisture cycle costs nothing, so it stays. The combined schedule became
the working recipe of Round 3.

**Question.** Can the moisture physics and the decay be combined without
losing the decay's gain?

**What we changed.** Per hourly window: p0 × η_moisture(RH, T; M_x 35 %)
× exp(−t/τ). Hourly station wind ×1 on every run. Because both factors
shrink p0, the multiplier is scanned up to ×4. Decay-alone rows on the
same hourly windows are the apples-to-apples control.

**How we scored it.** Mean IoU with area ratio in brackets, 3 seeds, four
calibration fires; final-day IoU for the two headline rows.

**Result.**

| variant | Bear | Brattain | Buck | Chimney |
|---|---|---|---|---|
| decay τ5, p0 ×2 | 0.416 (×0.5) | 0.360 (×0.6) | 0.437 (×0.5) | 0.386 (×0.7) |
| decay τ5, ×3 | 0.408 (×1.2) | 0.377 (×1.2) | **0.520** (×1.1) | 0.396 (×1.2) |
| moist × decay τ5, ×3 | 0.397 (×0.5) | 0.364 (×0.6) | 0.411 (×0.5) | 0.400 (×0.6) |
| **moist × decay τ5, ×4** | **0.418** (×0.8) | **0.383** (×1.0) | 0.504 (×0.8) | 0.390 (×0.7) |
| decay τ10, ×2 | 0.355 (×1.5) | 0.352 (×2.1) | 0.482 (×0.8) | 0.437 (×1.7) |
| moist × decay τ10, ×3 | 0.365 (×1.4) | 0.354 (×2.0) | 0.465 (×0.7) | 0.431 (×1.5) |
| moist × decay τ10, ×4 | 0.323 (×2.4) | 0.329 (×2.9) | 0.448 (×1.3) | **0.439** (×1.7) |
| Circle | 0.541 | 0.450 | 0.670 | 0.372 |

How to read it: mean IoU with area ratio in brackets; compare rows at a
similar area ratio (≈ ×0.8–1.0), because a bigger multiplier buys area
and area buys score. Bold is the best row per fire.

Final-day IoU, decay-alone ×2 vs moisture × decay ×4: Bear 0.367 →
**0.396**, Brattain 0.340 → **0.353**, Buck 0.359 → **0.446**, Chimney
0.269 → 0.276. Arrival MAE moves ±12 h either way.

- **Adding the moisture cycle costs nothing and slightly improves the
  final map.** At matched area the combined schedule equals decay-alone
  on mean IoU (within ±0.02) and beats it on final-day IoU on all four
  fires. The extra multiplier (×4 vs ×2) is the mean of η (≈ 0.6) being
  paid back.
- The daily truth cadence is why the mean-IoU gain is small: a day/night
  rhythm changes *when within the day* cells burn, which daily masks
  cannot see.
- Chimney again prefers the slow decay (τ10) and gains from moisture
  there (0.437 → 0.439, noise level): the fast fires are rate-limited,
  not stop-limited.

**What it means.** Keep the physics where we have it and the proxy where
we do not. Working recipe after this experiment ("the E17 recipe"):
**p0 = 4 × E1, dur as E1, hourly station wind, η at M_x 35 %, decay τ 5 d**
(τ 10 for fast fires).

**Questions this raises.**

- Will the moisture term earn its keep on sub-daily truth? Open; that is
  where the physics should be judged (GOFER hourly, PT-FireSprd
  3-hourly).
- What replaces the decay? → E21 (real containment: no), E28 (containment
  operator: yes, inside the ensemble).

**Verdict.** Kept: the combined schedule replaces decay-alone as the
working recipe. The decay itself remains a proxy to be replaced.

**Later.** E20 (the optimiser dropped moisture on three of four fires
because mean IoU at daily truth does not reward it), E21, E22 (the E17
recipe is the base for the clock test), E28.
