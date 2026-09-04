# E18 — a dynamic fire-line agent (paint Inactive along the model's own edge) · REJECTED as tested, direction KEPT

_Round: Round 3 — 2026-09-02: real weather in, explosion out_

**Idea.** cella lets any cell be repainted between steps, so containment
does not need a pre-drawn line — an agent can build one during the run
from what the fire is doing, which is what an operational tool would do.
New harness hook `EXP_LINE_RATE` (`wildfire_experiment.rs`): after every
wind window, take the model's *own* active edge (fuel cells with a
burning 8-neighbour), rank heel-first (nearest the ignition centroid),
and paint `rate × window_days` of them Inactive, starting after 24 h. No
truth is consulted. Rates bracket a large incident's line production
(one 30 m cell = 30 m of line): 100 / 300 / 1000 cells/day = 3 / 9 / 30 km
of line per day. Also with p0 ×1.5. ERA5 daily wind, E1 (p0, dur), 3 seeds.

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

**Findings.**

- **Binary outcome: either the line does nothing or it encircles the fire
  on day two.** On day 1 the model fire is small, so its whole edge is a
  few hundred cells; 300 cells/day of heel-first line closes the ring and
  the fire dies at ×0.1 of the observed area (Brattain: ×0.0). At 100/day
  the ring never closes and the fire explodes as before. There is no rate
  in between that yields the observed "grows, then plateaus" curve,
  because a line that holds everywhere and is built from the heel is an
  all-or-nothing perimeter fence.
- Arrival MAE collapses (Chimney 54 h → 4 h at 1000/day) only because
  almost nothing burns after the ring closes — a reminder that MAE over
  cells-burned-in-both is not a stand-alone metric.
- The one mild positive (Buck, 300/day with p0 ×1.5, +0.04) is the case
  where the fire out-grew the crews for a while before being caught, i.e.
  the shape we want.

**What a realistic agent needs** (why the direction stays open):

1. **Resources that ramp up**, not a constant rate from hour 24: real
   incidents go from a few engines to thousands of personnel over 3–7
   days. A rate ∝ (1 − e^(−t/τ)) reproduces the E16 decay *mechanistically*
   and is the natural bridge between the two experiments.
2. **Line that can fail.** Real line holds on the heel and flanks and is
   overrun at the head in wind; painting Inactive is a perfect firebreak.
   Make painted cells low-flammability fuel (density ×0.1) instead, so
   wind and slope can breach them — the spotting and gust physics then
   matter again.
3. **Tactics that do not chase the head**: build where the front is slowest
   (upwind side, downslope), skip cells where the local spread probability
   is above a safety threshold — the "anchor and flank" doctrine.
4. **Observed containment as the calibration target**: daily
   percent-contained from the incident reports gives the agent's *output*
   to match, without touching the burned-area truth.

**Verdict.** Rejected as implemented (heel-first, constant rate, perfect
line). The mechanism — repaint cells between steps from the fire's own
state — works and is the operationally right place for suppression; it
needs the four changes above before it can beat a decay.
