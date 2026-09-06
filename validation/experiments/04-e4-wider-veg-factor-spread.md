# E4 — wider veg_factor spread · REJECTED

_Round 1 (2026-08-15) · 1 seed · calibration fires · runner `exp_variants.py` · raw results not kept in `results/` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** Each fuel class has a multiplier on p0 (veg_factor). The
defaults are close together. We tried a much wider spread, grass burning
easily and timber litter hardly at all. Bear and Buck each lost 0.06;
the other two did not move. This spread is wrong. The per-class factors
stay in the search space for a proper calibration.

**Question.** Does a wider difference between fuel classes improve the
score?

**What we changed.** veg_factor Grass 2.0 / GrassShrub 1.4 / Shrub 1.0 /
TimberUnder 0.7 / TimberLitter 0.3 / Slash 0.8, against the narrow
defaults. E1 recipe otherwise.

**Why we expected it to matter.** Grass fires and timber fires spread at
very different rates in reality, and the six fires mix both fuels.

**How we scored it.** Change in mean IoU from the E1 recipe, one seed,
four calibration fires.

**Result.** Bear −0.061, Buck −0.064, Brattain and Chimney ≈ flat.

How to read it: negative is worse. Two clear losses, two ties.

**What it means.** Making grass twice as flammable pushes an already
over-burning model further over the cliff on grass-heavy landscapes.
The fuel classes probably do differ, but not in this direction at this
p0.

**Questions this raises.**

- What spread would help? Open. E20's optimiser searched a grass-to-
  timber ratio per fire and found values from 0.3 to 3.7 with no pattern,
  so the ratio does not transfer between fires.

**Verdict.** Rejected. Per-class veg_factors stay in the calibration
search space; this particular spread is not it.

**Later.** E20 (fuel ratios do not transfer).
