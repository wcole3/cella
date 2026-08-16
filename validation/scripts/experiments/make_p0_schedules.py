#!/usr/bin/env python
"""Build per-day p0 multiplier schedules from ERA5 precip + temperature.

For each calibration fire, writes JSON arrays (one multiplier per stepped
wind window) under scratchpad/schedules/:

- precip_k{K}.json:   scale = exp(-K * wet_mm), wet = precip + 0.5*prev_wet
                      (rain suppresses spread, with one-day carryover)
- temp_vpd.json:      scale from temperature anomaly: 1 + 0.04*(T - mean_T),
                      clamped to [0.4, 1.6] (hot day = drier = faster)
- combo_k{K}.json:    both effects multiplied
"""

import json
from pathlib import Path

import h5py
import numpy as np

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "schedules"
FIRES = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016"]
KS = [0.3, 1.0]

f = h5py.File(VAL / "data" / "dataset.hdf5")
OUT.mkdir(parents=True, exist_ok=True)
for fire in FIRES:
    g = f[fire]
    dates = sorted(g["u_component_of_wind_10m"].keys())
    # The scenario's wind entries follow these dates in order; windows = len-1.
    n_windows = len(dates) - 1
    precip = np.array([float(np.nanmean(g["total_precipitation_sum"][d][()])) * 1000.0
                       for d in dates[:n_windows]])
    temp = np.array([float(np.nanmean(g["temperature_2m"][d][()])) - 273.15
                     for d in dates[:n_windows]])

    wet = np.zeros(n_windows)
    carry = 0.0
    for i, p in enumerate(precip):
        carry = p + 0.5 * carry
        wet[i] = carry

    tanom = np.clip(1.0 + 0.04 * (temp - temp.mean()), 0.4, 1.6)

    d = OUT / fire
    d.mkdir(parents=True, exist_ok=True)
    for k in KS:
        rain = np.exp(-k * wet)
        (d / f"precip_k{k}.json").write_text(json.dumps(list(rain)))
        (d / f"combo_k{k}.json").write_text(json.dumps(list(rain * tanom)))
    (d / "temp_vpd.json").write_text(json.dumps(list(tanom)))
    print(f"{fire}: {n_windows} windows, precip total {precip.sum():.1f} mm, "
          f"rainy days {(precip > 0.5).sum()}, temp {temp.min():.0f}-{temp.max():.0f} C")
