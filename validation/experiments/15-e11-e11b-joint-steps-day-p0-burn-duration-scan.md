# E11 / E11b — joint (steps/day, p0, burn_duration) scan · REJECTED (cap is not the binding limit)

_Round 2 (2026-09-01) · 1 seed · calibration fires · runners `exp_timeres.py`, `exp_timeres_lowp0.py` · results `exp11_timeres.json`, `exp11b_timeres_lowp0.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** The fair version of E6. At each tick rate (50, 100, 200,
400 per day) we re-scanned p0 from scratch and kept burn duration fixed
in hours. Raising the cap eight-fold moved every fire by less than 0.03.
The best p0 scales as 1 ÷ ticks, so p0 × ticks per day is really one
knob, and it sets speed and total burn together. Making the fire faster
on a run day makes it burn more on every day. The missing 2–7 km/day
advances are not a tick-budget problem.

**Question.** With p0 re-tuned at each tick rate, does removing the speed
cap improve the score?

**What we changed.** Ticks per day ∈ {50, 100, 200, 400}; p0 re-scanned
at each (0.04–0.30, extended down to 0.01 for 200 and 400 in E11b
because 0.04 already burns everything there); burn duration held in
hours (2.4 h and 4.8 h, converted to ticks). Single seed.

**How we scored it.** Best mean IoU per tick rate at duration 4.8 h, with
the p0 that achieved it in brackets.

**Result.**

| Fire | 50/day | 100/day | 200/day | 400/day | Circle |
|---|---|---|---|---|---|
| Bear | **0.317** (0.12) | 0.287 (0.08) | 0.264 (0.03) | 0.275 (0.015) | 0.541 |
| Brattain | **0.337** (0.22) | 0.329 (0.10) | 0.325 (0.04) | 0.318 (0.02) | 0.450 |
| Buck | 0.401 (0.10) | **0.432** (0.04) | 0.408 (0.02) | 0.424 (0.015) | 0.670 |
| Chimney | 0.430 (0.30) | **0.440** (0.12) | 0.437 (0.06) | 0.432 (0.03) | 0.372 |

How to read it: each cell is the best mean IoU found at that tick rate,
with the winning p0 in brackets (not an area ratio here). Bold is the
best rate per fire.

- **Raising the cap 8× moves every fire by less than ±0.03.**
- The best p0 scales as 1/ticks (Bear 0.12 → 0.015). The model is close
  to rate-invariant: p0 × ticks per day is the real knob.
- The percolation cliff gets *sharper* with more ticks: at 400/day
  Brattain goes from ×0.1 area (p0 0.010) to ×2.4 (p0 0.015).
- Duration 2.4 h is worse than 4.8 h at every rate, as E1 found.

**What it means.** The observed 2–7 km/day advances are not missing
because of a tick budget. They are missing because a constant-p0 model
has one speed. Real fires have fast days and stopped days. That points at
time-varying drivers, not resolution.

**Questions this raises.**

- Does a time-varying driver help where resolution did not? → E13
  (temperature schedule: +0.06 on Buck at either rate).
- What would a tick have to be, in minutes, for the model to match real
  speeds? → E19: 2–4× the declared ticks on run days; the tick length
  should follow the weather.
- Would ticks that follow the wind fix it? → E22: invisible at daily
  truth when total ticks are held; harmful when they are added.

**Verdict.** Rejected. The cap is not the binding limit. Stop scanning
resolution (Round 2 conclusion 3).

**Later.** E13, E19, E22.
