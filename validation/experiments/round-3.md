# Round 3 — 2026-09-02: real weather in, explosion out

Two questions set the day. Can we load *observed* weather instead of ERA5
daily means, and what practices make a CA fire stop instead of burning
everything reachable? Research first (firecrawl, context7), then E14–E16c.

**What the literature says about stopping** (sources in each entry):

- PROPAGATOR (Trucchia et al. 2020, operational in Italy): spread
  probability × a fuel-moisture damping (Burgan & Rothermel 1984,
  extinction moisture 0.3) from hourly RH/T, *plus* suppression patterns
  when records exist; without them it over-predicts (Sahila et al. 2025).
- Freire & DaCamara 2019 (Alexandridis-type CA on Portuguese fires):
  avoid over-burning by *constraining runs to the observed perimeter* and
  report the model runs early; they plan FFMC/DC factors.
- Alexandridis 2011: probability factors tied to the Canadian FFMC and
  Drought Code.
- Nobody in this family reports a CA that stops on its own at the right
  size without either moisture + suppression data or a perimeter fence.

**Tooling added**

- `scripts/wind_station.py` — NOAA ISD hourly station loader (wind, T,
  dew point → RH), key-free, writes `station_hourly.json` with a full
  `provenance.weather` block (E14).
- `exp_station.py` (E14 wind, E15 moisture), `exp_stopping.py` (E16a/b),
  `exp_contain_holdout.py` (E16c), `exp_combined.py` (E17),
  `exp_fireline.py` (E18, with the new `EXP_LINE_RATE` harness hook),
  `exp_evolve.py` (E20, scipy differential evolution, 8 workers),
  `summarize.py` (table from any experiment JSON).
- `cella_lib/examples/wildfire_ros.rs` — measures the model's own front
  speed vs p0 / wind / burn duration (E19).
- Harness hooks `EXP_TICK_SCALE` (E22), `EXP_LINE_RAMP_DAYS` /
  `EXP_LINE_TYPE` / `EXP_LINE_TACTIC` (E23); runners `exp_containment.py`
  (E21), `exp_clock.py` + `exp_clock_b.py` (E22), `exp_fireline2.py` (E23).
- ICS-209-PLUS situation reports (Figshare, CC BY) → `containment.json`
  per scenario: daily percent-contained, acres, personnel (E21).
- Log reorganised into this folder, one file per experiment.

## Engine finding: hourly p0 schedules were 60× too slow — fixed with `set_p0`

An hourly schedule changes p0 553 times per run. The harness used to do
that by cloning the model and re-attaching (the Round-1 workaround for the
"p0 is baked at attach" footgun), which rebuilds the 8 × cells slope table
every hour: 5 minutes per Bear run instead of 5 seconds. Fix in
`cella_lib/src/wildfire.rs`: `WildfireModel::set_p0(p0)` rebuilds only the
per-fuel bases and `p_base`, using attach's exact arithmetic (the derived
state now keeps each cell's fuel slot and each fuel's `veg_factor`), so the
result is bit-identical to a fresh attach — pinned by a test — in O(cells).
The FNV snapshot suite is unchanged. The panel path still re-attaches
(fine for a slider); the harness uses `set_p0`.

## Verified results (all 3 seeds)

| Fire | Round-2 best (per-fire, ERA5) | Round-3 best | recipe | Circle |
|---|---|---|---|---|
| Bear | 0.317 | **0.427** | E1 p0 ×2, dur 10, decay τ5 | 0.541 |
| Brattain | 0.337 | **0.407** | E1 p0 ×2, dur 10, decay τ5 | 0.450 |
| Buck | 0.485 (E13 temp) | **0.541** | E1 p0 ×2, dur 5, decay τ5 | 0.670 |
| Chimney | 0.441 | 0.446 (E16a σ0.6, noise) | — | 0.372 |
| Global, all six | 0.22 / 0.31 / 0.37 / 0.42 / 0.15 / 0.32 | 0.45 / 0.37 / 0.47 / 0.39 / **0.16** / **0.51** | E20 transfer: p0 0.45, dur 15, τ 3.4 d, wind ×0.29 | |
| Per-fire optimum (E20, 3 seeds) | | 0.480 / 0.408 / 0.581 / 0.474 | per-fire, see E20 | |

