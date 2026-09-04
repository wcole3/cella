# E14 — hourly station wind (NOAA ISD) instead of ERA5 daily means · REJECTED (as a drop-in)

_Round: Round 3 — 2026-09-02: real weather in, explosion out_

**Question.** Round 2 showed the ERA5 daily domain-mean wind is too weak to
act and, on Chimney, points the wrong way on the run days. Does a real
hourly observation fix that?

**Input.** New loader `scripts/wind_station.py` pulls the nearest NOAA
Integrated Surface Database station-year CSV (no API key), parses hourly
wind direction (already the weather-report "from" bearing), speed (m/s),
temperature and dew point (→ RH), fills gaps by vector interpolation, and
writes `station_hourly.json` beside each scenario with a full
`provenance.weather` block (TEST_PLAN §2.1). Stations found:

| Fire | Station | Distance | Raw wind coverage | Mean / max wind |
|---|---|---|---|---|
| Bear | Oroville Municipal | 58 km | 76 % | 2.2 / 7.2 m/s |
| Brattain | Lake County Airport | 52 km | 89 % | 3.8 / 12.6 m/s |
| Buck | Red Bluff Municipal | 66 km | ≥ 100 % | 4.2 / 14.4 m/s |
| Chimney | Paso Robles Municipal | 41 km | 67 % | 3.5 / 9.8 m/s |

All are valley airports 40–70 km from the fire. Meteostat found nothing
closer; RAWS fire-weather stations would be closer but need a MesoWest
token. The station winds are 2–4× the ERA5 daily means, with a clear
diurnal cycle (Chimney RH 24 % by day, 76 % at night).

**Runs.** `exp_station.py wind`: E1 (p0, dur) recipes, 3 seeds, the ERA5
daily schedule vs the hourly station schedule scaled ×1 (as measured),
×0.5 (≈ mid-flame adjustment) and ×2 (gust / ridge proxy).

| Fire | ERA5 daily | station ×1 | ×0.5 | ×2 | Circle |
|---|---|---|---|---|---|
| Bear | **0.311** | 0.292 | 0.305 | 0.251 | 0.541 |
| Brattain | **0.336** | 0.320 | 0.329 | 0.299 | 0.450 |
| Buck | **0.403** | 0.360 | 0.390 | 0.331 | 0.670 |
| Chimney | 0.441 | **0.443** | 0.441 | 0.380 | 0.372 |

(mean IoU; area ratios shrink with wind strength: Chimney ×2.1 → ×1.8 →
×1.2.)

**Findings.**

- A real hourly wind is *not* a drop-in improvement. At ×1 it is flat on
  Chimney and 0.02–0.04 worse elsewhere; stronger wind always narrows the
  burn and loses. Same pattern as E5 and E10: this kernel gains direction
  by losing area, and p0 was tuned at ERA5 speed.
- On Chimney's run days (Aug 20–21) Paso Robles reports 2–7 m/s from the
  SW–W, not the easterly gusts the incident reports describe. A valley
  airport 41 km inland does not see the coastal-range wind either. The
  E9c result (rotated ERA5 scoring 0.57) therefore still has no
  observational input that reproduces it; that needs a ridge-top RAWS or
  a downscaled wind field (WindNinja), not a different point record.
- The loader itself is the keeper: it is the first real observed weather
  in the pipeline, it carries RH and temperature per hour, and it is what
  E15 builds on.

**Verdict.** Station wind as a drop-in: rejected. Station *weather* as
input to a moisture model: see E15.
