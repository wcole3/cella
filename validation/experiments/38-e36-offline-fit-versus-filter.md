# E36 — fit the first three days, then forecast · finding — the filter beats the offline GA on every fire; a three-day fit overfits to the box edges

_Round 5 (2026-09-05) · 1 seed · all six fires incl. holdout · runner `exp_r5_offline.py` → `wildfire_smc evolve` · results `exp36_offline.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** E20 fitted knobs with an optimiser that saw the whole fire.
E25 argued that a filter learning day by day is the honest version. This
is the head-to-head: a genetic algorithm sees only the first three days,
as an operations room would, and its best knob set is run forward and
scored as a forecast on days 4 onward against the filter on the same
days. The filter wins on every fire, by 0.025 to 0.104. Three days is
enough to fit and not enough to learn: the GA drove knobs to the edges of
their boxes to reproduce the early growth.

**Question.** Is learning as it burns better than fitting the start and
extrapolating?

**What we ran.** A genetic algorithm (population 24, 20 generations, 2
repeats, the same genes and weather schedule as the filter, the library's
`Evolution` with the wildfire driver) maximises mean IoU against the
first **three** observed perimeters. The winning genome is then run
forward as a 32-member *open* ensemble (every member the fitted knobs
with its own dice) and scored on every observation. Days 4 onward are
compared with the filter's forecasts on the same days (E33 seed 0).

**How we scored it.** Mean one-window-ahead consensus IoU on days 4+,
GA ensemble vs filter; Δ judged against E33's sd.

![Six small line charts of per-day forecast IoU: GA fitted on the shaded days then run forward, against the filter](figures/e36-offline.svg)

**Result.**

| Fire | fit IoU (days 1–3) | GA forward, days 4+ | filter, days 4+ | Δ | fitted p0 / duration / wind × |
|---|---|---|---|---|---|
| Bear | 0.590 | 0.418 | 0.470 | **−0.052** | 0.096 / 20 / 0.0 |
| Brattain | 0.363 | 0.387 | 0.431 | **−0.045** | 0.476 / 5 / 1.5 |
| Buck | 0.612 | 0.542 | 0.602 | **−0.061** | 0.121 / 20 / 1.5 |
| Chimney | 0.551 | 0.323 | 0.427 | **−0.104** | 0.323 / 5 / 0.0 |
| Ferguson* | 0.213 | 0.352 | 0.377 | **−0.025** | 0.600 / 16 / 0.13 |
| Pier* | 0.525 | 0.456 | 0.542 | **−0.085** | 0.080 / 19 / 1.5 |

How to read it: "fit IoU" is how well the GA matched the three days it
saw (not a forecast). The next two columns are forecasts on the same
later days; Δ = GA − filter, negative means the filter wins. Every Δ is
past the fire's E33 sd. `*` is the holdout pair. The last column shows
the knobs the GA chose; compare with the prior bounds p0 0.08–0.6,
duration 5–20, wind × 0–1.5. Brier: the GA ensemble is better on
Chimney, Ferguson and Pier and worse on Bear, Brattain and Buck. The
GA's fit score stopped improving after generation 8 on every fire.

- **Learning as it burns wins on every fire, by 0.025 to 0.104**,
  holdout included; the gap is two to eight times the noise.
- **Three days is enough to fit and not enough to learn.** Wind × is
  0.0 or 1.5 on five fires, burn duration 5 or 20 on four, p0 0.08 or
  0.60 on two: the GA drove the knobs to the box edges, because with
  three perimeters the objective rewards whatever reproduces the *early*
  growth. Chimney is the clean case: the fit put the containment
  intercept at its maximum (−1) so members stop almost at once, matched
  the slow first days (0.551), and then forecast a fire that stops while
  the real one runs: 0.323 forward, flat at 0.23 for the last week.
- **The one place the GA leads is the first forecast day on Ferguson**
  (0.33 vs 0.24 on day 4, 0.36 vs 0.32 on day 5). The fit found the high
  p0 Ferguson needs immediately; the filter takes until day 7 and then
  overtakes.
- The two cost about the same: 24 × 2 × 3 days per generation for twenty
  generations is roughly one 32-member run. The filter spends its compute
  on all thirty days; the GA re-simulates the first three.

**What it means.** E25's argument is now a measurement: the filter is not
a slow route to E20's knobs, it is a better forecaster than any fixed
knob set fitted to the start. Offline fitting stays useful as a
diagnostic of what the model *can* match.

**Questions this raises.**

- Would a fitted *start* for the filter (initialise the population near
  a short fit, then learn) buy a day or two of skill on Ferguson-like
  fires without paying later? Open; the obvious hybrid.

**Verdict.** Retire per-fire offline fitting as a forecasting method.

**Later.** Round 5 conclusion 5.
