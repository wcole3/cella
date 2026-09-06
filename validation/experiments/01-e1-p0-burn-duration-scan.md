# E1 — p0 × burn_duration scan · KEPT (the big lever)

_Round 1 (2026-08-15) · 1 seed · calibration fires · runner `exp_sweep.py`, 60 runs · raw results not kept in `results/` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** Out of the box the model burns far too much (TEST_PLAN §5:
Bear over-burned eight-fold). The two knobs that most directly set how
much burns are p0, the per-tick chance of spreading to a neighbour, and
burn duration, how long a cell keeps burning. We scanned both on the four
calibration fires. Both matter, about equally. The best p0 differs by fire
(0.12 to 0.30), so one shared setting scores lower than per-fire settings.
Chimney became the first fire to beat the Circle. Every later experiment
starts from the per-fire settings found here, called "the E1 recipe".

**Question.** Which (p0, burn duration) pair scores best on each fire, and
does one pair work for all four?

**What we changed.** p0 ∈ {0.08, 0.12, 0.16, 0.22, 0.30} × burn duration
∈ {2, 5, 10} ticks: 15 pairs per fire, 60 single-seed runs. Both ranges
sit inside the pre-registered search space (TEST_PLAN §6). Wind is the
ERA5 daily mean; no schedules, no spotting.

**Why we expected it to matter.** The textbook p0 of 0.58 was fitted for
a much coarser time step. At 50 ticks per day it puts every fire far
above the percolation cliff, so the model burns everything reachable.

**How we scored it.** Mean IoU over the observation days, higher is
better, one seed, each fire's best pair against the Circle on the same
fire.

**Result.**

| Fire | best (p0, dur) | mean IoU | Circle | note |
|---|---|---|---|---|
| Bear | 0.12, 10 | 0.317 | 0.541 | was ~0.12 final uncalibrated |
| Brattain | 0.22, 10 | 0.337 | 0.450 | |
| Buck | 0.16, 5 | 0.414 | 0.670 | under-burns (×0.7) at best point |
| Chimney | 0.30, 5 | 0.435 | 0.372 | **beats the Circle** |

How to read it: "best (p0, dur)" is the winning pair for that fire and
"mean IoU" its score. "Circle" is the dumb forecaster on the same fire;
the model beats it only where its number is higher. "×0.7" is an area
ratio: Buck's best run burned 70 % of the observed area.

- Best mean IoU per fire is 0.32–0.44. Uncalibrated final IoU had been
  0.12–0.28 (TEST_PLAN §5).
- Burn duration matters as much as p0. Duration 2 killed every fire: the
  front outruns its own fuel and goes out.
- The best p0 spans 0.12–0.30 across fires. The best single pair for all
  four (p0 0.22, dur 5) averages 0.334 against 0.376 for the per-fire
  bests. The original note put the cost of one global setting at about
  0.07 mean IoU; the two averages differ by 0.04, and the cost is larger
  on some fires than others.
- Buck under-burns (×0.7) even at its best point. The scan is trading
  burned area for fewer false alarms.

**What it means.** p0 and burn duration together are the size lever, and
they get the model from "loses to the Circle badly" to "loses by
0.05–0.25, beats it once". That the best p0 differs so much between fires
is itself a result: the model sits on a knife edge (TEST_PLAN §5), and
what each fire needs depends on things the model does not see. Chimney,
the fast wind-driven fire, beats the Circle because when a fire has a
strong direction, even a weak wind rule adds information about *where*.

**Questions this raises.**

- Can a p0 that changes over time do what a constant p0 cannot? → E3 and
  E13 (temperature: a little, on one fire), E15 (moisture: no), E16
  (a steady decay: yes, the biggest gain in the log).
- Is the spread in best p0 caused by the weak daily-mean wind? → E9b,
  E10: wind is inert at ERA5 speeds, so the spread is about the fires.
- Why does the best run under-burn Buck while over-burning others? → E8:
  the over-burn halo is the same on every seed, so lowering p0 is the
  only way to cut false alarms. E16b later fixes Buck with a stopping
  rule instead (+0.14).

**Verdict.** Kept. The per-fire (p0, dur) pairs are the E1 recipe that
every later experiment starts from.

**Later.** Round 1 verification at 3 seeds (E1 recipe + E3 schedule):
Bear 0.347, Brattain 0.344, Buck 0.448, Chimney 0.443 ([round-1.md](round-1.md)).
E11 showed p0 × ticks per day is one knob, so the E1 p0 values are tied to
50 ticks per day. E19 found duration 5 is below the fire's own survival
threshold at low p0, and E20's optimiser preferred duration 15–16. From
E24 on, p0 and duration are drawn from a prior instead of scanned.
