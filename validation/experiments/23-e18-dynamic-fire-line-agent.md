# E18 — a dynamic fire-line agent (paint Inactive along the model's own edge) · REJECTED as tested, direction KEPT

_Round 3 (2026-09-02) · 3 seeds · calibration fires · runner `exp_fireline.py`, harness hook `EXP_LINE_RATE` · results `exp18_fireline.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** The engine lets any cell be repainted between steps, so a
crew can be simulated: after each day, take the fire's own edge, start
nearest the ignition, and paint some length of it unburnable. No truth is
consulted. The outcome is binary. At 100 cells a day the ring never
closes and the fire explodes as before; at 300 or more it closes on day
two and the fire dies. Nothing in between gives the real "grows, then
plateaus" curve. The mechanism is right; the tactics are not.

**Question.** Can a rule-based crew, working from the fire's own state,
stop the model at the right size?

**What we changed.** Harness hook `EXP_LINE_RATE`: after every wind
window, take the model's active edge (fuel cells with a burning
8-neighbour), rank heel-first (nearest the ignition centroid), and paint
`rate × window_days` of them Inactive, starting after 24 h. Rates bracket
a large incident's line production (one 30 m cell = 30 m of line):
100 / 300 / 1000 cells/day = 3 / 9 / 30 km of line per day. Also with p0
×1.5. ERA5 daily wind, E1 recipe.

**How we scored it.** Mean IoU with area ratio in brackets, 3 seeds, four
calibration fires.

**Result.**

| variant | Bear | Brattain | Buck | Chimney |
|---|---|---|---|---|
| ctrl | 0.311 (×3.4) | 0.336 (×3.7) | 0.403 (×0.6) | 0.441 (×2.1) |
| line 100/day | 0.239 (×2.2) | 0.330 (×3.6) | 0.397 (×0.4) | 0.443 (×2.0) |
| line 100, p0 ×1.5 | 0.255 (×3.9) | 0.325 (×4.2) | 0.417 (×1.2) | 0.436 (×2.9) |
| line 300 | 0.203 (×0.1) | 0.123 (×0.1) | 0.382 (×0.3) | 0.431 (×1.7) |
| line 300, p0 ×1.5 | 0.298 (×0.3) | 0.297 (×4.0) | **0.440** (×0.5) | 0.415 (×2.5) |
| line 1000 | 0.187 (×0.1) | 0.056 (×0.0) | 0.374 (×0.3) | 0.222 (×0.1) |
| line 1000, p0 ×1.5 | 0.218 (×0.1) | 0.072 (×0.0) | 0.422 (×0.3) | 0.241 (×0.1) |
| Circle | 0.541 | 0.450 | 0.670 | 0.372 |

How to read it: mean IoU with area ratio in brackets. An area ratio of
×0.1 means the line strangled the fire; ×3–4 means it was ignored. Bold
is the one cell that beat its control.

- **Binary outcome: the line does nothing or encircles the fire on day
  two.** On day 1 the model fire is small, so its whole edge is a few
  hundred cells; 300 cells/day of heel-first line closes the ring and the
  fire dies at ×0.1 (Brattain ×0.0). At 100/day the ring never closes.
- Arrival MAE collapses (Chimney 54 h → 4 h at 1000/day) only because
  almost nothing burns after the ring closes: MAE over cells burned in
  both is not a stand-alone metric.
- The one mild positive (Buck, 300/day with p0 ×1.5, +0.04) is the case
  where the fire out-grew the crews for a while before being caught,
  which is the shape we want.

**What it means.** A line that holds everywhere and is built from the
heel is an all-or-nothing fence. A realistic agent needs four things:
resources that ramp up over 3–7 days (a rate ∝ 1 − e^(−t/τ) would
reproduce E16's decay mechanistically); line that can fail (low-
flammability fuel instead of Inactive, so wind and slope can breach it);
tactics that do not chase the head (anchor and flank); and observed
percent-contained as the target to match.

**Questions this raises.**

- Do those four changes produce a middle ground? → E23: no. Perfect line
  strangles, breachable line is ignored.
- Is retardant (a multiplier, not a fence) different? → E27: same binary
  outcome.
- Is percent-contained usable as a driver instead? → E21: no; it rises
  too slowly.

**Verdict.** Rejected as implemented. The mechanism (repaint cells between
steps from the fire's own state) works and is the operationally right
place for suppression.

**Later.** E23, E27 (both parked), E28 (stopping moved into the ensemble
as a probability instead of a place).
