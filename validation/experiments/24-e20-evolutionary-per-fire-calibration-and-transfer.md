# E20 — evolutionary per-fire calibration, then a transfer test · finding: three parameters transfer, two do not

_Round 3 (2026-09-02) · 1 seed in search, 3 seeds verified · all six fires incl. holdout · runner `exp_evolve.py` (scipy differential evolution, 8 workers) · results `exp20_evolve.json` · search space TEST_PLAN v1.3 · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** The model is fast (5–7 s per fire-run), so we let an optimiser search six knobs
separately on each calibration fire. The per-fire winners are *not* the
result; optimising against the answer is easy. The result is what the
four independent searches agree on, and whether the median of their
recipes transfers to the holdout fires. Three knobs agree: a short decay
(about 3 days), a long burn duration (15), and the station wind turned
down. That median recipe lifts the never-tuned Pier fire from 0.32 to
0.51. Fuel ratios and the fast-fire regime do not transfer.

**Question.** Which knob values do independent per-fire searches agree
on, and does their median transfer to fires no search saw?

**What we changed.** scipy differential evolution, popsize 6 × 6 genes, 12
generations (≈ 470 evaluations per fire, 1 seed each), objective = mean
IoU over the observation series. Genome: p0, burn duration, containment
τ (log), hourly-station-wind multiplier, moisture of extinction M_x
(≥ 150 ≈ off), grass:timber veg ratio (log). Winners re-run with 3 seeds.
Transfer recipe = per-gene median of the four winners, run with 3 seeds
on all six fires.

**How we scored it.** Mean IoU, 3 seeds; per-fire optima reported only
next to the transfer recipe.

**Result.** Per-fire winners (3-seed verified):

| Fire | p0 | dur | τ (days) | wind × | M_x | grass:timber | mean IoU | prev. best | Circle | area |
|---|---|---|---|---|---|---|---|---|---|---|
| Bear | 0.55 | 15 | 3.1 | 0.34 | off | 3.7 | **0.480** | 0.427 | 0.541 | ×0.66 |
| Brattain | 0.53 | 16 | 3.7 | 0.15 | off | 1.3 | **0.408** | 0.407 | 0.450 | ×0.81 |
| Buck | 0.35 | 15 | 2.5 | 0.77 | off | 1.2 | **0.581** | 0.541 | 0.670 | ×0.80 |
| Chimney | 0.37 | 3 | 48 (off) | 0.24 | 82 | 0.3 | **0.474** | 0.446 | 0.372 | ×0.71 |

How to read it: one row per fire, the six knob values the search chose,
then its 3-seed mean IoU against the previous best for that fire. These
scores peeked at the whole series; they are an upper bound, not a
forecast.

Transfer recipe (median: p0 0.45, dur 15, τ 3.4 d, wind ×0.29, M_x off,
grass:timber 1.2), 3 seeds, all six fires:

| Fire | control (Round-1 global) | E16c global + decay | **E20 transfer** | Circle |
|---|---|---|---|---|
| Bear | 0.220 | 0.319 | **0.452** | 0.541 |
| Brattain | 0.308 | 0.381 | 0.369 | 0.450 |
| Buck | 0.369 | 0.455 | **0.466** | 0.670 |
| Chimney | 0.421 | 0.393 | 0.391 | 0.372 |
| **Ferguson (holdout)** | 0.145 | 0.156 | 0.158 | 0.373 |
| **Pier (holdout)** | 0.321 | 0.463 | **0.512** | 0.559 |

How to read it: three global recipes side by side; bold is the best per
fire; bold fire names are the holdout pair. This table is honest: one
setting for all fires, no per-fire choice.

What the four searches agree on:

1. **Short containment time-scale, long burn duration.** Three of four
   fires land at τ = 2.5–3.7 days and dur = 15–16 ticks; the Round 1
   dur 5/10 was too short. A long burn duration keeps the front alive
   while the decay throttles it.
2. **Turn the station wind down.** Every fire chose a multiplier < 0.8,
   two of them < 0.25. Given E14 and E19 that is not "wind does not
   matter"; it is "this wind, from a valley airport, into this kernel,
   hurts".
3. **Moisture off** on three of four fires. At daily truth the damping
   only moves cells within the day; the search sees no reward. (E17 showed it is harmless and helps
   final-day IoU; the search objective, mean IoU, does not weight that.)
4. **The fast fire is a different regime.** Chimney wants no decay, a
   very short burn duration (3), and timber over grass. Under-burning
   fires need speed, not stopping; the same split as E16c.
5. **Fuel ratios do not transfer** (0.3 → 3.7). Not a parameter to fix
   globally.

**What it means.** The transfer recipe beats every previous global on
Bear (+0.13 over E16c) and on the Pier holdout (0.512 from 0.463), ties
Buck, and loses slightly on Brattain and Chimney (under-burn: area ×0.48
and ×0.59; τ too short for them). Ferguson is unchanged at 0.16: nothing
in this space helps a fire the model cannot keep up with. Five of six
still lose to the Circle; the gap is now 0.05–0.09 on Bear, Brattain and
Pier.

Caveats: 1-seed search noise (search vs verified differ by ≤ 0.02); p0 is
now 2–5× the Alexandridis value, so the decay is doing a lot of work; the
space was chosen after Round 3's findings, recorded as TEST_PLAN v1.3.

**Questions this raises.**

- Is the offline optimum compensating for the model's early-days error?
  → E25 finding 6: yes. With the state corrected daily, the filter
  settles on τ 5–20 d and wind × ≈ 1, not τ 3 d and wind off.
- Head to head, does a filter that learns day by day beat a fit to the
  start? → E36: yes, on every fire, by 0.025–0.104.
- What is the decay standing in for on the first days? → E21: not
  crews. Still open.

**Verdict.** Finding. Use the optimiser as a diagnostic, never as the
headline.

**Later.** E25, E36 (offline fitting retired as a forecaster), E35 (never
tune the prior toward past posteriors: "the E20 mistake in a new
costume").
