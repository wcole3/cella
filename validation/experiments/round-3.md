# Round 3 — 2026-09-02: real weather in, explosion out

_Score family: single-run mean IoU · 3 seeds · calibration fires; E16c and E20 report all six · terms: [GLOSSARY.md](GLOSSARY.md)_

## What we knew before

Wind maths is right and the wind input is weak (Round 2). Resolution is
not the limit. The model has one speed and cannot stop: the burned area
climbs until it runs out of land. Two questions set the day: can we load
*observed* weather instead of ERA5 daily means, and what practices make
a CA fire stop at the right size?

What the literature says about stopping (sources in each entry):

- PROPAGATOR (Trucchia et al. 2020, operational in Italy): spread
  probability × a fuel-moisture damping (Burgan & Rothermel 1984,
  extinction moisture 0.3) from hourly RH/T, *plus* suppression patterns
  when records exist; without them it over-predicts (Sahila et al. 2025).
- Freire & DaCamara 2019 (Alexandridis-type CA on Portuguese fires):
  avoid over-burning by *constraining runs to the observed perimeter* and
  report that the model runs early; they plan FFMC/DC factors.
- Alexandridis 2011: probability factors tied to the Canadian FFMC and
  Drought Code.
- Nobody in this family reports a CA that stops on its own at the right
  size without moisture + suppression data or a perimeter fence.

## What we ran

| # | Question | Answer |
|---|---|---|
| [E14](17-e14-hourly-station-wind.md) | Does hourly station wind fix the weak ERA5 wind? | No as a drop-in (stations 40–70 km away); the loader is kept. REJECTED / KEPT |
| [E15](18-e15-hourly-fuel-moisture-damping.md) | Does hourly moisture damping cap the burn? | No; it slows, then re-tuned it explodes again. REJECTED |
| [E16](19-e16-heterogeneity-and-containment-decay.md) | Does patchy fuel (a) or a steady decay (b) stop it? | (a) nothing; (b) +0.07 to +0.14, area right for the first time. (b) KEPT |
| [E16c](20-e16c-containment-decay-global-recipe-holdout.md) | Does one global decay hold on the holdout? | Pier +0.14 yes; Ferguson no (under-burns). KEPT with caveat |
| [E19](21-e19-what-is-one-tick-in-real-time.md) | How fast does the model move per tick, and does wind change it? | 0.5–0.8 cells/tick at E1; wind changes it 5–10 %. FINDING |
| [E17](22-e17-moisture-times-decay.md) | Moisture × decay together? | Equals decay alone on mean IoU, better final map. KEPT |
| [E18](23-e18-dynamic-fire-line-agent.md) | A crew that paints the fire's own edge? | Strangles or is ignored; no middle. REJECTED, direction kept |
| [E20](24-e20-evolutionary-per-fire-calibration-and-transfer.md) | What do per-fire optimiser runs agree on? | τ ≈ 3 d, dur 15, station wind down; median lifts Pier to 0.51. FINDING |
| [E21](25-e21-observed-containment-replaces-decay.md) | Real percent-contained instead of the decay? | Far worse; the decay is not suppression. REJECTED, relabelled |
| [E22](27-e22-rate-of-spread-driven-clock.md) | A clock that follows hourly wind? | Invisible at daily truth; harmful when it adds ticks. REJECTED |
| [E23](26-e23-ramped-breachable-line-agent.md) | Ramping, breachable, anchor-and-flank crew? | Still strangles or is ignored. REJECTED, parked |

## What we know now

Engine finding: hourly p0 schedules were 60× too slow, fixed with
`set_p0`. An hourly schedule changes p0 553 times per run. The harness
used to clone the model and re-attach (the Round 1 workaround for the
"p0 is baked at attach" footgun), which rebuilds the 8 × cells slope
table every hour: 5 minutes per Bear run instead of 5 seconds.
`WildfireModel::set_p0(p0)` rebuilds only the per-fuel bases and
`p_base`, using attach's exact arithmetic, so the result is bit-identical
to a fresh attach (pinned by a test) in O(cells). The FNV snapshot suite
is unchanged.

Verified results (all 3 seeds):

| Fire | Round-2 best (per-fire, ERA5) | Round-3 best | recipe | Circle |
|---|---|---|---|---|
| Bear | 0.317 | **0.427** | E1 p0 ×2, dur 10, decay τ5 | 0.541 |
| Brattain | 0.337 | **0.407** | E1 p0 ×2, dur 10, decay τ5 | 0.450 |
| Buck | 0.485 (E13 temp) | **0.541** | E1 p0 ×2, dur 5, decay τ5 | 0.670 |
| Chimney | 0.441 | 0.446 (E16a σ0.6, noise) | — | 0.372 |
| Global, all six | 0.22 / 0.31 / 0.37 / 0.42 / 0.15 / 0.32 | 0.45 / 0.37 / 0.47 / 0.39 / **0.16** / **0.51** | E20 transfer: p0 0.45, dur 15, τ 3.4 d, wind ×0.29 | |
| Per-fire optimum (E20, 3 seeds) | | 0.480 / 0.408 / 0.581 / 0.474 | per-fire, see E20 | |

