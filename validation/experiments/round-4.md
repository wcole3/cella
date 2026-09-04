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

**Tooling added:** `wildfire_smc.rs` (modes, env knobs `SMC_BETA`,
`SMC_SIGMA`, `SMC_IMMIGRANTS`, `SMC_ASSIM_EVERY`, `SMC_PRIOR`,
`SMC_WIND_ROT_DEG`), runners `exp_smc.py`, `exp_smc_imm.py`,
`exp_smc_horizon.py`. Cost: Bear with 32 members ≈ 80 s; Brattain ≈ 4 min.
