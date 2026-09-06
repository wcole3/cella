# E12 — shape: how round is the model? · finding

_Round 2 (2026-09-01) · seed 0 · calibration fires · one-off script · results `exp12_shape.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** IoU says how much two maps overlap, not what shape they
are. We measured shape directly: elongation (1.0 = disc) of the final
burned set for the truth, the model and the Circle, and each day's
growth direction compared with the wind. Brattain, the most elongated
real fire at 2.7, comes out of the model as a near-perfect disc (1.05).
On Chimney both the real fire and the model grow *against* the ERA5 wind,
following fuel and terrain, which is why it is the one fire the model
does well on.

**Question.** Are the model's fires the right shape, and do they follow
the wind?

**What we measured.** Seed-0 arrival fields at the E1 recipe. Elongation
= √(λ₁/λ₂) of the burned set's second-moment matrix (1.0 = disc). Daily
growth direction = centroid of the new burn relative to the previous
burned set. Mean absolute angle between two directions, where 90° means
no relationship.

**Result.**

| Fire | elongation truth / model / Circle (final) | mean \|truth − model\| growth dir | mean \|truth − ERA5 wind\| | mean \|model − ERA5 wind\| |
|---|---|---|---|---|
| Bear | 2.30 / 2.14 / 1.25 | 91° | 92° | 47° |
| Brattain | 2.70 / **1.05** / 1.24 | 68° | 65° | 73° |
| Buck | 1.02 / 1.69 / 1.37 | 75° | 71° | 77° |
| Chimney | 1.56 / 1.34 / 1.00 | 53° | 128° | 127° |

How to read it: the first column is three elongations, truth then model
then Circle; 1.0 is a disc. The angle columns compare daily growth
directions; 90° means unrelated, well under 90° means aligned, well over
means opposed. Bold marks the model's worst shape miss.

- Brattain: truth 2.70, model 1.05. The most elongated real fire is a
  disc in the model.
- Bear: the model's growth follows the ERA5 wind (47°) while the real
  fire does not (92°). The ERA5 direction there is actively misleading.
- Chimney: truth and model both grow against the ERA5 wind (128°, 127°).
  The model is following fuel and terrain corridors, which is also why
  it beats the Circle there.
- Buck: the real fire is round (1.02); the model makes it too long (1.69).

**What it means.** The model has a shape problem that IoU hides. It
cannot make a long fire where the real fire is long, and it follows the
ERA5 wind where the real fire ignores it. Shape diagnostics belong in the
harness so every run carries them.

**Questions this raises.**

- What sets the model's shape if not wind? → E19, E37: terrain and fuel;
  the wind rule changes speed by 5–10 % and shape almost not at all.
- Can *any* knob setting make Brattain's shape at Brattain's size? →
  E37: no. Large and elongated is outside the reachable region.

**Verdict.** Finding. Elongation and growth direction are worth adding
to the report JSON.

**Later.** E19 (rate is set by p0, not wind), E22 (a wind clock widens
rather than stretches), E37 (the reachable region is a wedge; E12's
measure became one of its two axes), E30 (kernel refit, the fix, not
yet run).
