#!/usr/bin/env python3
"""Convert the PyTorchFire six-fire HDF5 pack into canonical v1 scenarios.

See validation/FORMATS.md for the layout this emits. For each fire in
`validation/data/dataset.hdf5` this writes
`validation/data/scenarios/<fire>/{scenario,config,truth}.json`.

Run with the venv python:

    validation/.venv/bin/python validation/scripts/convert_pytorchfire.py
"""

import datetime as dt
import json
import math
import subprocess
import sys
from pathlib import Path

import h5py
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "data"
OUT = DATA / "scenarios"

STEPS_PER_DAY = 50  # follows the papers published on this dataset
FORMAT_VERSION = 1

# FBFM40 fuel model code -> (cella fuel class name, veg_factor).
# Grouped by the standard Scott & Burgan families; factors are a first-guess
# relative flammability ordering (grass fastest, timber litter slowest).
# These are DECLARED DEFAULTS under test, not calibrated values — see
# validation/TEST_PLAN.md for how calibration is allowed to change them.
FBFM40_GROUPS = [
    (range(101, 110), "Grass", 1.2),       # GR1-GR9
    (range(121, 125), "GrassShrub", 1.0),  # GS1-GS4
    (range(141, 150), "Shrub", 0.9),       # SH1-SH9
    (range(161, 166), "TimberUnder", 0.8), # TU1-TU5
    (range(181, 190), "TimberLitter", 0.5),# TL1-TL9
    (range(201, 205), "Slash", 0.7),       # SB1-SB4
]
# 91-99: urban, snow/ice, agriculture, water, barren -> unburnable.


def git_hash() -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"], cwd=ROOT, text=True
        ).strip()
    except Exception:
        return "unknown"


def wind_to_speed_dir(u: float, v: float) -> tuple[float, float]:
    """ERA5 u (eastward) / v (northward) -> (speed m/s, direction degrees).

    cella's convention: direction the wind blows TOWARD, 0 deg = +x (east),
    90 deg = +y which is *down* the grid. Row 0 of the raster is the northern
    edge, so northward v means -y in grid coordinates: dir = atan2(-v, u).
    """
    speed = math.hypot(u, v)
    direction = math.degrees(math.atan2(-v, u)) % 360.0
    return speed, direction


def convert_fire(f: h5py.File, name: str) -> None:
    g = f[name]
    h, w = int(g.attrs["height"]), int(g.attrs["width"])
    fuel_codes = g["230FBFM40"][()]
    elevation = g["ELEV2020"][()].astype(np.float32)
    dates = sorted(g["fire"].keys())  # chronological
    t0 = dt.datetime.fromisoformat(dates[0])
    hours = [ (dt.datetime.fromisoformat(d) - t0).total_seconds() / 3600.0 for d in dates ]

    # Arrival field: first observation time each cell shows burned; -1 never.
    arrival = np.full((h, w), -1.0)
    for d, hrs in zip(reversed(dates), reversed(hours)):
        burned = g["fire"][d][()] > 0
        arrival[burned] = hrs  # earlier dates overwrite later ones

    # Initial cell types: fuel class name, Inactive for unburnable, and the
    # t0 observation as the Burning ignition set (never later truth).
    names = np.full((h, w), "Inactive", dtype=object)
    for rng, cls, _ in FBFM40_GROUPS:
        names[np.isin(fuel_codes, list(rng))] = cls
    day0 = g["fire"][dates[0]][()] > 0
    names[day0] = "Burning"

    wind = []
    for d, hrs in zip(dates, hours):
        u = float(np.nanmean(g["u_component_of_wind_10m"][d][()]))
        v = float(np.nanmean(g["v_component_of_wind_10m"][d][()]))
        speed, direction = wind_to_speed_dir(u, v)
        wind.append({"hours": hrs, "speed_ms": round(speed, 3), "dir_deg": round(direction, 2)})

    scenario = {
        "format_version": FORMAT_VERSION,
        "id": name,
        "grid": {
            "width": w, "height": h,
            "cell_size_m": float(g.attrs["resolution"]),
            "crs": str(g.attrs["crs"]),
            "origin": [float(x) for x in g.attrs["bounds_in_3310"][:2]],
        },
        "t0_utc": t0.strftime("%Y-%m-%dT00:00:00Z"),
        "provenance": {
            "source": "PyTorchFire six-fire pack (dataset.hdf5)",
            "source_url": "https://github.com/mzhen77/neural-ca-wildfire",
            "license": "CC-BY-4.0",
            "retrieved": "2026-08-14",
            "converter": "validation/scripts/convert_pytorchfire.py",
            "converter_git": git_hash(),
            "simplifications": [
                "wind = domain-mean ERA5 u/v per day (per-cell field discarded)",
                "FBFM40 codes grouped into 6 named classes with first-guess veg_factors",
                "canopy cover / LAI layers unused (density left uniform)",
                "arrival quantized to daily observation times",
            ],
        },
        "wind": wind,
        "steps_per_hour": STEPS_PER_DAY / 24.0,
    }

    config = {
        "dim": "2d",
        "width": w,
        "height": h,
        "history_limit": 0,
        "initial": names.ravel().tolist(),
        "rule": {"subrules": []},
        "model": {"wildfire": {
            "params": {
                "seed": 0,  # harness overrides per ensemble member
                "p0": 0.58,
                "fuels": [{"name": cls, "veg_factor": vf} for _, cls, vf in FBFM40_GROUPS],
                "wind_speed": wind[0]["speed_ms"],
                "wind_dir_deg": wind[0]["dir_deg"],
                "c1": 0.045,
                "c2": 0.131,
                "slope_a": 0.078,
                "cell_size": float(g.attrs["resolution"]),
                "burn_duration": 5,
                "spotting": None,
            },
            "env": {"density": [], "elevation": elevation.ravel().tolist()},
        }},
    }

    truth = {
        "format_version": FORMAT_VERSION,
        "time_unit": "hours_since_t0",
        "observed_at": hours,
        "arrival_hours": arrival.ravel().tolist(),
        "spatial_accuracy_m": 375.0,
        "accuracy_note": "VIIRS-derived daily cumulative masks; arrival quantized to observation days",
    }

    out = OUT / name
    out.mkdir(parents=True, exist_ok=True)
    (out / "scenario.json").write_text(json.dumps(scenario, indent=1))
    (out / "config.json").write_text(json.dumps(config))
    (out / "truth.json").write_text(json.dumps(truth))
    burned_final = int((arrival >= 0).sum())
    print(f"{name}: {w}x{h}, {len(dates)} observations over {hours[-1]:.0f}h, "
          f"ignition {int(day0.sum())} -> final {burned_final} burned cells")


def main() -> None:
    with h5py.File(DATA / "dataset.hdf5", "r") as f:
        for name in f.keys():
            convert_fire(f, name)


if __name__ == "__main__":
    main()
