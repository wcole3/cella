#!/usr/bin/env python3
"""Convert the PyTorchFire six-fire HDF5 pack into cella validation inputs.

For each fire in `validation/data/dataset.hdf5` this writes, under
`validation/data/converted/<fire>/`:

- `config.json`  — a cella `CellaConfig` (2d + wildfire model): fuel classes
  from LANDFIRE FBFM40 codes, elevation from ELEV, ignition cells from the
  first day's observed fire mask.
- `truth.json`   — the observed cumulative burned mask for every day, as rows
  of '0'/'1' characters (compact and trivial to parse from Rust).
- `meta.json`    — the per-day uniform wind schedule (domain-mean ERA5 u/v ->
  speed m/s + direction degrees, 0 = +x/east, 90 = +y/south i.e. grid-down),
  dates, and grid dimensions.

Everything here is a *starting* mapping, deliberately simple; calibration
comes later. Run with the venv python:

    validation/.venv/bin/python validation/scripts/convert_pytorchfire.py
"""

import json
import math
import sys
from pathlib import Path

import h5py
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "data"
OUT = DATA / "converted"

# FBFM40 fuel model code -> (cella fuel class name, veg_factor).
# Grouped by the standard Scott & Burgan families; factors are a first-guess
# relative flammability ordering (grass fastest, timber litter slowest) to be
# calibrated against the observed perimeters later.
FBFM40_GROUPS = [
    (range(101, 110), "Grass", 1.2),      # GR1-GR9
    (range(121, 125), "GrassShrub", 1.0), # GS1-GS4
    (range(141, 150), "Shrub", 0.9),      # SH1-SH9
    (range(161, 166), "TimberUnder", 0.8),# TU1-TU5
    (range(181, 190), "TimberLitter", 0.5),# TL1-TL9
    (range(201, 205), "Slash", 0.7),      # SB1-SB4
]
# 91-99: urban, snow/ice, agriculture, water, barren -> unburnable.


def fuel_name(code: int) -> str | None:
    """Class name for an FBFM40 code, or None for unburnable/nodata."""
    for rng, name, _ in FBFM40_GROUPS:
        if code in rng:
            return name
    return None


def wind_to_speed_dir(u: float, v: float) -> tuple[float, float]:
    """ERA5 u (eastward) / v (northward) -> (speed m/s, direction degrees).

    cella's convention: direction the wind blows TOWARD, 0 deg = +x (east),
    90 deg = +y which is *down* the grid. Row 0 of the raster is the northern
    edge, so northward v means -y in grid coordinates: dir = atan2(-v, u).
    """
    speed = math.hypot(u, v)
    direction = math.degrees(math.atan2(-v, u)) % 360.0
    return speed, direction


def convert_fire(f: h5py.File, name: str, steps_per_day: int) -> None:
    g = f[name]
    h, w = g.attrs["height"], g.attrs["width"]
    fuel_codes = g["230FBFM40"][()]
    elevation = g["ELEV2020"][()].astype(np.float32)
    dates = sorted(g["fire"].keys())
    masks = {d: (g["fire"][d][()] > 0) for d in dates}

    # Initial cell types: fuel class name, or Inactive for unburnable, with
    # the first observed mask as the ignition perimeter.
    names = np.full((h, w), "Inactive", dtype=object)
    for rng, cls, _ in FBFM40_GROUPS:
        sel = np.isin(fuel_codes, list(rng))
        names[sel] = cls
    day0 = masks[dates[0]]
    names[day0] = "Burning"

    fuels = [{"name": cls, "veg_factor": vf} for _, cls, vf in FBFM40_GROUPS]

    # Domain-mean wind per day.
    wind = []
    for d in dates:
        u = float(np.nanmean(g["u_component_of_wind_10m"][d][()]))
        v = float(np.nanmean(g["v_component_of_wind_10m"][d][()]))
        speed, direction = wind_to_speed_dir(u, v)
        wind.append({"date": d, "speed": round(speed, 3), "dir_deg": round(direction, 2)})

    config = {
        "dim": "2d",
        "width": int(w),
        "height": int(h),
        "history_limit": 0,
        "initial": names.ravel().tolist(),
        "rule": {"subrules": []},
        "model": {"wildfire": {
            "params": {
                "seed": 0,  # the harness overrides this per ensemble member
                "p0": 0.58,
                "fuels": fuels,
                "wind_speed": wind[0]["speed"],
                "wind_dir_deg": wind[0]["dir_deg"],
                "c1": 0.045,
                "c2": 0.131,
                "slope_a": 0.078,
                "cell_size": 30.0,
                # Cells stay burning for a fraction of a day; calibration knob.
                "burn_duration": 5,
                "spotting": None,
            },
            "env": {"density": [], "elevation": elevation.ravel().tolist()},
        }},
    }

    truth = {
        "dates": dates,
        "masks": {d: ["".join("1" if v else "0" for v in row) for row in masks[d]] for d in dates},
    }
    meta = {
        "fire": name,
        "width": int(w),
        "height": int(h),
        "resolution_m": int(g.attrs["resolution"]),
        "steps_per_day": steps_per_day,
        "wind": wind,
        "burnable_cells": int(sum(1 for n in names.ravel() if n not in ("Inactive",))),
        "day0_burned": int(day0.sum()),
        "final_burned": int(masks[dates[-1]].sum()),
    }

    out = OUT / name
    out.mkdir(parents=True, exist_ok=True)
    (out / "config.json").write_text(json.dumps(config))
    (out / "truth.json").write_text(json.dumps(truth))
    (out / "meta.json").write_text(json.dumps(meta, indent=1))
    print(f"{name}: {w}x{h}, {len(dates)} days, day0 {meta['day0_burned']} -> final {meta['final_burned']} burned cells")


def main() -> None:
    steps_per_day = int(sys.argv[1]) if len(sys.argv) > 1 else 50
    with h5py.File(DATA / "dataset.hdf5", "r") as f:
        for name in f.keys():
            convert_fire(f, name, steps_per_day)


if __name__ == "__main__":
    main()
