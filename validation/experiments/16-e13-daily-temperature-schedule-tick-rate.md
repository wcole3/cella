# E13 — daily temperature schedule × tick rate · schedule KEPT, rate REJECTED

_Round 2 (2026-09-01) · 3 seeds · calibration fires · runner `exp_sched_timeres.py` · results `exp13_sched_timeres.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** Two questions in one run: does E3's temperature schedule
survive three seeds, and does it interact with tick rate? The schedule
holds: +0.06 on Buck at either rate, never a loss. Tick rate adds nothing
on top. The temperature proxy is weak (a few percent per degree), so
fires that grow 4–7 km in a day need a driver with far more day-to-day
range, such as hourly wind or humidity.

**Question.** Does the temperature schedule reproduce at 3 seeds, and
does a higher tick rate help once a time-varying driver is present?

**What we changed.** 50 vs 200 ticks per day, with and without the E3
`temp_vpd` schedule, at the E11 best p0 for each rate, duration 4.8 h.

**How we scored it.** Mean IoU, 3 seeds, four calibration fires.

**Result.**

| Fire | 50 const | 50 + temp | 200 const | 200 + temp |
|---|---|---|---|---|
| Bear | 0.311 | 0.313 | 0.263 | 0.271 |
| Brattain | 0.336 | 0.339 | 0.327 | 0.324 |
| Buck | 0.427 | **0.485** | 0.420 | 0.476 |
| Chimney | 0.437 | 0.440 | 0.439 | 0.436 |

How to read it: "const" is constant p0, "+ temp" is the daily temperature
multiplier. Bold is a new best for that fire.

- The schedule is worth +0.06 on Buck at either rate. Buck 0.485 is a new
  best (final IoU 0.44 vs 0.35). Never a loss elsewhere.
- Tick rate adds nothing on top of the schedule.

**What it means.** The E3 result is real. But the temperature proxy moves
p0 by about ±0.04 per °C of anomaly, which is a small range, and it helps
the slow fire (Buck). The fast fires need a driver that can change spread several-fold from
one day to the next, far more range than daily ERA5 means offer.

**Questions this raises.**

- Can hourly station weather supply that range? → E14 (wind: no), E15
  (moisture from humidity and temperature: slows but cannot stop).
- Does a periodic driver of any strength cap the burn? → E15, E16: no; a
  one-way decay does.

**Verdict.** Schedule kept, tick rate rejected. Round 2 conclusion 3:
stop scanning resolution.

**Later.** E14, E15, E16, E17 (the moisture cycle is kept alongside the
decay at no cost).
