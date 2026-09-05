# Wildfire Validation

This directory holds the tooling for scoring cella's wildfire model against
real, observed fires. The code and docs here are committed; the inputs and
outputs (`data/`, `papers/`, `results/`, `.venv/`) are gitignored — they are
pulled by scripts and can always be recreated.

**New here, or non-technical? Start with [ANALYSIS.md](ANALYSIS.md)** — the
plain-language guide to what the scores mean, what the model's current
shortcomings are, and how to read a results table without fooling yourself.
The formal pre-registered protocol lives in [TEST_PLAN.md](TEST_PLAN.md),
and every improvement experiment (kept and rejected) is recorded in
[experiments/](experiments/README.md).

## The pipeline, start to finish

```
dataset.hdf5  ──convert_pytorchfire.py──▶  scenario.json + config.json + truth.json
   (observed fires)                                    │
                                                       ▼
                                     cargo run --example wildfire_validate
                                                       │
                                                       ▼
                                  per-day IoU / Sørensen table + results JSON
```

### 1. Get the data

The first dataset is the **PyTorchFire six-fire pack**: six real US megafires
(Bear 2020, Chimney 2016, Pier 2017, Brattain 2020, Ferguson 2018, Buck 2017)
as 30 m grids with LANDFIRE fuels/terrain, daily ERA5 wind, and one observed
cumulative burned-area mask per day. License CC-BY 4.0.

```bash
curl -L -o validation/data/dataset.hdf5 \
  https://raw.githubusercontent.com/mzhen77/neural-ca-wildfire/main/data/hdf5/dataset.hdf5
```

Open-access papers behind the dataset and the metrics live in
`validation/papers/` (see `scripts/` history for the download list). The two
paywalled classics (Alexandridis 2008, Filippi 2014) are cited but not stored.

### 2. Convert to cella inputs

```bash
uv venv validation/.venv && VIRTUAL_ENV=$PWD/validation/.venv uv pip install numpy h5py
validation/.venv/bin/python validation/scripts/convert_pytorchfire.py
```

This writes `validation/data/scenarios/<fire>/` in the **canonical v1
scenario format — see [FORMATS.md](FORMATS.md)** for the full spec:

- `scenario.json` — identity, provenance (source, license, converter git
  hash, every simplification made), wind schedule, tick↔hours mapping.
- `config.json` — a normal cella config with the wildfire model attached:
  FBFM40 fuel codes grouped into named fuel classes, elevation layer,
  unburnable cells as Inactive, the `t0` observation as Burning ignition.
- `truth.json` — the observed **arrival time** per cell (hours since `t0`,
  −1 = never burned) plus the times actually observed and the truth's own
  spatial accuracy. Masks, area curves, and arrival metrics all derive
  from this one field.

**Before comparing anything, inspect the weather feed.** A wind given in
the wrong convention, the wrong units, or averaged flat looks *exactly*
like a bad model in the score table — Round 2 spent a day proving our
wind maths right and our ERA5 daily-mean input wrong. Every new source
goes through the checklist in [TEST_PLAN.md §2.1](TEST_PLAN.md) and gets
its answers written into `scenario.json → provenance.weather`
([FORMATS.md](FORMATS.md)). The same applies when quoting another fire
model's score: name the wind convention, units, height, and averaging on
both sides, or the comparison is not one.

### 2b. (Optional) Pull real hourly station weather

```bash
validation/.venv/bin/python validation/scripts/wind_station.py            # all six fires
validation/.venv/bin/python validation/scripts/wind_station.py Bear_2020  # one fire
```

Downloads the nearest NOAA Integrated Surface Database (ISD) station-year
CSV — hourly wind, temperature, dew point, no API key — and writes
`station_hourly.json` beside the scenario: `from_deg` (weather-report
bearing, as ISD already reports it), `speed_ms` (10 m), `temp_c`,
`rh_pct`, one row per hour since `t0`, plus the station's distance,
coverage and a full `provenance.weather` block. The experiment runners
(`scripts/experiments/exp_station.py`) splice it into a scenario copy as
an hourly wind schedule and an hourly fuel-moisture p0 schedule; the
committed scenarios keep ERA5 so reported numbers stay reproducible.
Nearest stations are valley airports 34–72 km from these fires — read the
`caveat` field before trusting a wind result (TEST_PLAN §2.1).

### 3. Run the harness

