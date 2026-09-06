# E7 — spotting on · REJECTED

_Round 1 (2026-08-15) · 1 seed · calibration fires · runner `exp_spotting.py` · raw results not kept in `results/` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** Spotting is embers starting new fires ahead of the front.
The model supports it and it was off. We switched it on at three settings
inside the pre-registered range. Every setting was neutral to
catastrophic: the mid and far settings doubled to quadrupled the burned
area and cost 0.1–0.2 of mean IoU on every fire. Spotting amplifies the
one problem the model already has, burning too much.

**Question.** Does ember spotting improve the score?

**What we changed.** Three settings inside the pre-registered spotting
space: spot probability 0.001–0.005 per burning cell, median jump 5–20
cells. E1 recipe otherwise.

**Why we expected it to matter.** The fast fires (Chimney, Brattain,
Ferguson) moved 2–7 km in a day, which surface spread at 30 m per tick
cannot do; spotting is how real fires make such jumps.

**How we scored it.** Mean IoU and area ratio against the E1 recipe, one
seed, four calibration fires.

**Result.** Every setting neutral to catastrophic. Mid and far settings
add ×2–4 area and cost 0.1–0.2 mean IoU on every fire.

How to read it: a loss of 0.1–0.2 is five to ten times the noise floor.

**What it means.** Until the model can stop, anything that helps it start
new fires makes it worse. Spotting may matter for the fast wind-driven
fires once a stopping mechanism exists, and it will matter on the Camp
Fire tier (TEST_PLAN §2, T4), which is an ember-driven fire.

**Questions this raises.**

- Does spotting help once the model can stop? Open; not retested after
  E16 or E28.
- Is spotting the missing mechanism for the fast fires? → E19 and E37
  point elsewhere first: the wind rule does not stretch the fire at all.

**Verdict.** Rejected. Not worth revisiting until the model can stop.

**Later.** Not revisited. The stopping problem was addressed by E16
(decay) and E28 (containment operator); spotting was never re-enabled.
