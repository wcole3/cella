# E2 — canopy-cover density layer · REJECTED

_Round 1 (2026-08-15) · 1 seed · calibration fires · runner `exp_variants.py::cc_density` · raw results not kept in `results/` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** The model treats every cell of a fuel class the same. The
dataset carries a canopy-cover layer (how much of each cell is covered by
tree crowns), so we tried using it as a per-cell flammability multiplier.
It helped one fire slightly, hurt two, and hurt Buck by 0.045. This
particular mapping is wrong; the idea is not ruled out.

**Question.** Does per-cell tree cover, used as a density multiplier on
p0, add the spatial detail the model lacks?

**What we changed.** Density from the HDF5 `230CC` layer: forest cells
0.5 + 0.5 × (CC / 75), grass cells kept at 1.0. CC is *tree* cover, so
grass must not be punished for having none. Everything else at the E1
recipe.

**Why we expected it to matter.** Real fuel beds are patchy at 30 m and
the model's are uniform inside each fuel class. Patchiness changes where
a fire can go.

**How we scored it.** Change in mean IoU from the E1 recipe, one seed,
four calibration fires.

**Result.** Bear +0.018, Brattain −0.017, Buck −0.045, Chimney −0.009.

How to read it: positive is better. At one seed anything under about 0.02
is noise, so Bear, Brattain and Chimney are ties and Buck is a real loss.

**What it means.** Mixed to negative. The canopy layer describes trees,
and the model's problem is not which trees burn but that it burns
everything within reach. A per-cell multiplier that lowers p0 in some
places just shifts where the over-burn happens.

**Questions this raises.**

- Would a different mapping (or canopy cover as a fuel-class refinement)
  work? Open; not retried.
- Does spatial patchiness of any kind help at this grid size? → E16a:
  random heterogeneity with the same mean changes nothing.

**Verdict.** Rejected as tested. Per-class veg_factors stay in the
calibration search space.

**Later.** The density multiplier itself was reused as the representation
of painted retardant (E27, `set_density`). E20 found that even the coarse
grass-to-timber ratio does not transfer between fires (0.3 to 3.7).
