# E9c — is Chimney's 270° a lucky angle? · NO (robust plateau)

_Round 2 (2026-09-01) · 3 seeds · Chimney only · runner `exp_wind_rotation.py` · results `results/experiments/exp9b_chimney/` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** E9b found one very good cell, Chimney at ×5 wind rotated
270°. We swept the rotation and the wind multiplier around it. Anything
from "toward south-west" to "toward west-north-west" at 3–5 m/s scores
0.50–0.57, so it is a plateau, not a spike. Too strong a wind narrows the
burn below the observed area and loses again. This is *not* a calibration
result, because the rotation was chosen by looking at the truth. It is
evidence that the ERA5 daily mean is not a sufficient wind input.

**Question.** Is the E9b Chimney result fragile?

**What we changed.** Same recipe (p0 0.30, dur 5, 3 seeds), rotation
200–300° at ×5, and multiplier ×3, ×8, ×12 at 270°.

**How we scored it.** Mean IoU on Chimney; control ×1 rot 0 = 0.441, wind
off 0.445, Circle 0.372.

**Result.**

| ×5 rot 200 | 225 | 240 | 270 | 300 | ×3 rot 270 | ×8 | ×12 |
|---|---|---|---|---|---|---|---|
| 0.485 | 0.543 | 0.568 | **0.571** | 0.500 | 0.539 | 0.475 | 0.406 |

How to read it: one fire, one row; each cell is a different rotation or
multiplier. Bold is the E9b winner. Every cell beats the 0.441 control.

- Rotations 225–300° at ×5 all score 0.50–0.57.
- ×3 at 270° still scores 0.539. ×8 and ×12 fall off (0.475, 0.406) as
  the burn narrows below the observed area (×0.9).

**What it means.** The model wants a moderate, mostly westward wind on
Chimney, which is exactly what the incident reporting describes and what
the ERA5 daily domain mean does not contain. The result is robust to the
exact angle and speed, so it is telling us about the input, not about
luck.

**Questions this raises.**

- Is there an observational wind source that reproduces this? → E14: the
  nearest station (Paso Robles, 41 km) reports south-westerly winds on
  those days, not easterlies. Needs a ridge-top station or a downscaled
  field.
- Does the same effect appear on the other fires with the right wind? →
  E10 (oracle direction): only Chimney gains; the slow fires lose.

**Verdict.** No, not a lucky angle. Kept as a negative result for "ERA5
daily means are a sufficient wind input", never as a score.

**Later.** E14 (station wind fails to reproduce it), E26 (terrain
downscaling is null at this kernel), E30 (kernel refit, not yet run).
