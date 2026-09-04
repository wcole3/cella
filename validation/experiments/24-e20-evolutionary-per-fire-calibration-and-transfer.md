# E20 — evolutionary per-fire calibration, then a transfer test · finding: three parameters transfer, two do not

_Round: Round 3 — 2026-09-02: real weather in, explosion out_

**Why.** The model is fast (5–7 s per fire-run), so a population search
is affordable. The risk is obvious — optimising against the answer — so
the per-fire optima are *not* the result. The result is what the four
independent searches agree on, and whether the median recipe transfers
to the two holdout fires that no search ever saw. Search space
pre-registered in TEST_PLAN v1.3.

**Method.** `exp_evolve.py`: scipy differential evolution, popsize 6 × 6
genes, 12 generations (≈ 470 evaluations per fire, 1 seed each,
8 workers), objective = mean IoU over the observation series. Genome:
p0, burn duration, containment τ (log), hourly-station-wind multiplier,
moisture of extinction M_x (≥ 150 ≈ off), grass:timber veg ratio (log).
Winners re-run with 3 seeds. Transfer recipe = per-gene median of the four
winners, run with 3 seeds on all six fires.

**Per-fire winners (3-seed verified)**

| Fire | p0 | dur | τ (days) | wind × | M_x | grass:timber | mean IoU | prev. best | Circle | area |
|---|---|---|---|---|---|---|---|---|---|---|
| Bear | 0.55 | 15 | 3.1 | 0.34 | off | 3.7 | **0.480** | 0.427 | 0.541 | ×0.66 |
| Brattain | 0.53 | 16 | 3.7 | 0.15 | off | 1.3 | **0.408** | 0.407 | 0.450 | ×0.81 |
| Buck | 0.35 | 15 | 2.5 | 0.77 | off | 1.2 | **0.581** | 0.541 | 0.670 | ×0.80 |
| Chimney | 0.37 | 3 | 48 (off) | 0.24 | 82 | 0.3 | **0.474** | 0.446 | 0.372 | ×0.71 |

**Transfer recipe** (median: p0 0.45, dur 15, τ 3.4 d, wind ×0.29, M_x off,
grass:timber 1.2), 3 seeds, all six fires:

| Fire | control (Round-1 global) | E16c global + decay | **E20 transfer** | Circle |
|---|---|---|---|---|
| Bear | 0.220 | 0.319 | **0.452** | 0.541 |
| Brattain | 0.308 | 0.381 | 0.369 | 0.450 |
| Buck | 0.369 | 0.455 | **0.466** | 0.670 |
| Chimney | 0.421 | 0.393 | 0.391 | 0.372 |
| **Ferguson (holdout)** | 0.145 | 0.156 | 0.158 | 0.373 |
| **Pier (holdout)** | 0.321 | 0.463 | **0.512** | 0.559 |

**Learnings — what the four searches agree on**

1. **Short containment time-scale, long burn duration.** Three of four
   fires land at τ = 2.5–3.7 days and dur = 15–16 ticks; the Round-1 dur
   5/10 was too short. A long burn duration keeps the front alive while
   the decay throttles it — the two knobs work as a pair.
2. **Turn the station wind down.** Every fire chose a wind multiplier
   < 0.8, two of them < 0.25. Given E14 and E19 that is not "wind does not
   matter"; it is "this wind, from a valley airport, into this kernel,
   hurts". The optimiser confirms the diagnosis rather than the physics.
3. **Moisture off** on three of four fires. At daily truth the damping only
   moves cells within the day; the search sees no reward and drops it.
   (E17 showed it is harmless and helps final-day IoU; the search
   objective — mean IoU — does not weight that.)
4. **The fast fire is a different regime.** Chimney wants no decay, a very
   short burn duration (3), and timber over grass. Under-burning fires
   need speed, not stopping — the same split as E16c.
5. **Fuel ratios do not transfer** (0.3 → 3.7). Either the six-class
   grouping is too coarse or the ratio is compensating for something
   local (terrain, wind error). Not a parameter to fix globally.

**Transfer verdict.** The median recipe beats every previous global on
Bear (+0.13 over E16c) and on the **Pier holdout (0.512, from 0.463; the
control was 0.321)**, ties Buck, and loses slightly on Brattain and
Chimney (under-burn: area ×0.48 / ×0.59 — the τ is too short for them).
Ferguson is unchanged at 0.16: nothing in this search space can help a
fire the model cannot keep up with. Five of six still lose to the Circle;
the gap is now 0.05–0.09 on Bear, Brattain, Pier.

**Caveats.** 1-seed search noise (search vs verified differ by ≤ 0.02);
p0 is now at ×2–5 the Alexandridis value, i.e. the decay is doing a lot
of work; and 470 evaluations × 4 fires is well inside the §6 budget but
the space was chosen after Round 3's findings, not before Round 1 —
recorded as v1.3 for that reason.
