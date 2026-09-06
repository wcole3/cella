# E37 — illuminate the fire model: what shapes can it make? · finding — a wedge; three of six fires are outside anything the knobs can reach

_Round 5 (2026-09-05) · MAP-Elites, 960 evaluations per fire · all six fires incl. holdout · runner `exp_r5_illuminate.py` → `wildfire_smc map` · results `exp37_illuminate.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** E12 measured that the model's fires are rounder than the
real ones (Brattain elongation 2.7 observed vs 1.05 modelled at E1's
knobs) and E19/E22 argued the wind rule widens rather than stretches.
Both were about particular knob settings. Here we asked the whole knob
space at once: over every p0, burn duration and wind multiplier, what
combinations of *size* and *shape* can this model produce at all, on this
landscape, with this weather? The reachable region is a wedge: small
fires can be any shape, large fires are round. Brattain, Ferguson and
Pier are large and elongated, so no calibration will ever draw them.
That converts the shape problem into an acceptance test for the E30
kernel refit.

**Question.** What shapes can the fire model make at all, and are the
observed fires among them?

**What we ran.** MAP-Elites (library `Evolution`, `map_elites` search,
batch 32, 30 generations, iso + line emitter) over p0 0.08–0.6 (log), burn
duration 5–20 and wind × 0–1.5, no stopping rule and **no objective**
(pure illumination). Behaviour axes after five days of the scenario's own
weather: **growth** (burned share of the grid, axis 0–0.10, 20 bins;
explosive settings pile into the top bin) × **elongation** (√(λ₁/λ₂) of
the burned set's second-moment matrix, E12's measure, axis 1–4, 20 bins).
The observed perimeter's growth and elongation on the same five days are
plotted in the same coordinates.

**How we read it.** A filled bin means some knob setting produced a fire
of that size and shape. If the observed fire's bin is empty, the model
cannot draw it.

![Six archives: reachable growth × elongation cells shaded, the observed fire's daily position as dots](figures/e37-illuminate.svg)

**Result.**

| Fire | cells filled | model elongation, any size | model elongation at the observed size | observed (day 5) growth / elongation | reachable? |
|---|---|---|---|---|---|
| Bear | 125 (31 %) | 3.19 | 2.03 | 0.047 / 1.98 | yes |
| Brattain | 112 (28 %) | 5.52 | 1.35 | 0.110 / 1.83 | **no** |
| Buck | 60 (15 %) | 1.95 | 1.49 | 0.058 / 1.50 | borderline |
| Chimney | 48 (12 %) | 1.54 | 1.35 | 0.067 / 1.22 | yes |
| Ferguson* | 52 (13 %) | 2.06 | 1.38 | 0.080 / 1.61 | **no** |
| Pier* | 34 (9 %) | 1.39 | 1.15 | 0.102 / 1.45 | **no** |

How to read it: "cells filled" is coverage of the 400-bin map; the two
elongation columns are the most stretched fire the model made at any
size, and at a size at least as big as the real fire on day 5; then the
real fire's size and shape; then whether the real one lies inside the
reachable region. `*` is the holdout pair.

- **The reachable region is a wedge.** On every fire the archive fills
  from the bottom-left: small fires can be any shape up to 2–5, large
  fires are round (elongation 1.1–1.35). The most elongated elites are
  always the same recipe: burn duration 5, p0 0.08–0.18, **wind × ≈ 0**.
  Their shape comes from terrain and fuel, not from wind.
- **Wind does not stretch the fire.** Across all six archives, raising
  wind × moves a setting *right* (bigger) and *down* (rounder). The
  biggest elites have wind × 0.5–0.75 and elongation 1.1–1.35. This is
  E19's "wind changes speed 5–10 %" and E12's "too round" measured over
  the entire knob space: the exponential wind
  factor is a speed knob, not a shape knob.
- **Three observed fires sit outside the wedge.** Brattain (1.83 at
  growth 0.11), Ferguson (1.61 at 0.08) and Pier (1.45 at 0.10) are all
  above the most elongated fire the model can make at their size (1.35,
  1.38, 1.15). Buck is on the edge. Bear and Chimney, the round slow
  fires, are inside. For the three outside, **no p0 / duration / wind
  setting reproduces the observed shape**. That is the E31/E33 pattern:
  Bear and Chimney are where the ensemble beats or approaches the Circle,
  Brattain/Ferguson/Pier where it trails.
- Method note: with no objective every elite scores 1, so the archive is
  a pure map of reachability, and coverage (9–31 %) is the size of the
  model's behavioural repertoire on each landscape. The growth axis cap
  (0.10) was needed because the natural range puts every fire in one
  bin; 96–253 evaluations per fire ran past the cap into the top bin.

**What it means.** The illumination map separates "calibrate better" from
"the model cannot do this". Three of six fires (two holdout) are in the
second class. What would have to change is a mechanism that makes a
*large* fire elongated: a wind response that lengthens the head relative
to the flanks rather than raising every direction's probability. That is
E30's kernel refit, now with a target: after the refit, re-run this map
and the observed dots must fall inside the shaded region.

**Questions this raises.**

- Does the E30 refit move the wedge? Open; this experiment is its
  acceptance test.
- Would spotting (E7) or a fire-induced indraft (research notes §2) add
  reach at size? Open.

**Verdict.** Finding, the most useful one in the round for the model
work.

**Later.** E30 (not yet run).
