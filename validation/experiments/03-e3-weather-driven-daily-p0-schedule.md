# E3 — weather-driven daily p0 schedule · KEPT (first physics win)

_Round 1 (2026-08-15) · 1 seed · calibration fires · runners `make_p0_schedules.py`, `exp_variants.py` · raw results not kept in `results/` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** Real fires slow down on cool or rainy days; the model's p0
is the same every day. We built daily multipliers on p0 from the weather
reanalysis (a rain factor, a temperature factor, and both together) and
applied them per day. Temperature never hurt and lifted Buck by 0.05.
Rain is strong medicine: it fixed Bear's over-burn but starved Buck,
which was already under-burning. This was the first change that added
physics rather than tuning a constant.

**Question.** Does letting p0 follow daily weather improve the score?

**What we changed.** Three schedules from ERA5 layers, applied per wind
window through the harness hook `EXP_P0_SCALE`:

- rain: p0 × exp(−k × wet), k = 1, with one day of carry-over;
- temperature: p0 × (1 + 0.04 × (T − mean)), clamped to [0.4, 1.6];
- both multiplied.

Base settings: the E1 recipe per fire.

**Why we expected it to matter.** The area curves (ANALYSIS §5) show real
fires with fast days and stalled days. A constant p0 has one speed.

**How we scored it.** Mean IoU, one seed, four calibration fires, against
the E1 control.

**Result.**

| Fire | control | rain k=1 | temp | rain×temp |
|---|---|---|---|---|
| Bear | 0.317 | 0.350 | 0.318 | **0.353** |
| Brattain | 0.337 | 0.344 | 0.339 | **0.346** |
| Buck | 0.414 | 0.332 | **0.460** | 0.356 |
| Chimney | 0.435 | 0.439 | 0.441 | **0.441** |

How to read it: mean IoU, higher is better; bold is the best schedule for
that fire. Control is the E1 recipe with constant p0.

- Temperature never hurts and is worth +0.05 on Buck.
- Rain collapsed Bear's over-burn from ×3.3 to ×0.3 of the observed area:
  two rainy days carry real stopping information. The same factor
  over-suppresses Buck (0.414 → 0.332).
- Rain × temperature is best on three fires by small margins.

**What it means.** A time-varying driver moves the score where a constant
cannot. The gains are small at one seed, but the direction is right, and
the rain result shows that a factor which *removes* spread on some days
can fix an over-burn. It also shows the risk: the same factor hurts a
fire the model already under-burns. The two failure directions want
different medicine.

**Questions this raises.**

- Does the temperature gain survive more seeds? → E13: yes, +0.06 on
  Buck at 3 seeds, at either tick rate.
- Would hourly humidity and temperature (a proper fuel-moisture proxy) do
  more than daily means? → E15: it slows the fire but cannot stop it.
- Should the schedule live in the model rather than the harness? → Round
  3 added `WildfireModel::set_p0`; a model-side schedule input is still on the roadmap (§10.4).

**Verdict.** Kept. The temperature schedule joins the working recipe. Rain
stays optional because it hurts under-burning fires.

**Later.** An engine bug almost hid this result: p0 is baked in when the
model attaches to the grid, so the first E3 run silently changed nothing
([round-1.md](round-1.md), engine findings). Round 1 verification at 3
seeds: Buck 0.448 with the schedule. E13 confirmed the gain. E15 and E16
showed that periodic damping cannot cap the burn and a one-way decay can.