How to read it: mean IoU; the global row lists Bear / Brattain / Buck /
Chimney / Ferguson / Pier, the last two holdout; bold is the improvement
of the round. The per-fire optimum row peeked at the whole series and is
an upper bound.

1. **Observed weather is in the pipeline** (E14), but a valley airport
   40–70 km away is not the wind on the fire. Next input to try: RAWS
   fire-weather stations or a downscaled field.
2. **Periodic damping cannot cap the burn** (E13 temperature, E15
   moisture and night). It changes *when* cells burn, not *whether*.
3. **A monotone decay can** (E16b/c): the largest gain in the log,
   confirmed on the Pier holdout, area finally the right size. It is a
   suppression proxy with a fitted τ, so labelled as such.
4. **Two failure directions, two mechanisms.** The decay fixes over-burn
   and hurts under-burn (Ferguson, Chimney). Fast fires need the *rate*
   side before any stopping rule can help them.
5. **Heterogeneity does nothing** at this grid size (E16a). Dropped.
6. **Both effects, kept** (E17): moisture × decay equals decay alone on
   mean IoU and improves final-day IoU. Working recipe: p0 ×4 E1, hourly
   station wind, η at M_x 35 %, τ 5 d.
7. **One tick is not yet a defined time** (E19): wind changes the model's
   front speed by 5–10 % where reality changes it 2–3×; the fast fires
   need 2–4× the declared ticks/day.
8. **The fire-line agent is the right mechanism, wrong tactics** (E18,
   E23): a perfect line is all-or-nothing, a breachable line is ignored,
   the one right-sized case has the wrong shape. Parked; needs an
   intensity model or observed lines.
9. **The optimiser agrees with the experiments** (E20): τ 2.5–3.7 d, dur
   15, station wind turned down transfer across fires and lift the Pier
   holdout to 0.51; fuel ratios and the fast-fire regime do not. A
   diagnostic, never the headline.
10. **The decay is not suppression** (E21): real ICS-209 percent-
    contained (all six fires now carry `containment.json`) rises far too
    slowly (0–13 % by day 4) to cap anything. τ 5 d is an *early-days
    growth-rate decline* whose mechanism is still to be isolated.
11. **A wind clock needs a kernel that stretches** (E22): moving ticks to
    windy hours is invisible at daily truth; adding them is E11 again (area ×1.5–3.6).

Tooling added: `scripts/wind_station.py`; `exp_station.py`,
`exp_stopping.py`, `exp_contain_holdout.py`, `exp_combined.py`,
`exp_fireline.py` (+ `EXP_LINE_RATE`), `exp_evolve.py` (scipy differential
evolution, 8 workers), `summarize.py`; `cella_lib/examples/wildfire_ros.rs`
(E19); hooks `EXP_TICK_SCALE` (E22), `EXP_LINE_RAMP_DAYS` / `EXP_LINE_TYPE`
/ `EXP_LINE_TACTIC` (E23); runners `exp_containment.py` (E21),
`exp_clock.py` + `exp_clock_b.py` (E22), `exp_fireline2.py` (E23);
ICS-209-PLUS situation reports → `containment.json` per scenario (E21).
The log was reorganised into this folder, one file per experiment.

## Still open after this round

- What does τ stand in for? Fit τ per fire and regress it on first-week
  weather, fuel and ignition-mask age. Still open.
- A published stopping mechanism instead of a fitted clock. → E28.
- The rate side for fast fires: refit c1 or an elliptical rule (E30), or
  a ROS-driven clock once the kernel stretches. → E30 not yet run.
- Observed containment lines and drop locations for the repaint
  mechanism. Data not yet sourced.
- Engine: make the panel's p0 edit use `set_p0`; expose a p0-schedule
  input on the model so drivers stop being a harness hack (roadmap §10.4).

## Configuration after this round

- **Per fire (E17 recipe):** p0 = 4 × E1, dur as E1, hourly station wind
  ×1, moisture η at M_x 35 %, decay τ 5 d (τ 10 for fast fires).
- **Global (E20 transfer):** p0 0.45, dur 15, τ 3.4 d, station wind
  ×0.29, moisture off → 0.45 / 0.37 / 0.47 / 0.39 / 0.16 / 0.51.
- The decay is labelled an empirical early-days growth-rate decline, not
  suppression.
