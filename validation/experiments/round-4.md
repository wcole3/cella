# Round 4 — 2026-09-04: ensembles — Monte Carlo maps and a fire that learns as it burns

Question set by the user after Round 3: the model is fast, so use that.
Can an *ensemble* of runs approximate a fire better than any single run,
and can generations of simulations develop — the evolutionary idea from
E20, but on-line, while the fire is burning?

**Design (one Rust example, `cella_lib/examples/wildfire_smc.rs`)**

- **Open mode = Monte Carlo (E24).** M members drawn from one broad prior
  (p0 log-uniform 0.08–0.6, burn duration 5–20, containment τ log-uniform
  2–100 d, wind multiplier 0–1.5), each with its own RNG seed, run
  independently. Output per observation time: the fraction of members in
  which each cell has burned — a burn-probability map — scored by Brier
  score, consensus IoU (probability ≥ 0.5), best-threshold IoU
  (diagnostic; it peeks) and mean member IoU, next to persistence and the
  Circle. This is the "burn probability" product operational simulators
  (PROPAGATOR, FSim) ship.
- **Assim mode = particle filter with GA operators (E25).** Same members.
  At each observation time every member's *current* burned set is scored
  against the observed mask; members are resampled with weights
  exp(β·IoU) (systematic resampling); each child inherits its parent's
  grid state — you cannot re-draw the past — with **mutated** parameters
  (log-normal jitter, σ) and a fresh seed; optionally a fraction of
  children are **immigrants** with fresh prior parameters (diversity floor);
  then everyone keeps simulating from where they are. Every score at t_k
  is a **one-window-ahead forecast** made from the state assimilated at
  t_{k−1}; the truth at t_k is only used after it has been scored. That is
  how an operational nowcast would use yesterday's perimeter, and it is
  the honest way to use truth (TEST_PLAN v1.4).
- Nothing is chosen per fire, so the holdout pair is scored alongside the
  calibration four.

**Literature.** Sequential Monte Carlo for fire spread: Xue, Gu & Hu
(ACM TOMACS 2012) assimilated fire-front observations into DEVS-FIRE with
particle filters; Rochoux et al. (NHESS 2014/15) and Zhang et al. (2017)
did the same for a Rothermel/level-set model with ensemble Kalman and
particle filters; PyTorchFire (2025) calibrates parameters by gradient on
the observed mask over the first days. All report that assimilation
corrects both the front position and the parameters, and all note the
degeneracy/collapse problem — hence the immigrants operator (standard GA
practice) tested in E25b.

## Results (32 members, one untuned prior, all six fires, forecasts)

| Fire | open consensus (E24) | assimilating + immigrants (E25) | best tuned single run | Circle |
|---|---|---|---|---|
| Bear | 0.399 | **0.473** | 0.480 (E20 per-fire) | 0.541 |
| Brattain | 0.330 | **0.400** | 0.408 | 0.450 |
| Buck | 0.481 | **0.616** | 0.581 | 0.670 |
| Chimney | 0.382 | **0.426** | 0.474 | 0.372 |
| Ferguson (holdout) | 0.131 | **0.345** | 0.158 (transfer) | 0.373 |
| Pier (holdout) | 0.478 | **0.533** | 0.512 (transfer) | 0.559 |

## Round 4 conclusions → what to do next

1. **The ensemble is the product.** A 32-member burn-probability map from
   one broad prior, with no per-fire tuning, ties the hand-tuned single
   runs of Round 1 and is far better calibrated (E24). Every deterministic
   score earlier in this log is a lower bound.
2. **Learning as it burns is the headline mode.** Resample on yesterday's
   perimeter, mutate, keep simulating: +0.05 to +0.21 forecast IoU over
   the same ensemble without learning, on every fire including the
   holdout; Ferguson finally moves (0.13 → 0.34); Bear beats the Circle on
   days 2–4 (E25). Recommended operators β 10, σ 0.2, 20 % immigrants.
3. **The offline optimum was compensation.** With the state corrected
   daily the population settles on τ 5–20 d and wind × ≈ 1, not E20's
   τ 3 d / wind off. Use E20 as a diagnostic only.
4. **Still open:** the late-fire stall (Bear days 8+, the E21 mechanism),
   which no parameter learning fixes because the model has no way to
   stop *in place*; the fast-fire rate problem (Chimney, E19) which
   assimilation narrows but a kernel that stretches must close; and
   a perimeter-distance likelihood and reliability
   diagrams for the probability maps.
5. **Ensembles are first-party now.** `cella_lib::ensemble::WildfireEnsemble`
   (config block `"ensemble"`, `CellaConfig::build_ensemble`, guide in
   `docs/ensemble.md`); members share the slope table; the SMC example is
   a thin scorer on top of it.
6. **The decay has a physical replacement** (E28): the FSim-style
   containment-probability operator — each day a member is contained with
   probability sigmoid(a + b·ln growth), parameters learned by the filter —
   matches or beats the τ decay on all six fires with the decay switched
   off (Bear 0.482 vs 0.456). Recommended default for ensembles.
7. **Terrain wind is null until the kernel responds to wind** (E26): the
   mass-consistent downscaler (`cella_lib::wind_field`, 6 s per run via a
   two-solve basis on a coarsened grid) moves scores by ±0.01. E19's
   5–10 % wind–rate response is the bottleneck; E30 (kernel refit) comes
   before any more wind work.
8. **Retardant as a paintable multiplier** (E27, `set_density`) has the
   same binary outcome as the fences of E18/E23: the representation is
   right, the agent's placement rule is what is missing. Parked with the
   line agents; ready for observed drop/line locations.

Replicates (E25d, three seeds): Bear 0.468±0.005, Brattain 0.404±0.003, Buck 0.605±0.009, Chimney 0.429±0.004, Ferguson 0.349±0.008, Pier 0.535±0.004 — spread ≤ 0.02.

**Tooling added:** `wildfire_smc.rs` (modes, env knobs `SMC_BETA`,
`SMC_SIGMA`, `SMC_IMMIGRANTS`, `SMC_ASSIM_EVERY`, `SMC_PRIOR`,
`SMC_WIND_ROT_DEG`), runners `exp_smc.py`, `exp_smc_imm.py`,
`exp_smc_horizon.py`. Cost: Bear with 32 members ≈ 80 s; Brattain ≈ 4 min.
