# Wildfire Validation

> **Not for real fires.** This is mostly personal interest. No one should
> currently be using this to model real fires. Nothing here has been
> checked for operational use, and the results below show the model
> still loses to very simple guesses on several fires.

## What this directory is

It scores cella's wildfire model against six real, observed US fires and
keeps a written record of every attempt to improve the score, including
the ones that failed. The code and docs here are committed. The inputs
and outputs (`data/`, `papers/`, `results/`, `.venv/`) are gitignored;
the steps below recreate them.

The rule behind everything here: the goal is the best answer, not a
flattering score. Every score is printed next to two deliberately dumb
forecasters (the "Circle" and "persistence"), because a model is only
interesting where it beats them.

## Where to start reading

| If you are... | Read |
|---|---|
| new, or non-technical | [ANALYSIS.md](ANALYSIS.md): what the scores mean, what the model gets wrong, how to read a results table without fooling yourself |
| looking for what was tried | [experiments/README.md](experiments/README.md): the six fires, one table of every experiment (kept and rejected), one file per experiment |
| meeting an unfamiliar term | [experiments/GLOSSARY.md](experiments/GLOSSARY.md): every term used in the log |
| checking the rules | [TEST_PLAN.md](TEST_PLAN.md): the pre-registered protocol (metrics, baselines, tuning/holdout split) |
| writing a converter or reading the files | [FORMATS.md](FORMATS.md): the scenario file layout |

## Current status

As of the latest round (Round 7, 2026-09-23/25). Details and caveats are
in [ANALYSIS.md](ANALYSIS.md) (section 5f) and the
[experiment table](experiments/README.md).

- The pipeline below runs end to end on all six fires (Bear 2020,
  Brattain 2020, Buck 2017, Chimney 2016, Ferguson 2018, Pier 2017).
- With textbook parameters the model over-burns badly and loses to the
  Circle on 5 of 6 fires (Bear 2020 is over-burned by about 8x).
- The recommended configuration is a 32-member ensemble that learns from
  each observed perimeter as the fire burns (beta 10, sigma 0.2, 20 %
  immigrants, containment operator on). Its forecast IoU is 0.35-0.60
  across the six fires (E31 replication of the Round 4 setup), within
  0.03-0.07 of the Circle on five fires; it beats the Circle outright on
  only one of the six (ANALYSIS section 5f). This configuration has not
  changed since Round 5.
- Round 6 and 7 tested an arrival-time spread kernel with a learned
  wind-direction gene ("Arm B"). It was **rejected as tested**: it loses
  to the recommended configuration on Pier by 4.60 sd against a
  pre-registered limit of 1 sd. Every option from those rounds is
  off by default.
- Correcting the ensemble from the observed perimeter helps, but the
  trivial guess "yesterday's map, unchanged" still beats it on every
  fire, so it is not a forecasting product.
- Known limits: the daily ERA5 wind input is weak and sometimes points
  the wrong way; the model cannot draw the long, thin shapes of Brattain,
  Ferguson and Pier at their size (spotting lifts this for Ferguson, and
  maybe Pier, not Brattain).

## How to reproduce

### Prerequisites

- A Rust toolchain (`cargo`). The wildfire examples live in `cella_lib/`,
  which is its **own build root**: run `cargo` commands from `cella_lib/`,
  not from the repo root.
