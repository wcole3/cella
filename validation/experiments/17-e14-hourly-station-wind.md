# E14 — hourly station wind (NOAA ISD) instead of ERA5 daily means · REJECTED as a drop-in; loader KEPT

_Round 3 (2026-09-02) · 3 seeds · calibration fires · runner `exp_station.py wind`, loader `scripts/wind_station.py` · results `exp14_station.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** Round 2 showed the daily-average wind is too weak to act
and, on Chimney, points the wrong way on the run days. So we fetched real
hourly weather from the nearest airport station for each fire and used it
instead. As a drop-in it did not help: flat on Chimney, 0.02–0.04 worse
elsewhere, and stronger versions always narrowed the burn and lost. The
stations are valley airports 40–70 km from the fires and do not see the
ridge winds that drove them. The loader is the keeper: it is the first
observed weather in the pipeline and carries hourly humidity and
temperature, which E15 uses.

**Question.** Does real hourly wind fix what the daily mean could not?

**What we changed.**

- New loader `scripts/wind_station.py`: pulls the nearest NOAA Integrated
  Surface Database station-year CSV (no API key), parses hourly wind
  direction (already the weather-report from-bearing), speed (m/s),
  temperature and dew point (→ relative humidity), fills gaps by vector
  interpolation, and writes `station_hourly.json` beside each scenario
  with a full `provenance.weather` block (TEST_PLAN §2.1).
- Runs: E1 recipe, 3 seeds, ERA5 daily schedule vs the hourly station
  schedule scaled ×1 (as measured), ×0.5 (about the mid-flame adjustment)
  and ×2 (gust or ridge proxy).

Stations found:

| Fire | Station | Distance | Raw wind coverage | Mean / max wind |
|---|---|---|---|---|
| Bear | Oroville Municipal | 58 km | 76 % | 2.2 / 7.2 m/s |
| Brattain | Lake County Airport | 52 km | 89 % | 3.8 / 12.6 m/s |
| Buck | Red Bluff Municipal | 66 km | ≥ 100 % | 4.2 / 14.4 m/s |
| Chimney | Paso Robles Municipal | 41 km | 67 % | 3.5 / 9.8 m/s |

How to read it: "coverage" is the share of hours with a raw wind record
before gap-filling. Station winds are 2–4× the ERA5 daily means, with a
clear day/night cycle (Chimney humidity 24 % by day, 76 % at night).
Meteostat found nothing closer; RAWS fire-weather stations would be
closer but need a MesoWest token.

**How we scored it.** Mean IoU, 3 seeds, four calibration fires; area
ratio watched alongside.

**Result.**

| Fire | ERA5 daily | station ×1 | ×0.5 | ×2 | Circle |
|---|---|---|---|---|---|
| Bear | **0.311** | 0.292 | 0.305 | 0.251 | 0.541 |
| Brattain | **0.336** | 0.320 | 0.329 | 0.299 | 0.450 |
| Buck | **0.403** | 0.360 | 0.390 | 0.331 | 0.670 |
| Chimney | 0.441 | **0.443** | 0.441 | 0.380 | 0.372 |

How to read it: mean IoU, higher is better; bold is the best wind input
per fire. Area ratios shrink with wind strength: Chimney ×2.1 → ×1.8 →
×1.2 across ERA5, station ×1, station ×2.

- A real hourly wind is *not* a drop-in improvement. At ×1 it is flat on
  Chimney and 0.02–0.04 worse elsewhere; stronger wind always narrows
  the burn and loses. Same pattern as E5 and E10: this kernel gains
  direction by losing area, and p0 was tuned at ERA5 speed.
- On Chimney's run days (Aug 20–21) Paso Robles reports 2–7 m/s from the
  SW–W, not the easterly gusts the incident reports describe. A valley
  airport 41 km inland does not see the coastal-range wind either.

**What it means.** The E9c result (rotated ERA5 scoring 0.57) still has
no observational input that reproduces it. Reproducing it needs a
ridge-top fire-weather station or a wind field downscaled over the
terrain, not a different point record. And a stronger wind of any origin
hurts until the kernel's response to wind changes (E19).

**Questions this raises.**

- Can the station's hourly humidity and temperature drive a fuel-
  moisture model? → E15: it slows the fire but cannot stop it.
- Would a terrain-downscaled wind field see the ridge wind? → E26: built;
  changes nothing at this kernel.
- Why does every real wind hurt? → E19: the kernel's speed barely
  responds to wind; extra wind just narrows the burn.

**Verdict.** Station wind as a drop-in: rejected. Station *weather* as
input to a moisture model: E15. The loader stays.

**Later.** E15, E17 (station wind ×1 is part of the E17 recipe), E20 (the
optimiser turned the station wind down on every fire), E26, E30 (kernel
refit, not yet run).
