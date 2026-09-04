# E19 — what is one tick in real time? The model's own rate of spread · finding (data)

_Round: Round 3 — 2026-09-02: real weather in, explosion out_

**Question.** The scenario format declares 50 ticks/day (copied from the
papers on this dataset) but nothing ties a tick to a physical duration.
Freire & DaCamara (2019) fixed theirs by simulating until the observed
area burned and dividing the observed time by the mean tick count
(→ 20 min). We do the cleaner version: measure the front speed the model
actually produces, in cells per tick, as a function of p0, wind and burn
duration, then ask what tick length would make it match reality.

**Method.** New example `cella_lib/examples/wildfire_ros.rs`: 240 × 120
homogeneous fuel (veg 1, density 1, flat), a burning column at x = 0..2 so
the front is a straight line, wind from 270° (blowing +x) or calm. Front
position = mean over rows of the rightmost burning/burned cell; speed =
slope of a linear fit after a 20-tick warm-up; 3 seeds. Output
`results/experiments/exp19_ros.json`.

**Front speed, cells per tick** (dur 5 | dur 10; columns wind 0 / 2 / 5 / 8 m/s):

| p0 | calm | 2 m/s | 5 m/s | 8 m/s | | calm | 2 | 5 | 8 |
|---|---|---|---|---|---|---|---|---|---|
| 0.05 | 0 | 0 | 0 | 0 | | 0.22 | 0.23 | 0.23 | 0.24 |
| 0.08 | 0.31 | 0.19 | 0 | 0 | | 0.34 | 0.35 | 0.36 | 0.38 |
| 0.10 | 0.40 | 0.41 | 0.42 | 0.43 | | 0.42 | 0.41 | 0.43 | 0.44 |
| 0.12 | 0.47 | 0.48 | 0.50 | 0.52 | | 0.48 | 0.48 | 0.50 | 0.52 |
| 0.16 | 0.57 | 0.59 | 0.60 | 0.63 | | 0.57 | 0.59 | 0.61 | 0.63 |
| 0.22 | 0.70 | 0.71 | 0.73 | 0.76 | | 0.70 | 0.71 | 0.73 | 0.76 |
| 0.30 | 0.82 | 0.83 | 0.86 | 0.89 | | same | | | |
| 0.44 | 0.96 | 0.97 | 0.98 | 0.99 | | same | | | |
| 0.58 | 1.00 | 1.00 | 1.00 | 1.00 | | same | | | |

**Findings.**

1. **Rate is set by p0, not by wind.** From calm to 8 m/s the front
   speeds up by 5–10 % at any p0. In Rothermel-type models rate of spread
   grows roughly as wind^1.5–2; a 5 m/s wind doubles or triples it. The
   Alexandridis kernel with c1 = 0.045, c2 = 0.131 was fitted for shape
   (elongation), not speed, and at these speeds it delivers neither much.
   This is the mechanical reason behind E5, E9b, E10 and E14: no wind
   input, however good, can make this model run 4–7 km in a day.
2. **The cap is real and lower than we said.** The hard limit is one cell
   per tick, but at the E1 recipes the front actually moves 0.47–0.82
   cells/tick, so the real ceiling at 50 ticks/day is 0.7–1.2 km/day, not
   1.5. Observed peak-day advances (Round 2, p95): Bear 1.9 km, Buck 1.8,
   Brattain 4.1, Chimney 5.2. To reproduce them at the E1 p0 the model
   would need 100–210 ticks/day — 2–4× the declared 50. E11 showed that
   raising ticks *with a constant p0* does not help because speed and
   total burn are one knob; the honest reading of both experiments
   together is that **the tick length should vary with the weather**, not
   the probability: on a run day one tick is minutes, on a still night it
   is an hour.
3. **Burn duration 5 is below the fire's own persistence threshold at low
   p0.** At p0 0.08, dur 5 the wind-driven front dies (0.19 → 0 cells/tick
   with wind) while dur 10 survives; the front outruns its own fuel. This
   is E1's "dur 2 kills every fire" seen from the other side and argues
   for dur ≥ 10 ticks whenever p0 < 0.1.
4. **A tick, today:** at 50/day one tick is 28.8 minutes. In model terms
   the front covers 14–25 m per tick at the E1 recipes (calm), i.e.
   0.5–0.9 km/h — a reasonable *daytime* timber/brush spread rate, an
   order of magnitude short of a wind-driven grass run (5–10 km/h).

**What this suggests to build** (not yet run):

- A **rate-of-spread–driven clock**: keep p0 for *whether* a cell burns
  and let the harness (later the model) advance a variable number of ticks
  per hour from a Rothermel-like ROS(fuel, wind, slope, moisture), so that
  ticks per hour = ROS / (cells·per·tick × cell size). That gives wind the
  speed effect the kernel lacks and makes "one tick" a defined quantity.
- Alternatively re-fit c1 (the speed coefficient) on the observed
  front-speed table from Round 2 rather than inheriting Spetses 1990.

**Verdict.** Finding. The declared 50 ticks/day is a convention that is
right within a factor 2–4 on calm days and wrong on run days; it should
become a function of the weather.
