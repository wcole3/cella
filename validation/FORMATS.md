# Validation Data Formats (v1)

Every data source — daily satellite masks, 3-hourly isochrone shapefiles,
hourly GOES perimeters, timestamped point observations — is converted into
**one canonical scenario layout** so the harness, the metrics, and the
calibration loop never need source-specific code. Converters absorb the mess;
everything downstream reads exactly this.

```
validation/data/scenarios/<scenario-id>/
├── scenario.json   # identity, provenance, grid, time base
├── config.json     # cella input: a normal CellaConfig (2d + wildfire model)
└── truth.json      # observed fire arrival, on the same grid
```

`<scenario-id>` is `<fire>_<year>` (e.g. `Bear_2020`, `Dogrib_2001`).

## Design decisions, and why

**Arrival time is the one canonical truth representation.** Sources publish
different things (cumulative daily masks, isochrone polygons, point
observations), but all of them are views of the same underlying field: *when
did each cell start burning*. We store that directly — one number per cell —
because every other view derives from it:

- burned mask at observation time `t`  =  `arrival <= t`
- burned area curve  =  count of `arrival <= t` per `t`
- arrival-time error  =  compare the field itself

Converting masks → arrival loses nothing (arrival = first time a mask shows
the cell burned); converting arrival → masks is one comparison. The reverse
choice (masks as canonical) would forever lock metrics to the coarsest
source's cadence.

**Truth carries its own accuracy statement.** GOES perimeters have ±1 km
edges; VIIRS-based masks ~375 m; NIST point interpolation varies locally. A
score is meaningless without knowing the truth's own error, so
`truth.json` must say what its `spatial_accuracy_m` is and where it came
from, and the harness prints it next to every score table.

**Provenance is mandatory.** `scenario.json` records the source dataset,
URL, license, retrieval date, converter script + git hash, and every
simplification the conversion made (e.g. "wind = domain-mean of ERA5 u/v").
The goal of validation is the *best answer*, not a good-looking one — a
result nobody can trace to its inputs is not an answer.

**Weather provenance is mandatory too.** Every weather-driven comparison
depends on how the wind was measured, averaged and oriented, and a wrong
convention looks exactly like a bad model in a score table
([TEST_PLAN.md §2.1](TEST_PLAN.md)). So `provenance.weather` records, for
each source: the variables and their height (10 m, 20 ft, mid-flame),
units, native time and space resolution, how the converter averaged them,
the direction convention of the source and the conversion applied, and
how the grid's north-up orientation was verified. A scenario without it
is not ready to be scored against anything.

**Time is hours since `t0`, as f64.** Daily sources use multiples of 24;
sub-daily sources need no schema change. `t0` is an ISO-8601 UTC timestamp in
`scenario.json`.

**Everything is flat row-major arrays in JSON.** Matches cella's own config
format, needs no geospatial libraries to read, diffs cleanly, and the files
are gitignored so size is a non-issue. Compression can come later without a
schema change.

## `scenario.json`

```jsonc
{
  "format_version": 2,
  "id": "Bear_2020",
  "grid": { "width": 748, "height": 619, "cell_size_m": 30.0,
            "crs": "EPSG:3310", "origin": [-99441.97, 205010.71] },
  "t0_utc": "2020-08-19T00:00:00Z",
  "provenance": {
    "source": "PyTorchFire six-fire pack (dataset.hdf5)",
    "source_url": "https://github.com/mzhen77/neural-ca-wildfire",
    "license": "CC-BY-4.0",
    "retrieved": "2026-08-14",
    "converter": "validation/scripts/convert_pytorchfire.py",
    "converter_git": "<hash>",
    "simplifications": [
      "wind = domain-mean ERA5 u/v per day (per-cell field discarded)",
      "FBFM40 codes grouped into 6 named classes with first-guess veg_factors"
    ],
    // Required (TEST_PLAN §2.1): what the weather feed is and how it was
    // bent into the schedule below. Free text per key, but every key present.
    "weather": {
      "source": "ERA5 reanalysis via the six-fire pack",
      "variables": "u/v 10 m wind (m/s), 2 m temperature, total precipitation",
      "wind_height_m": 10.0,
      "native_resolution": "~31 km grid, daily values",
      "averaging": "domain mean of u and v per day, then hypot -> speed (vector mean, not speed mean)",
      "source_direction_convention": "u eastward / v northward components",
      "conversion": "from_deg = atan2(-u, -v) from north, clockwise",
      "grid_orientation_check": "LANDFIRE aspect vs elevation gradient: cos +0.96 for row 0 = north on all six fires"
    }
  },
  // Wind schedule the harness applies between observation windows:
  // from_deg = compass bearing the wind blows FROM, 0 = north, clockwise
  // (the weather-report convention; v1 files used "dir_deg" = grid angle
  // the wind blew toward, 0 = +x, and are rejected by v2 readers).
  "wind": [ { "hours": 0.0, "speed_ms": 2.3, "from_deg": 51.2 }, ... ],
  "steps_per_hour": 2.0833   // simulation ticks per hour of real time
}
```

