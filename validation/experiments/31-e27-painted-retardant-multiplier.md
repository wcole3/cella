# E27 — painted retardant: a per-cell multiplier that dries out · REJECTED (agent tactics, not the material, are the problem)

_Round 4 (2026-09-04) · 3 seeds · calibration fires · runner `exp_retardant.py`, hooks `EXP_LINE_TYPE=density:<f>`, `EXP_LINE_RECOVER_H` · results `exp27_retardant.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** The literature (Giménez et al. 2004; PROPAGATOR) models
retardant as treated fuel that is
much harder to ignite and recovers as the coating breaks. We added
exactly that: a per-cell multiplier on p0 that can be painted and later
restored, and let the E23 crew lay retardant instead of a wall. Same
binary outcome as every line agent before it: a multiplier of 0.02
behaves like a fence, 0.1 like nothing. Recovery time makes no
difference. The material is right; the decision of where to put it is
what we have not modelled.

**Question.** Does a realistic retardant (weakened fuel that recovers)
give the line agent a middle ground?

**What we changed.** New `WildfireModel::set_density(idx, f)`. Agent at
1000 cells/day, 3-day ramp, up-wind tactic; multiplier 0.02 / 0.05 / 0.1
(fresh drop → light coverage); recovery to 1.0 after 24 h, 72 h or never.
ERA5 daily wind, E1 recipe.

**How we scored it.** Mean IoU with area ratio in brackets, 3 seeds, four
calibration fires.

**Result.**

| variant | Bear | Brattain | Buck | Chimney |
|---|---|---|---|---|
| ctrl | 0.311 (×3.4) | 0.336 (×3.7) | 0.403 (×0.6) | 0.441 (×2.1) |
| 0.02, recover 24 h | 0.264 (×0.2) | 0.342 (×3.2) | 0.419 (×0.4) | 0.385 (×1.2) |
| 0.02, never | 0.262 (×0.2) | 0.335 (×3.0) | 0.419 (×0.4) | 0.367 (×0.9) |
| 0.05, recover 24 h | 0.275 (×0.3) | 0.343 (×3.4) | 0.420 (×0.4) | **0.443** (×1.8) |
| 0.1, recover 72 h | 0.276 (×0.5) | 0.339 (×3.5) | **0.422** (×0.4) | 0.444 (×1.9) |
| Circle | 0.541 | 0.450 | 0.670 | 0.372 |

How to read it: mean IoU with area ratio in brackets; the first number in
a variant is the multiplier on p0 for treated cells. Bold marks cells at
or just above control.

- **Same binary outcome as E18/E23.** Bear is strangled (area ×0.2–0.8,
  IoU −0.04); Brattain is untouched (×3.0–3.5); Buck and Chimney move by
  ≤ 0.02.
- **Recovery time is irrelevant at this cadence.** 24 h, 72 h and never
  give the same scores to two decimals: by the time treated fuel dries,
  the front has either been stopped there or has gone elsewhere.

**What it means.** The mechanism is right and cheap (paint, restore, per
cell); what is missing is the *decision* of where and how much. Real
crews place retardant ahead of the head on a flank they can hold, at
coverage matched to the fuel. Three agents (E18, E23, E27) with geometric
rules have not found that; it wants observed drop and line locations or a
fire-intensity model.

**Questions this raises.**

- Where would observed drop and line locations come from? Open; incident
  GIS drop logs and dozer lines.

**Verdict.** Rejected as an experiment; `set_density` kept as the
representation of retardant and wet line.

**Later.** E28 (stopping handled as a probability per ensemble member
rather than a place).
