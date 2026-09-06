# E8 — ensemble burn-probability threshold · finding, not a lever

_Round 1 (2026-08-15) · 5 seeds · calibration fires · runner `exp_ensemble.py` · raw results not kept in `results/` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** Run the same settings with five different seeds and ask:
should a cell count as burned if any seed burned it, or only if most
did? "Any seed" (the union) was best or tied on all four fires; stricter
voting only ever lost. The reason is a diagnosis: the over-burn halo is
the same on every seed, so averaging seeds cannot remove false alarms.
Seeds only differ in which parts of the real burn they cover.

**Question.** Can voting across seeds cut the model's false alarms?

**What we changed.** Nothing in the model. Five seeds of the E1 recipe;
score "cells burned in at least q of 5 seeds" for q = 1 (union) to 5
(unanimous).

**Why we expected it to matter.** If different seeds over-burn in
different places, requiring agreement should erase the disagreements and
leave the true burn.

**How we scored it.** Mean IoU of the voted mask, four calibration fires.

**Result.** Union (q = 1) best or tied on all four fires (+0.05 Buck,
+0.02 Bear over the single-seed run); every stricter q loses.

How to read it: q = 1 is the most generous mask; if it wins, the seeds
disagree about *true* burn, not about false alarms.

**What it means.** The over-burn is deterministic. Given a p0 above the
percolation threshold, every seed eventually burns everything reachable;
the dice only decide the order. So seed averaging cannot fix the model's
main problem. Union-of-seeds is a legitimate small gain if ensemble masks
are ever reported.

**Questions this raises.**

- Would an ensemble help if its members disagreed about where the fire
  *stops*? → E24: yes. Once the prior includes a stopping mechanism,
  members stop in different places and a 50 % vote beats the tuned single
  runs.
- Is there information in the probability itself, not just a threshold?
  → E24/E25: the Brier score of the probability map beats the Circle's on
  most fires.

**Verdict.** Finding. Not a lever in Round 1.

**Later.** E24 inverted this result with a stopping mechanism in the
prior; consensus at 0.5 became the standard ensemble mask. E32 showed 8
members is too few for a probability map and 32 is the knee.