`steps_per_hour` makes the tick↔wall-clock mapping explicit and per-scenario
(the six-fire pack follows the papers' 50 ticks/day). It is a *declared
conversion constant*, not a tuning knob — changing it is a calibration
decision and goes through the test plan.

## `config.json`

Unchanged cella `CellaConfig` (dim 2d, empty subrules, wildfire model with
ignition cells pre-set from the observations at `t0`). The harness overrides
`seed` per ensemble member and wind per schedule entry; everything else in
here is part of the parameter set under evaluation.

## `truth.json`

```jsonc
{
  "format_version": 2,
  "time_unit": "hours_since_t0",
  // One observation per time the source actually observed (not interpolated):
  "observed_at": [0.0, 24.0, 48.0, ...],
  // Flat row-major, one f64 per cell:
  //   >= 0  : hours since t0 when the cell was FIRST observed burning/burned
  //   -1    : never observed burned during the record
  "arrival_hours": [ ... ],
  "spatial_accuracy_m": 375.0,
  "accuracy_note": "VIIRS-derived daily masks; arrival quantized to the observation days"
}
```

Arrival values are quantized to entries of `observed_at` for mask-derived
sources (a cell first seen burned on day 3 gets 72.0, though it ignited
sometime in (48, 72]). Metrics that care (arrival-time error) must treat
this quantization as part of `spatial`/temporal accuracy — the harness's
arrival metric reports it alongside.

## What converters must and must not do

- MUST populate every field above; no optional provenance.
- MUST derive ignition (`Burning` cells in `config.json`) only from
  observations at `t0` — never from later truth.
- MUST NOT bake calibrated parameters into `config.json` silently: the
  parameters written there are the *declared defaults* under test; calibrated
  variants are produced by the calibration loop into separate,
  clearly-labelled scenario copies.
- MUST record every simplification that discards source information
  (per-cell wind → mean, severity → binary, etc.) in `simplifications`.

## Current converters

| Source | Script | Status |
|---|---|---|
| PyTorchFire six-fire HDF5 | `scripts/convert_pytorchfire.py` | emits v1 |
| Isochrone polygons (PT-FireSprd, GOFER, NIROPS, FIRIS) | `scripts/rasterize_isochrones.py` | planned — one shared tool; needs input-layer assembly per region before full scenarios exist |
| Dogrib `.asc` (Cell2Fire instance) | planned | inputs are drop-in; observed-perimeter truth still to be sourced (Prometheus sample data) |
| Camp Fire NIST points | planned | needs point → arrival-surface interpolation; scenario will carry large `spatial_accuracy_m` variation |

## Changelog

- **v2 (2026-09-01):** wind entries carry `from_deg` — the compass bearing
  the wind blows *from*, 0° = north, clockwise (weather-report convention),
  matching `WildfireParams::wind_from_deg`. v1 used `dir_deg`, the grid
  angle the wind blew *toward* (0° = +x, 90° = +y). Convert with
  `from = (dir − 90) mod 360`. Readers reject v1 files rather than guess.
- **v1 (2026-08-14):** initial layout.
