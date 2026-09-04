# E2 — canopy-cover density layer · REJECTED

_Round: Round 1 — 2026-08-15_

Hypothesis: per-cell density from the HDF5 `230CC` layer adds the spatial
heterogeneity the model lacks. Mapping: forest cells 0.5 + 0.5·(CC/75),
grass kept at 1.0 (CC is *tree* cover; grass must not be punished).
`exp_variants.py::cc_density`.

Result: Bear +0.018, Brattain −0.017, Buck −0.045, Chimney −0.009.
Mixed-to-negative. Other mappings might work; this one doesn't.