```bash
cd cella_lib   # its own build root — running from the repo root won't find it
cargo run --release --example wildfire_validate -- \
    ../validation/data/scenarios/Bear_2020 5 ../validation/results/Bear_2020.json
```

Arguments: fire directory, ensemble size (seeds), output path. For each seed
the harness rebuilds the grid, sets that seed and each day's wind on the
model, advances `steps_per_day` ticks per day, and scores the simulated
burned set (Burning + BurnedOut) against the observed mask with the two
field-standard overlap metrics:

- **IoU / Jaccard** = overlap / union — the score the comparison papers
  report (the neural-CA baseline reaches IoU > 0.6 at 72 h on this data).
- **Sørensen** = 2·overlap / (sum of areas).

Day 0 always scores 1.0 by construction (the ignition *is* the first
observed mask) — a built-in sanity check on grid alignment.

An optional 4th argument (a second output path) also dumps the per-cell
arrival grids — the seed-0 simulation and the radial null — which the
figure step below needs.

### 4. Regenerate the figures (optional)

The charts and maps embedded in [ANALYSIS.md](ANALYSIS.md) live in
`figures/` (committed, unlike `results/`). To rebuild them after a model
or converter change, run the harness for all six fires with the fields
dump, then:

```bash
VIRTUAL_ENV=$PWD/validation/.venv uv pip install matplotlib
validation/.venv/bin/python validation/scripts/p0_sweep.py     # knife-edge data (slow)
validation/.venv/bin/python validation/scripts/make_figures.py # writes figures/*.png
```

## Status and first findings (2026-08-14)

The pipeline runs end-to-end on all six fires. With textbook Alexandridis
parameters (p0 = 0.58) the model **over-burns Bear 2020 by ~8×** (447k cells
simulated vs 56k observed, final IoU 0.124) — expected, because those
constants were tuned for a much coarser time step than 50 ticks/day. A quick
probe shows the classic percolation cliff:

| p0 | final sim burned | final IoU | behaviour |
|---|---|---|---|
| 0.10 | 5k | 0.089 | fire dies out |
| 0.20 | 212k | 0.100 | over-burns |
| 0.58 | 447k | 0.124 | burns almost everything reachable |

The observed fire (56k cells) sits inside a narrow band between "dies" and
"explodes" — which is precisely why published CA validations calibrate
per-fire (Alexandridis via black-box optimization, PyTorchFire via
gradients). **Next step: a calibration loop** — grid-search or
coordinate-descent over (p0, burn_duration, per-class veg_factors, spotting)
maximizing mean IoU over the daily series, using the deterministic seed
ensemble. The harness's JSON reports are designed to drive that loop.

Known simplifications to revisit as scores improve: uniform domain-mean wind
(ERA5 gives per-cell u/v we currently average), no fuel moisture / weather
beyond wind, no suppression (late-fire days flatten in reality partly because
of containment — visible in the Bear table where observed growth stalls),
density layer unused (canopy cover is available in the HDF5).

## Status update (2026-09-01): wind audit and the speed cap

Round 2 in [experiments/](experiments/README.md) audited the wind path
after a tester expected weather-report ("from", 0° = north) directions.
That "toward, 0° = +x" angle was our own invention, so the model now
takes the weather-report "from" bearing (`wind_from_deg`, 0° = north,
clockwise) everywhere; scenario format is v2 (`from_deg`). Code, converter, and raster orientation
(checked against the LANDFIRE aspect layer) all agree, so the validation
runs were never mis-winded. What the audit did find: ERA5 daily
domain-mean winds (0.1–3 m/s) leave the wind kernel inert, and on Chimney
2016 they point the wrong way on the run days; and the one-cell-per-tick
front cap (1.5 km/day at 50 ticks/day) is broken by 80–98 % of observed
burned area on the fast fires. Plain-language version in
[ANALYSIS.md §5a](ANALYSIS.md).

## Status update (2026-09-02): observed weather, and a way to stop

