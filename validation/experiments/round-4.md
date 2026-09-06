# Round 4 — 2026-09-04: ensembles — Monte Carlo maps and a fire that learns as it burns

_Score family changes here: forecast consensus IoU and Brier of a 32-member ensemble · all six fires, nothing chosen per fire · terms: [GLOSSARY.md](GLOSSARY.md)_

## What we knew before

Single runs with a fitted decay reach 0.37–0.54 mean IoU and lose to the
Circle on five of six fires. The decay is a fitted clock, not physics.
The model is fast (5–7 s per fire-run). Question set by the user: can an
*ensemble* of runs approximate a fire better than any single run, and can
generations of simulations develop, the evolutionary idea from E20, but
on-line, while the fire is burning?

Design (one Rust example, `cella_lib/examples/wildfire_smc.rs`):

- **Open mode = Monte Carlo (E24).** M members drawn from one broad prior
  (p0 log-uniform 0.08–0.6, burn duration 5–20, containment τ log-uniform
  2–100 d, wind multiplier 0–1.5), each with its own seed, run
  independently. Output per observation time: the fraction of members in
  which each cell has burned, a burn-probability map, scored by Brier,
  consensus IoU (probability ≥ 0.5), best-threshold IoU (diagnostic; it
  peeks) and mean member IoU, next to persistence and the Circle. This is
  the "burn probability" product operational simulators ship.
- **Assim mode = particle filter with GA operators (E25).** At each
  observation time every member's *current* burned set is scored against
  the observed mask; members are resampled with weights exp(β·IoU); each
  child inherits its parent's grid state (you cannot re-draw the past)
  with mutated parameters (σ) and a fresh seed; a fraction of children are
  immigrants with fresh prior parameters; then everyone keeps simulating.
  Every score at t_k is a **one-window-ahead forecast** made from the
  state assimilated at t_{k−1}. That is how an operational nowcast would
  use yesterday's perimeter, and it is the honest way to use truth
  (TEST_PLAN v1.4).

Literature: Xue, Gu & Hu (ACM TOMACS 2012) assimilated fire-front
observations into DEVS-FIRE with particle filters; Rochoux et al. (NHESS
2014/15) and Zhang et al. (2017) did the same for a Rothermel/level-set
model; PyTorchFire (2025) calibrates by gradient on the first days. All
report that assimilation corrects both front position and parameters, and
all note the collapse problem, hence the immigrants operator.

## What we ran

| # | Question | Answer |
|---|---|---|
| [E24](28-e24-monte-carlo-burn-probability.md) | Does a 32-member burn-probability map from an untuned prior beat tuned single runs? | Ties or beats them on 4 of 6; far better calibrated. KEPT |
| [E25](29-e25-generational-ensemble-particle-filter.md) | Does learning from each day's mask improve the forecast? | +0.05 to +0.21 on every fire; Ferguson 0.13 → 0.34. KEPT (headline) |
| [E26](30-e26-terrain-wind-field.md) | Does terrain-downscaled wind help? | ±0.01; the kernel does not respond to wind. NULL, infrastructure kept |
| [E27](31-e27-painted-retardant-multiplier.md) | Does painted retardant give the crew a middle ground? | No; fence or nothing again. REJECTED |
| [E28](32-e28-containment-probability-operator.md) | Can a growth-dependent containment probability replace the decay? | Yes, at no cost. KEPT (recommended) |
| [E31](33-e31-generic-engine-replication.md) | Does the new generic engine reproduce E25/E28? | Yes within 0.009 on the recommended row; E28's Bear gain was noise. PASS |

## What we know now

Results (32 members, one untuned prior, all six fires, forecasts):

| Fire | open consensus (E24) | assimilating + immigrants (E25) | best tuned single run | Circle |
|---|---|---|---|---|
| Bear | 0.399 | **0.473** | 0.480 (E20 per-fire) | 0.541 |
| Brattain | 0.330 | **0.400** | 0.408 | 0.450 |
| Buck | 0.481 | **0.616** | 0.581 | 0.670 |
| Chimney | 0.382 | **0.426** | 0.474 | 0.372 |
| Ferguson (holdout) | 0.131 | **0.345** | 0.158 (transfer) | 0.373 |
| Pier (holdout) | 0.478 | **0.533** | 0.512 (transfer) | 0.559 |

