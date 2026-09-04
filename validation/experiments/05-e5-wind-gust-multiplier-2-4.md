# E5 — wind gust multiplier ×2 / ×4 · REJECTED

_Round: Round 1 — 2026-08-15_

Hypothesis: ERA5 daily-mean winds (0.6–1.2 m/s) flatten the gusts;
at V ≈ 1 the wind kernel `exp(0.045·V)` ≈ 1.05 is nearly inert.
`EXP_WIND_SCALE`. Result: helps only Chimney at ×2 (+0.013), hurts Bear
(−0.02/−0.04), ≈ flat elsewhere. Consistently *narrows* the burn
(area ratios drop) — directionality without accuracy. The real fix is
per-cell wind (§10.4), not a scalar multiplier.