Round 3 ([experiments/round-3.md](experiments/round-3.md)): hourly NOAA
ISD station weather can be loaded (`scripts/wind_station.py`) but the
nearest airports are 40–70 km off the fire and score no better than ERA5;
hourly fuel-moisture damping slows the fire without capping it; a monotone
containment decay (p0 × e^(−t/5 d), p0 ×2) is the largest gain in the log
and holds on the Pier holdout (0.32 → 0.46) — a suppression proxy, not
physics, and useless on fires the model under-burns (Ferguson). Engine:
`WildfireModel::set_p0` makes hourly p0 schedules cheap. Later that day:
moisture × decay kept together (E17); the model's own rate of spread
measured — wind moves it 5–10 %, so ticks/day should follow the weather
(E19); a dynamic fire-line agent hook `EXP_LINE_RATE` (E18, needs better
tactics); an evolutionary per-fire search whose median recipe (p0 0.45,
dur 15, τ 3.4 d, wind ×0.29) lifts the Pier holdout to 0.51 (E20).
Follow-ups E21–E23 were negative: ICS-209 percent-contained (now in every
scenario as `containment.json`) rises too slowly to cap the burn, so the
decay is an early-growth decline, not suppression; a wind-driven tick
clock is invisible at daily truth and harmful un-normalised; the improved
line agent still has no middle ground.

## Status update (2026-09-04): ensembles

Round 4 ([experiments/round-4.md](experiments/round-4.md)): the model is
now run as an ensemble. `cella_lib/examples/wildfire_smc` draws 32
members from one untuned prior and outputs a burn-probability map (E24);
in `assim` mode it resamples members on each observed mask, mutates their
parameters and keeps simulating — a particle filter with GA operators
whose scores are one-window-ahead forecasts (E25). Forecast IoU 0.35–0.62
on all six fires, holdout included, within 0.03–0.07 of the Circle and
above it on Chimney; Ferguson 0.13 → 0.34. Recommended: β 10, σ 0.2,
20 % immigrants. Ensembles are a library feature (`"ensemble"` config
block, [docs/ensemble.md](../docs/ensemble.md)). The ad-hoc decay has a
physical replacement: the FSim-style containment-probability operator
(E28) matches or beats it with the decay off. Terrain wind
(`cella_lib::wind_field`, E26) and painted retardant (`set_density`, E27)
are in place but null/negative until the kernel's wind response (E30) and
an agent placement rule exist.

## The challenge ladder (downloaded and waiting)

Deeper research found harder validation targets; the directly fetchable ones
are already in `data/`. In order of increasing difficulty after the six-fire
pack:

1. **Dogrib 2001** (`data/dogrib/instance/`, from the Cell2Fire repo) — the
   Prometheus/Cell2Fire reference case: `.asc` grids (fuel, elevation),
   weather stream, ignition, observed final burn. Essentially our flat-array
   format already. Published bars to beat: Prometheus F1 = 0.74, Cell2Fire
   F1 = 0.83. Exposes lattice/front-shape artifacts under a mid-run wind
   shift; needs a Canadian-FBP → fuel-class mapping.
2. **PT-FireSprd** (`data/pt-firesprd/`, CC-BY,
   [Zenodo](https://doi.org/10.5281/zenodo.7495506)) — 80 Portuguese fires
   2015–2021 with ~3-hourly progression polygons and measured rate-of-spread,
   including Pedrógão Grande 2017 (junction fire, wind reversals, 8.9 km/h
   ROS). Exposes junction-fire acceleration a memoryless CA lacks.
3. **GOFER** (`data/gofer/`, CC-BY,
   [Zenodo](https://zenodo.org/records/10442843)) — hourly GOES-derived
   perimeters/fire lines for 28 large 2019–2021 California fires including
   Creek 2020 (plume-driven) and Dixie 2021 (terrain channeling, 3-month
   soak). Edges are ±1 km, so score arrival-time and growth-rate, not 30 m
   IoU.
4. **Camp Fire 2018** (`data/campfire_nist/`, NIST TN 2135 Appendix F) —
   2,200 timestamped fire-spread observations at sub-hourly resolution.
   The endgame spotting test: ember-driven spread across a canyon and
   through "unburnable" urban fuel; requires interpolating an arrival-time
   surface from points.

Not downloaded (request-gated or digitize-from-paper): RxCADRE instrumented
burns (USFS archive, drop-in ASCII — fetch when we get there), NIROPS/FIRIS
IR archives, Kilmore East 2009 (paper in `papers/`, isochrones would need
digitizing), Marshall Fire (awaiting NIST release), CFSDS Canada
(OSF, distributional validation).

Shared prerequisite for rungs 2–4: one `isochrones → arrival-time flat
array` rasterizer (GDAL) — build once, unlocks PT-FireSprd, GOFER, NIROPS,
and FIRIS alike. Metric to add alongside it: arrival-time error (the
sub-daily sources make plain IoU under-informative).