- Python 3 with a virtual environment at `validation/.venv` (the
  commands below use [uv](https://docs.astral.sh/uv/); plain `python -m
  venv` works too), and `curl`.

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

The first dataset is the **PyTorchFire six-fire pack**: six real US
megafires as 30 m grids with LANDFIRE fuels/terrain, daily ERA5 wind, and
one observed cumulative burned-area mask per day. License CC-BY 4.0.

```bash
curl -L -o validation/data/dataset.hdf5 \
  https://raw.githubusercontent.com/mzhen77/neural-ca-wildfire/main/data/hdf5/dataset.hdf5
```

(Create `validation/data/` first if it does not exist.) Open-access
papers behind the dataset and the metrics were saved by hand into
`validation/papers/`; nothing needs them to run. The two paywalled
classics (Alexandridis 2008, Filippi 2014) are cited but not stored.

### 2. Convert to cella inputs

```bash
uv venv validation/.venv
VIRTUAL_ENV=$PWD/validation/.venv uv pip install numpy h5py
validation/.venv/bin/python validation/scripts/convert_pytorchfire.py
```

This writes `validation/data/scenarios/<fire>/` in the **canonical
scenario format (currently v2), specified in [FORMATS.md](FORMATS.md)**:

- `scenario.json`: identity, provenance (source, license, converter git
  hash, every simplification made), wind schedule, tick-to-hours mapping.
- `config.json`: a normal cella config with the wildfire model attached:
  FBFM40 fuel codes grouped into named fuel classes, elevation layer,
  unburnable cells as Inactive, the `t0` observation as the Burning
  ignition.
- `truth.json`: the observed **arrival time** per cell (hours since `t0`,
  -1 = never burned), the times actually observed, and the truth's own
  spatial accuracy. Masks, area curves, and arrival metrics all derive
  from this one field.

**Before comparing anything, inspect the weather feed.** A wind given in
the wrong convention, the wrong units, or averaged flat looks *exactly*
like a bad model in the score table. Round 2 spent a day proving our
wind math right and our ERA5 daily-mean input wrong. Every new source
goes through the checklist in [TEST_PLAN.md section 2.1](TEST_PLAN.md)
and gets its answers written into `scenario.json` under
`provenance.weather` ([FORMATS.md](FORMATS.md)). The same applies when
quoting another fire model's score: name the wind convention, units,
height, and averaging on both sides, or the comparison is not one.

### 2b. (Optional) Pull real hourly station weather

```bash
VIRTUAL_ENV=$PWD/validation/.venv uv pip install pandas
validation/.venv/bin/python validation/scripts/wind_station.py            # all six fires
validation/.venv/bin/python validation/scripts/wind_station.py Bear_2020  # one fire
```

Downloads the nearest NOAA Integrated Surface Database (ISD) station-year
CSV (hourly wind, temperature, dew point; no API key) and writes
`station_hourly.json` beside the scenario: `from_deg` (weather-report
bearing, as ISD already reports it), `speed_ms` (10 m), `temp_c`,
`rh_pct`, one row per hour since `t0`, plus the station's distance,
coverage and a full `provenance.weather` block. The experiment runners
(`scripts/experiments/exp_station.py`) splice it into a scenario copy as
an hourly wind schedule and an hourly fuel-moisture p0 schedule; the
committed scenarios keep ERA5 so reported numbers stay reproducible.
The nearest stations are valley airports 34-72 km from these fires, so
read the `caveat` field before trusting a wind result (TEST_PLAN
section 2.1).

### 2c. (Optional) Percent-contained records

Some experiments (E21, E42, E49) read a `containment.json` per scenario:
the daily "percent contained" from the ICS-209-PLUS incident reports
(data in `validation/data/ics209/`). No committed script builds this
file, so those experiments cannot be rerun from this repo alone. The
harness and the ensemble runs below do not need it.

### 3. Run the harness

```bash
cd cella_lib   # its own build root; running from the repo root won't find it
cargo run --release --example wildfire_validate -- \
    ../validation/data/scenarios/Bear_2020 5 ../validation/results/Bear_2020.json
```

Arguments: fire directory, ensemble size (seeds), output path. For each
seed the harness rebuilds the grid, sets that seed and each day's wind on
the model, advances `steps_per_day` ticks per day, and scores the
simulated burned set (Burning + BurnedOut) against the observed mask with
the two field-standard overlap metrics:

- **IoU / Jaccard** = overlap / union. This is the score the comparison
  papers report (the neural-CA baseline reaches IoU > 0.6 at 72 h on this
  data).
- **Sørensen** = 2 x overlap / (sum of areas).

Day 0 always scores 1.0 by construction (the ignition *is* the first
observed mask), which doubles as a sanity check on grid alignment.

An optional 4th argument (a second output path) also dumps the per-cell
arrival grids (the seed-0 simulation and the radial null), which the
figure step below needs.

### 4. Run the ensemble forecast (the recommended configuration)

```bash
cd cella_lib
SMC_BETA=10 SMC_SIGMA=0.2 SMC_IMMIGRANTS=0.2 SMC_CONTAIN=1 SMC_TAU_OFF=1 \
cargo run --release --example wildfire_smc -- \
    ../validation/data/scenarios/Bear_2020 32 assim ../validation/results/Bear_smc.json
```

Arguments: scenario directory, number of members, mode
(`open`, `assim`, `evolve`, `map`, `replay`), output path. `assim` is the
learning-as-it-burns mode. The `SMC_*` environment variables are
documented at the top of `cella_lib/examples/wildfire_smc/main.rs`. The
scripts in `validation/scripts/experiments/` drive these runs for each
experiment; every experiment file names its runner and output. They call
the **release binary** at `cella_lib/target/release/examples/`, so run
`cargo build --release --examples` in `cella_lib/` first. A five-seed,
six-fire batch takes hours. Results land in `validation/results/experiments/`.

### 5. Regenerate the figures (optional)

The charts and maps embedded in [ANALYSIS.md](ANALYSIS.md) live in
`figures/` (committed, unlike `results/`). To rebuild them after a model
or converter change, run the harness for all six fires with the fields
dump (step 3, 4th argument, output to `validation/results/fields/`), then:

```bash
VIRTUAL_ENV=$PWD/validation/.venv uv pip install matplotlib
validation/.venv/bin/python validation/scripts/p0_sweep.py     # knife-edge data (slow)
validation/.venv/bin/python validation/scripts/make_figures.py # writes figures/*.png
```

## Harder datasets (downloaded, not yet used)

Further research found harder validation targets. The directly fetchable
ones were saved by hand into `data/`; none has a converter yet (see the
planned-converter table in [FORMATS.md](FORMATS.md)). In order of
increasing difficulty after the six-fire pack:

1. **Dogrib 2001** (`data/dogrib/instance/`, from the Cell2Fire repo):
   the Prometheus/Cell2Fire reference case with `.asc` grids (fuel,
   elevation), weather stream, ignition, observed final burn. Essentially
   our flat-array format already. Published bars to beat: Prometheus
   F1 = 0.74, Cell2Fire F1 = 0.83. Exposes lattice/front-shape artifacts
   under a mid-run wind shift; needs a Canadian-FBP to fuel-class
   mapping.
2. **PT-FireSprd** (`data/pt-firesprd/`, CC-BY,
   [Zenodo](https://doi.org/10.5281/zenodo.7495506)): 80 Portuguese fires
   2015-2021 with ~3-hourly progression polygons and measured
   rate-of-spread, including Pedrógão Grande 2017 (junction fire, wind
   reversals, 8.9 km/h ROS). Exposes junction-fire acceleration a
   memoryless CA lacks.
3. **GOFER** (`data/gofer/`, CC-BY,
   [Zenodo](https://zenodo.org/records/10442843)): hourly GOES-derived
   perimeters/fire lines for 28 large 2019-2021 California fires
   including Creek 2020 (plume-driven) and Dixie 2021 (terrain
   channeling, 3-month soak). Edges are +-1 km, so score arrival time and
   growth rate, not 30 m IoU.
4. **Camp Fire 2018** (`data/campfire_nist/`, NIST TN 2135 Appendix F):
   2,200 timestamped fire-spread observations at sub-hourly resolution.
   The endgame spotting test: ember-driven spread across a canyon and
   through "unburnable" urban fuel; requires interpolating an
   arrival-time surface from points.

Not downloaded (request-gated or digitize-from-paper): RxCADRE
instrumented burns (USFS archive, drop-in ASCII), NIROPS/FIRIS IR
archives, Kilmore East 2009 (paper in `papers/`, isochrones would need
digitizing), Marshall Fire (awaiting NIST release), CFSDS Canada (OSF,
distributional validation).

Shared prerequisite for rungs 2-4: one `isochrones -> arrival-time flat
array` rasterizer (GDAL), to be built once; it unlocks PT-FireSprd,
GOFER, NIROPS, and FIRIS alike. Metric to add alongside it: arrival-time
error (the sub-daily sources make plain IoU under-informative).

## History

Dated status updates, oldest first, condensed. Numbers and dates are as
recorded at the time; later rounds sometimes changed how earlier results
should be read (each experiment file has a "Later" section).

### 2026-08-14: first findings

The pipeline ran end to end on all six fires. With textbook Alexandridis
parameters (p0 = 0.58) the model **over-burns Bear 2020 by ~8x** (447k
cells simulated vs 56k observed, final IoU 0.124). This was expected:
those constants were tuned for a much coarser time step than 50
ticks/day. A quick probe showed the classic percolation cliff:

| p0 | final sim burned | final IoU | behavior |
|---|---|---|---|
| 0.10 | 5k | 0.089 | fire dies out |
| 0.20 | 212k | 0.100 | over-burns |
| 0.58 | 447k | 0.124 | burns almost everything reachable |

The observed fire (56k cells) sits inside a narrow band between "dies"
and "explodes", which is why published CA validations calibrate per fire
(Alexandridis via black-box optimization, PyTorchFire via gradients). The
planned next step was a calibration loop over (p0, burn_duration,
per-class veg_factors, spotting) maximizing mean IoU over the daily
series. Known simplifications at that point: uniform domain-mean wind
(ERA5 gives per-cell u/v; we averaged it), no fuel moisture or weather
beyond wind, no suppression (late-fire days flatten in reality partly
because of containment), density layer unused (canopy cover is available
in the HDF5).

### 2026-09-01: wind audit and the speed cap (Round 2)

[experiments/round-2.md](experiments/round-2.md). A tester expected
weather-report directions ("from", 0 = north). The old "toward, 0 = +x"
angle was our own invention, so the model now takes the weather-report
"from" bearing (`wind_from_deg`, 0 = north, clockwise) everywhere, and
the scenario format is v2 (`from_deg`). Code, converter, and raster
orientation (checked against the LANDFIRE aspect layer) all agree, so the
validation runs were never mis-winded. What the audit did find: ERA5
daily domain-mean winds (0.1-3 m/s) leave the wind kernel inert, and on
Chimney 2016 they point the wrong way on the run days; and the
one-cell-per-tick front cap (1.5 km/day at 50 ticks/day) is broken by
80-98 % of observed burned area on the fast fires. Plain-language version
in [ANALYSIS.md section 5a](ANALYSIS.md).

### 2026-09-02: observed weather, and a way to stop (Round 3)

[experiments/round-3.md](experiments/round-3.md). Hourly NOAA ISD station
weather can be loaded (`scripts/wind_station.py`), but the nearest
airports are 40-70 km off the fire and score no better than ERA5. Hourly
fuel-moisture damping slows the fire without capping it. A monotone
containment decay (p0 x e^(-t/5 d), p0 x2) is the largest gain in the log
and holds on the Pier holdout (0.32 to 0.46); it is a suppression proxy,
not physics, and useless on fires the model under-burns (Ferguson).
Engine: `WildfireModel::set_p0` makes hourly p0 schedules cheap. Later
that day: moisture x decay kept together (E17); the model's own rate of
spread measured, where wind moves it 5-10 %, so ticks/day should follow
the weather (E19); a dynamic fire-line agent hook `EXP_LINE_RATE` (E18,
needs better tactics); and an evolutionary per-fire search whose median
recipe (p0 0.45, dur 15, tau 3.4 d, wind x0.29) lifts the Pier holdout
to 0.51 (E20). Follow-ups E21-E23 were negative: ICS-209 percent-contained
(now in every scenario as `containment.json`) rises too slowly to cap the
burn, so the decay is an early-growth decline, not suppression; a
wind-driven tick clock is invisible at daily truth and harmful
un-normalized; the improved line agent still has no middle ground.

### 2026-09-04: ensembles (Round 4)

[experiments/round-4.md](experiments/round-4.md). The model is now run as
an ensemble. `cella_lib/examples/wildfire_smc` draws 32 members from one
untuned prior and outputs a burn-probability map (E24); in `assim` mode it
resamples members on each observed mask, mutates their parameters and
keeps simulating: a particle filter with GA operators whose scores are
one-window-ahead forecasts (E25). Forecast IoU 0.35-0.62 on all six
fires, holdout included, within 0.03-0.07 of the Circle and above it on
Chimney; Ferguson 0.13 to 0.34. Recommended: beta 10, sigma 0.2, 20 %
immigrants. The ad-hoc decay has a physical replacement: the FSim-style
containment-probability operator (E28) matches or beats it with the decay
off. Terrain wind (`cella_lib::wildfire::wind_field`, E26) and painted
retardant (`set_density`, E27) are in place but null/negative until the
kernel's wind response (E30) and an agent placement rule exist.

### 2026-09-05: the ensemble became a library feature (E31)

The wildfire-only ensemble was replaced by `cella_lib::explore`:
ensembles, evolution and MAP-Elites for any rule or model
([docs/explore.md](../docs/explore.md)). The wildfire model takes part
through `WildfireDriver` (weather schedule, decay, containment roll). E31
re-ran E25 and E28 through it with the unchanged runners: the recommended
configuration (containment only) replicates within 0.009 IoU on all six
fires; E28's +0.026 on Bear turned out to be noise, so containment-only
and the decay are a tie everywhere, and the operator is kept for being
the physical mechanism at no cost. Runs are now bit-reproducible for a
given seed.

### 2026-09-05: the methods themselves (Round 5)

[experiments/round-5.md](experiments/round-5.md), E32-E38. These tested
the ensemble and GA machinery rather than the fire model, every run on
the generic `explore` engine with the E31 base configuration. Five seeds
give a per-fire noise floor (sd <= 0.015, Buck 0.039); 32 members is the
knee; the filter's operators sit on a plateau; a narrow prior hurts and a
very broad one is free; the day-by-day filter beats a GA fitted to the
first three days on every fire; and a MAP-Elites reachability map shows
the model can be elongated only while small, so Brattain, Ferguson and
Pier are shapes it cannot draw at their size (the target for the E30
kernel refit). The filter's one failure mode, a population that has fully
stopped while the fire grows, is repaired by `immigrant_reset` (kept as
an option).

### 2026-09-05: experiment log rewritten

Every experiment file was rewritten to one shape for readability (numbers,
tables and verdicts unchanged) and [experiments/GLOSSARY.md](experiments/GLOSSARY.md)
was added.

### 2026-09-11/12: nulls, state correction, a kernel that tells time (Round 6)

[experiments/round-6.md](experiments/round-6.md), E41-E43, E30a, E30,
E30b. Wind's shape signal is mostly the *sign* (which end is the front),
not the stretch. The arrival-time kernel, validated on a flat grid,
loses to the E33 baseline on the six real fires as tested; a one-seed
pilot with a learned wind-direction gene (E30b, "Arm B") recovered it.
Observed-perimeter state correction (E40) beats E33 but loses to lagged
persistence. Every new mechanism is opt-in, off by default. Plain-language
version in [ANALYSIS.md section 5e](ANALYSIS.md).

### 2026-09-23/25: the promotion test (Round 7)

[experiments/round-7.md](experiments/round-7.md), E44-E49. The five-seed
Arm B run (E44) beats E33 on four of six fires but loses on Pier by
4.60 sd, tripping the pre-registered stop rule, so **Arm B is rejected as
tested**. Two mechanism tests (E45, E46) could not state the gene's gain
in one sentence; both were fire-specific. The containment operator has no
scaling bug under the 4x clock (E49). Plain-language version in
[ANALYSIS.md section 5f](ANALYSIS.md).
