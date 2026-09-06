# Round 2 — 2026-09-01: is the wind right, and why is the model too round?

_Score family: single-run mean IoU · mostly 3 seeds · calibration fires (front-speed note: all six) · terms: [GLOSSARY.md](GLOSSARY.md)_

## What we knew before

Round 1's recipes lose to the Circle on 3 of 4 calibration fires. The
model's fires are the wrong shape (ANALYSIS §5: too slow at the start,
unstoppable at the end). A GUI tester then saw "wind direction 0°" push
the fire **east** and expected north-to-south, the weather-report
convention. Before touching parameters again, this round audits the wind
path end to end and then asks what actually limits the shape of the burn.

## What we ran

| # | Question | Answer |
|---|---|---|
| [E9a](09-e9a-wind-convention-audit-code-converter-raster.md) | Is the wind convention consistent through code, converter and raster? | Yes. Home-made convention, but self-consistent; no result was mis-winded. NO BUG |
| [E9b](10-e9b-rotate-the-whole-wind-schedule.md) | Does one rotation of the wind win everywhere? | No. At ERA5 speeds wind is inert; at ×5 fires disagree; Chimney at ×5 rot 270° scores 0.571. FINDING |
| [E9c](11-e9c-is-chimney-s-270-a-lucky-angle.md) | Is Chimney's 270° a lucky angle? | No, a plateau 225–300° at ×3–5. Evidence about the input, not a score |
| [E10](12-e10-wind-direction-oracle-upper-bound-cheats.md) | With perfect daily direction, how much would the kernel buy? | At most +0.08 (Chimney); hurts Bear and Buck. FINDING |
| [—](13-observed-front-speed-vs-the-model-s-hard-cap.md) | How often do real fronts break the 1.5 km/day cap? | On the days that matter: 80–98 % of area on Brattain, Chimney, Ferguson. FINDING |
| [E12](14-e12-shape-how-round-is-the-model.md) | How round is the model? | Brattain 2.70 real vs 1.05 model. FINDING |
| [E11/b](15-e11-e11b-joint-steps-day-p0-burn-duration-scan.md) | Does raising the cap fix it (p0 re-scanned)? | No, ±0.03 for 50 → 400 ticks/day; p0 × ticks is one knob. REJECTED |
| [E13](16-e13-daily-temperature-schedule-tick-rate.md) | Does the temperature schedule hold at 3 seeds? | Yes, +0.06 Buck at either tick rate; tick rate adds nothing. Schedule KEPT |

## What we know now

1. **No wind bug, but a home-made convention.** "Toward, 0° = +x" was
   ours alone (Alexandridis defines only a relative angle; PyTorchFire
   uses toward/east/counter-clockwise; every weather source and
   operational simulator uses the bearing the wind comes *from*). Done
   the same day: the parameter is now `wind_from_deg` in the weather
   convention everywhere (model field, panel, configs, converter,
   scenario format v2). Old `wind_dir_deg` files are rejected on load.
   Every number in this log is unchanged: the conversion is exact
   (`toward = from + 90°`) and the six-fire reports were re-run to
   confirm.
2. **Wind input, not wind maths, is the weak link.** ERA5 daily domain
   means are too weak to act (E9b) and wrong on Chimney's run days (E9c).
   Codified as TEST_PLAN §2.1 and process rule 6: every source's wind
   convention, units, height and averaging get inspected and written into
   `provenance.weather` before any comparison is quoted.
3. **Resolution is not the limit.** E6, E11, E11b and E13 all say the
   same thing: p0 × ticks is one knob. No more runs on ticks per day.
4. **Time-varying drivers are the lever.** The temperature schedule is a
   3-seed-verified win on the fire it fits (Buck). Fast days and stopped days both need to become representable: promote the
   schedule into the model (roadmap §10.4) and add a higher-range driver (hourly wind, humidity).
5. **Shape diagnostics catch what IoU hides** (E12: Brattain 2.7 vs
   1.05). Worth adding elongation and growth direction to the report
   JSON.

Tooling added: `EXP_WIND_ROT_DEG` hook; runners `exp_wind_rotation.py`
(E9), `exp_wind_oracle.py` (E10), `exp_timeres.py` (E11). Shape and
speed diagnostics are one-off scripts whose outputs are kept in
`results/experiments/` (`obs_front_speed.json`, `exp12_shape.json`).

Gotcha that cost one run: the repo root and `cella_lib/` are separate
cargo build roots. `cargo run` inside `cella_lib/` writes to
`cella_lib/target/`; a stale `target/release/examples/wildfire_experiment`
at the repo root silently ran without the new hook (every rotation
scored identically, the tell). Runners now point at `cella_lib/target/`.

## Still open after this round

- Get hourly wind (ERA5 hourly or a station record) into the schedule
  and re-run E9b: if the rotation effect vanishes and Chimney improves
  at ×1, the kernel is vindicated. → E14.
- A driver with more day-to-day range than temperature. → E15.
- Why the model is round: what would make a large fire elongated? →
  E19, E37, E30.
- Fix the two-build-roots trap in the older `exp_*.py` runners (they are
  safe only by accident).

## Configuration after this round

Unchanged from Round 1 (E1 recipe + temperature schedule). The wind
parameter changed name and convention (`wind_from_deg`, scenario format
v2) with every score identical.