(Global row: Bear / Brattain / Buck / Chimney / Ferguson / Pier; the last
two are holdout.)

## Round 3 conclusions → what to do next

1. **Observed weather is in the pipeline** (E14), but a valley airport
   40–70 km away is not the wind on the fire; as a drop-in it scores the
   same or slightly worse than ERA5. Next input to try: RAWS fire-weather
   stations (MesoWest/Synoptic token) or a WindNinja-downscaled field.
2. **Periodic damping cannot cap the burn** (E13 temperature, E15
   moisture/night). It changes *when* cells burn, not *whether*. Stop
   spending runs on it until sub-daily truth (GOFER, PT-FireSprd) makes
   timing the metric.
3. **A monotone decay can** (E16b/c): the largest gain in the log,
   confirmed on the Pier holdout, area finally the right size. It is a
   suppression proxy with a fitted τ, so label it as such. Replace it with
   real data next: daily percent-contained from InciWeb/NIFC for the six
   fires (a time series we can fetch), then containment lines as
   unburnable cells (spatial). Both are "load observations", same as
   today's wind work.
4. **Two failure directions, two mechanisms.** The decay fixes over-burn
   and hurts under-burn (Ferguson, Chimney). Fast fires need the *rate*
   side — stronger, correctly pointed wind and a real ROS — before any
   stopping rule can help them.
5. **Heterogeneity does nothing** at this grid size (E16a). Dropped.
6. **Both effects, kept** (E17): moisture × decay equals decay-alone on
   mean IoU and improves final-day IoU; the physics stays in, the proxy
   does the capping. Working recipe: p0 ×4 E1, hourly station wind, η at
   M_x 35 %, τ 5 d.
7. **One tick is not yet a defined time** (E19): wind changes the model's
   front speed by 5–10 % where reality changes it 2–3×; the fast fires
   need 2–4× the declared ticks/day. Build a rate-of-spread–driven clock
   (ticks per hour from ROS(fuel, wind, slope, moisture)) or refit c1 on
   the observed front-speed table.
8. **The fire-line agent is the right mechanism, wrong tactics** (E18):
   painting the model's own edge between steps works operationally, but a
   perfect heel-first line is all-or-nothing. Needs ramping resources,
   breachable line, anchor-and-flank tactics, and percent-contained as
   its target.
9. **The optimiser agrees with the experiments** (E20): τ 2.5–3.7 d,
   dur 15, station wind turned down transfer across fires and lift the
   Pier holdout to 0.51; fuel ratios and the fast-fire regime do not
   transfer. Use it as a diagnostic, never as the headline.
10. **The decay is not suppression** (E21): real ICS-209 percent-contained
    (all six fires now carry `containment.json`) rises far too slowly
    (0–13 % by day 4) to cap anything; the fitted τ 5 d is an *early-days
    growth-rate decline* whose mechanism is still to be isolated — fit τ
    per fire, regress on first-week weather/fuel/ignition-mask age.
11. **A wind clock needs a kernel that stretches** (E22/E22b): moving
    ticks to windy hours is invisible at daily truth; adding them is E11
    again (area ×1.5–3.6). Re-fit c1/c2 for elongation (or an elliptical
    rule) before revisiting, and revisit on hourly truth.
12. **Rule-based line agents are parked** (E18, E23): perfect line
    strangles, breachable line is ignored, the one right-sized case has
    the wrong shape. The repaint mechanism stays for *observed* daily
    perimeters/lines; a rule agent needs an intensity model.
13. Engine: `set_p0` closes the Round-1 footgun for the harness; consider
   making the panel's p0 edit use it too, and expose a `p0 schedule`
   input on the model so weather/suppression drivers stop being a harness
   hack (roadmap §10.4).
