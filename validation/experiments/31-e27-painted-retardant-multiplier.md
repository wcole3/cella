# E27 — painted retardant: a per-cell multiplier that dries out · REJECTED (agent tactics, not the material, are the problem)

_Round: Round 4 — 2026-09-04: ensembles_

**Why.** The literature models retardant as treated fuel that is much
harder to ignite and recovers as the coating breaks (Giménez et al. 2004;
PROPAGATOR sets treated cells to near-inert moisture). New
`WildfireModel::set_density(idx, f)` paints exactly that — a per-cell
multiplier on the base probability that can later be restored — so the
line agent can lay retardant instead of an unburnable fence (E18/E23).
Harness hooks `EXP_LINE_TYPE=density:<f>` and `EXP_LINE_RECOVER_H`.

**Runs.** `exp_retardant.py`: agent at 1000 cells/day, 3-day ramp,
up-wind tactic; multiplier 0.02 / 0.05 / 0.1 (fresh drop → light
coverage); recovery to 1.0 after 24 h, 72 h or never. ERA5 daily wind, E1
(p0, dur), 3 seeds, calibration fires.

| variant | Bear | Brattain | Buck | Chimney |
|---|---|---|---|---|
| ctrl | 0.311 (×3.4) | 0.336 (×3.7) | 0.403 (×0.6) | 0.441 (×2.1) |
| 0.02, recover 24 h | 0.264 (×0.2) | 0.342 (×3.2) | 0.419 (×0.4) | 0.385 (×1.2) |
| 0.02, never | 0.262 (×0.2) | 0.335 (×3.0) | 0.419 (×0.4) | 0.367 (×0.9) |
| 0.05, recover 24 h | 0.275 (×0.3) | 0.343 (×3.4) | 0.420 (×0.4) | **0.443** (×1.8) |
| 0.1, recover 72 h | 0.276 (×0.5) | 0.339 (×3.5) | **0.422** (×0.4) | 0.444 (×1.9) |
| Circle | 0.541 | 0.450 | 0.670 | 0.372 |

**Findings.**

- **Same binary outcome as E18/E23.** Bear is strangled (area ×0.2–0.8,
  IoU −0.04); Brattain is untouched (×3.0–3.5); Buck and Chimney move by
  ≤ 0.02. A multiplier of 0.02 behaves like an Inactive fence, 0.1 like
  no line; there is no intermediate coverage that produces a fire which
  is *slowed and steered* rather than stopped or ignored.
- **Recovery time is irrelevant at this cadence.** 24 h, 72 h and never
  give the same scores to two decimals: by the time treated fuel dries,
  the front has either been stopped there or has gone elsewhere.
- The mechanism is right and cheap (paint, restore, per cell); what is
  missing is the *decision* of where and how much — real crews place
  retardant ahead of the head on a flank they can hold, at coverage
  matched to the fuel. Three agents (E18, E23, E27) with geometric rules
  have not found that; it wants either observed drop/line locations or a
  fire-intensity model to reason about.

**Verdict.** Rejected as an experiment; `set_density` kept as the correct
representation of retardant and wet line for when observed suppression
locations are available (incident GIS drop logs, dozer lines).
