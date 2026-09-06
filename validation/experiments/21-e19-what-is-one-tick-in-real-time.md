# E19 — what is one tick in real time? The model's own rate of spread · finding (data)

_Round 3 (2026-09-02) · 3 seeds · synthetic grid, no fire · example `cella_lib/examples/wildfire_ros.rs` · results `exp19_ros.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** The scenarios say 50 ticks per day, copied from the papers,
but nothing ties a tick to a length of time. Freire & DaCamara (2019)
fixed theirs by simulating until the observed area burned and dividing
the observed time by the mean tick count (about 20 min). We measured the speed the
model's front actually produces, in cells per tick, on a flat uniform
grid, as a function of p0, wind and burn duration. Speed is set by p0;
wind changes it by only 5–10 % from calm to 8 m/s, where a real fire
speeds up two to three times. At the E1 recipe the front covers 0.5–0.8
cells per tick, so the real ceiling is 0.7–1.2 km/day, and matching the
observed run days would need 2–4× the declared ticks. The tick length
should follow the weather, not be a constant.

**Question.** How fast does the model's front move, and what does that
make one tick in hours?

**What we measured.** 240 × 120 homogeneous fuel (veg 1, density 1,
flat), a burning column at x = 0..2 so the front is a straight line,
wind from 270° (blowing +x) or calm. Front position = mean over rows of
the rightmost burning or burned cell; speed = slope of a linear fit
after a 20-tick warm-up; 3 seeds.

**Result.** Front speed, cells per tick (left block duration 5, right
block duration 10; columns wind 0 / 2 / 5 / 8 m/s):

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

How to read it: 1.00 is the hard cap (one cell per tick). Read a row left
to right to see the wind effect, a column top to bottom to see the p0
effect. "same" means the duration-10 block matches duration 5 at that p0.

1. **Rate is set by p0, not by wind.** From calm to 8 m/s the front
   speeds up by 5–10 % at any p0. In Rothermel-type models rate of spread
   grows roughly as wind^1.5–2; a 5 m/s wind doubles or triples it. The
   Alexandridis kernel (c1 = 0.045, c2 = 0.131) was fitted for shape, not
   speed, and delivers little of either at these speeds. This is the
   mechanical reason behind E5, E9b, E10 and E14: no wind input, however
   good, can make this model run 4–7 km in a day.
2. **The cap is real and lower than we said.** At the E1 recipes the
   front moves 0.47–0.82 cells/tick, so the real ceiling at 50 ticks/day
   is 0.7–1.2 km/day, not 1.5. Observed peak-day advances (Round 2, p95):
   Bear 1.9 km, Buck 1.8, Brattain 4.1, Chimney 5.2. Reproducing them at
   the E1 p0 would need 100–210 ticks/day, 2–4× the declared 50. E11
   showed that raising ticks *with a constant p0* does not help because
   speed and total burn are one knob. Read together: **the tick length
   should vary with the weather**, not the probability.
3. **Burn duration 5 is below the fire's own persistence threshold at
   low p0.** At p0 0.08, dur 5 the wind-driven front dies (0.19 → 0 with
   wind) while dur 10 survives. This is E1's "dur 2 kills every fire"
   seen from the other side and argues for dur ≥ 10 whenever p0 < 0.1.
4. **A tick, today:** at 50/day one tick is 28.8 minutes. The front
   covers 14–25 m per tick at the E1 recipes (calm), i.e. 0.5–0.9 km/h, a
   reasonable *daytime* timber or brush spread rate, an order of
   magnitude short of a wind-driven grass run (5–10 km/h).

**What it means.** The declared 50 ticks/day is a convention that is
right within a factor 2–4 on calm days and wrong on run days. Two ways to
fix it: a rate-of-spread–driven clock (ticks per hour from a Rothermel-
like ROS of fuel, wind, slope and moisture), or a re-fit of c1 on the
observed front-speed table from Round 2 rather than inheriting Spetses
1990.

**Questions this raises.**

- Does a wind-driven clock help? → E22: invisible at daily truth when
  total ticks are held; harmful when ticks are added, because the kernel
  widens rather than stretches.
- Does terrain wind help before the kernel is refit? → E26: no.
- Does the wind rule change shape anywhere in knob space? → E37: no; it
  is a speed knob. E30 (refit c1 or an elliptical rule) is the fix, not
  yet run.

**Verdict.** Finding.

**Later.** E22, E26, E37, E30.
