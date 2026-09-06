# E16c — the containment decay as one global recipe, reported on all six fires · KEPT with a caveat

_Round 3 (2026-09-02) · 3 seeds · all six fires incl. holdout · runner `exp_contain_holdout.py` · results `exp16c_contain_global.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** E16b chose its decay while looking at the four calibration
fires. The honest test is one setting for every fire, scored on the two
holdout fires no parameter was ever chosen on. Pier, never tuned on,
gains +0.14 and doubles its final-day overlap: the effect is real, not
memorised. Ferguson does not move, because the model already burns too
little there and a decay only makes that worse. A time decay fixes
over-burning only. The two failure directions need two mechanisms.

**Question.** Does the decay transfer to fires it was not tuned on?

**What we changed.** The Round 1 global recipe (p0 0.22, dur 5) with and
without the decay (τ 5, p0 ×2; and τ 10, p0 ×1.5), 3 seeds, all six fires.

**How we scored it.** Mean IoU with area ratio in brackets; final-day IoU
for the τ 5 recipe.

**Result.**

| Fire | global control | + decay τ5, p0 ×2 | + decay τ10, p0 ×1.5 | Circle |
|---|---|---|---|---|
| Bear | 0.220 (×4.0) | **0.319** (×1.5) | 0.275 (×2.0) | 0.541 |
| Brattain | 0.308 (×3.3) | 0.381 (×0.6) | **0.390** (×0.9) | 0.450 |
| Buck | 0.369 (×2.4) | **0.455** (×1.5) | 0.445 (×1.5) | 0.670 |
| Chimney | **0.421** (×1.5) | 0.393 (×0.6) | 0.397 (×0.9) | 0.372 |
| **Ferguson (holdout)** | 0.145 (×0.9) | 0.156 (×0.1) | 0.148 (×0.1) | 0.373 |
| **Pier (holdout)** | 0.321 (×4.3) | **0.463** (×1.6) | 0.388 (×2.3) | 0.559 |

How to read it: mean IoU with area ratio in brackets; bold in a score
cell is the best setting for that fire; bold fire names are the holdout
pair. Final-day IoU with the τ5 recipe: Bear 0.107 → 0.315, Brattain
0.232 → 0.352, Buck 0.303 → 0.509, Pier 0.229 → 0.456.

- **The holdout confirms the effect on the fire it can help.** Pier gains
  +0.14 mean IoU and doubles its final-day IoU; area ratio ×4.3 → ×1.6.
  Same magnitude as the calibration fires.
- **It cannot help a fire the model already under-burns.** Ferguson's
  control already burns only ×0.9 of the observed area (the model is too
  slow there, see the front-speed note); the decay starves it to ×0.1 and
  the score stays at 0.15. Chimney, the other fast fire, loses 0.03 for
  the same reason.
- With the decay, one global setting now beats Round 1's *per-fire*
  recipes on Bear, Brattain and Buck (0.32/0.38/0.46 vs 0.32/0.34/0.41).
- Still loses to the Circle on five of six. The gap is 0.07–0.22 instead
  of 0.13–0.32.

**What it means.** Area is roughly right now; *where* the fire stops is
still wrong, which a spatially blind exp(−t/τ) cannot know. And the fast
fires need the opposite medicine: more speed, not an earlier stop.

**Questions this raises.**

- Can real containment records replace the fitted τ? → E21: no; they
  rise too slowly.
- Can containment *lines* (unburnable cells) supply the "where"? → E18,
  E23: rule-based line agents either strangle or are ignored; parked.
- What do the fast fires need? → E19: a kernel whose speed follows wind;
  E30 (not yet run).

**Verdict.** Kept as the working global stopping mechanism, labelled a
calibrated suppression proxy, not physics.

**Later.** E20 (transfer recipe lifts Pier further to 0.51), E21, E25 (the
filter lifts Ferguson from 0.13 to 0.34 by raising p0 and switching the
decay off by itself), E28.
