# E17 — periodic × monotone: hourly fuel-moisture damping × containment decay · KEPT (physics retained at no cost)

_Round: Round 3 — 2026-09-02: real weather in, explosion out_

**Why.** E16b's uniform decay works but has no day/night physics; E15's
moisture damping has the physics but cannot cap the burn. Both effects
are real, so the question is whether they can be combined without losing
the gain. `exp_combined.py`, 3 seeds, E1 (p0, dur), hourly station wind
×1 on every run. Per hourly window: p0 × η_moisture(RH, T; M_x 35 %) ×
exp(−t/τ). Because both factors shrink p0, the multiplier is scanned up
to ×4. Decay-alone rows are the apples-to-apples control on the same
hourly windows.

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

Final-day IoU, decay-alone ×2 vs moisture × decay ×4: Bear 0.367 →
**0.396**, Brattain 0.340 → **0.353**, Buck 0.359 → **0.446**, Chimney
0.269 → 0.276. Arrival MAE moves ±12 h either way.

**Findings.**

- **Adding the moisture cycle costs nothing and slightly improves the
  final map.** At a matched area ratio (≈ ×0.8–1.0) the combined
  schedule equals decay-alone on mean IoU (within ±0.02) and beats it on
  final-day IoU on all four fires. The extra multiplier (×4 vs ×2) is just
  the mean of η (≈ 0.6) being paid back.
- The daily truth cadence is why the mean-IoU gain is small: a day/night
  rhythm changes *when within the day* cells burn, which daily masks
  cannot see. The moisture term is expected to earn its keep on the
  sub-daily tiers (GOFER hourly, PT-FireSprd 3-hourly), which is where the
  physics should be judged.
- Chimney again prefers the slow decay (τ10) and gains from moisture there
  (0.437 → 0.439, noise-level) — consistent with E16: the fast fires are
  rate-limited, not stop-limited.
- Practical recipe for now: **p0 = 4 × E1, dur as E1, hourly station
  wind, η at M_x 35 %, decay τ 5 d** (τ 10 for fast fires). Physics kept
  where we have it, proxy where we do not.

**Verdict.** Kept: the combined schedule replaces decay-alone as the
working recipe. The decay itself remains a proxy to be replaced by real
containment data (E16c conclusion).