How to read it: the first two columns are forecast consensus IoU (the
new score family); the "best tuned single run" column is the old family
(single-run mean IoU, and E20 peeked at the whole series), shown for
context. Bold is the ensemble's headline.

1. **The ensemble is the product.** A 32-member burn-probability map from
   one broad prior, with no per-fire tuning, ties the hand-tuned single
   runs of Round 1 and is far better calibrated (E24). Every
   deterministic score earlier in this log is a lower bound.
2. **Learning as it burns is the headline mode.** Resample on yesterday's
   perimeter, mutate, keep simulating: +0.05 to +0.21 forecast IoU over
   the same ensemble without learning, on every fire including the
   holdout; Bear beats the Circle on days 2–4 (E25). Recommended
   operators β 10, σ 0.2, 20 % immigrants.
3. **The offline optimum was compensation.** With the state corrected
   daily the population settles on τ 5–20 d and wind × ≈ 1, not E20's
   τ 3 d and wind off. Use E20 as a diagnostic only.
4. **Ensembles are first-party.** First as
   `cella_lib::ensemble::WildfireEnsemble`, then (E31) as the model-
   agnostic `cella_lib::explore` engine with a `WildfireDriver`
   ([docs/explore.md](../../docs/explore.md)); runs are bit-reproducible
   for a given seed.
5. **The decay has a physical replacement** (E28): the FSim-style
   containment operator, each day a member is contained with probability
   sigmoid(a + b·ln growth), parameters learned by the filter, does the
   decay's job on all six fires with the decay off. E31 showed E28's +0.026 on Bear (0.482 vs 0.456) was run-to-run
   noise: containment-only and the decay
   are a tie everywhere, and the operator is kept for being the physical
   mechanism at no cost.
6. **Terrain wind is null until the kernel responds to wind** (E26):
   `cella_lib::wind_field` (6 s per run) moves scores by ±0.01. E19's
   5–10 % response is the bottleneck; E30 comes before any more wind
   work.
7. **Retardant as a paintable multiplier** (E27, `set_density`) has the
   same binary outcome as the fences of E18/E23: the representation is
   right, the placement rule is missing. Parked with the line agents.

Replicates (E25d, three seeds): Bear 0.468 ± 0.005, Brattain 0.404 ±
0.003, Buck 0.605 ± 0.009, Chimney 0.429 ± 0.004, Ferguson 0.349 ± 0.008,
Pier 0.535 ± 0.004: spread ≤ 0.02. E31 found the same configuration had
scored 0.456 and 0.473 on Bear under the old engine, so single-run
comparisons need a proper noise floor (→ E33).

Tooling added: `wildfire_smc.rs` (modes, env knobs `SMC_BETA`,
`SMC_SIGMA`, `SMC_IMMIGRANTS`, `SMC_ASSIM_EVERY`, `SMC_PRIOR`,
`SMC_WIND_ROT_DEG`, later `SMC_CONTAIN`, `SMC_TAU_OFF`), runners
`exp_smc.py`, `exp_smc_imm.py`, `exp_smc_horizon.py`, `exp_smc_reps.py`,
`exp_containment_op.py`, `exp_windfield.py`, `exp_retardant.py`. Cost:
Bear with 32 members ≈ 80 s; Brattain ≈ 4 min.

## Still open after this round

- The late-fire stall (Bear days 8+, the E21 mechanism): no parameter
  learning fixes it because the model has no way to stop *in place*.
- The fast-fire rate problem (Chimney, Ferguson; E19): assimilation
  narrows it, a kernel that stretches must close it. → E30.
- A perimeter-distance likelihood and reliability diagrams for the
  probability maps.
- Check the learned containment rate against ICS-209; a fuel term for
  the operator.
- The methods themselves: noise floor, ensemble size, operators, prior,
  offline fit vs filter. → Round 5.

## Configuration after this round

- **Recommended:** `wildfire_smc assim`, 32 members, β 10, σ 0.2,
  immigrants 0.2, containment operator on, decay off, the E25 broad
  prior; generic `explore` engine (E31). Forecast consensus IoU
  0.478 / 0.415 / 0.600 / 0.446 / 0.353 / 0.535 (E31 row).
- **Score family:** one-window-ahead consensus IoU and Brier. Not
  comparable with Rounds 1–3.
