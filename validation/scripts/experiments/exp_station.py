#!/usr/bin/env python
"""E14 / E15: real hourly station weather instead of ERA5 daily means.

E14 (mode `wind`): replace the daily ERA5 wind schedule with the nearest
NOAA ISD station's hourly wind (`station_hourly.json`, from
`scripts/wind_station.py`). Variants scale the 10 m station wind by a
factor (1.0 as measured; 0.5 ~ a mid-flame wind adjustment; 2.0 ~ a gust /
ridge-top proxy). Control = ERA5 daily as converted.

E15 (mode `moisture`): keep the hourly station wind (x1) and additionally
scale p0 every hour by a fuel-moisture damping factor built from the
station's RH and temperature — the mechanism operational CA simulators use
to make fire stop at night and on humid days (PROPAGATOR, Trucchia et al.
2020, after Burgan & Rothermel 1984):

    EMC (Fosberg / Simard 1-h fuel moisture, %) from RH and T
    r   = EMC / M_x            (M_x = moisture of extinction)
    eta = 1 - 2.59 r + 5.11 r^2 - 3.52 r^3, clipped to [0, 1]  (Rothermel 1972)

Because damping lowers the average p0, each fire's E1 p0 is re-scanned by
a multiplier {1, 1.5, 2, 3}. A `night` variant (p0 x 0.3 from 20:00 to
08:00 local, no moisture) separates the plain diurnal effect from the
humidity physics.

Both modes run the four calibration fires, 3 seeds, at the E1 (p0, dur)
recipes, writing exp14_station.json / exp15_moisture.json.
"""
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import numpy as np

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
BIN = REPO / "cella_lib/target/release/examples/wildfire_experiment"
BEST = {"Bear_2020": (0.12, 10), "Brattain_2020": (0.22, 10),
        "Buck_2017": (0.16, 5), "Chimney_2016": (0.30, 5)}
SEEDS = 3
UTC_OFFSET_H = -7  # PDT for all four fires (Aug-Oct, California / Oregon)


def emc_fosberg(rh: float, temp_c: float) -> float:
    """1-hour fine dead fuel equilibrium moisture content, percent.
    Fosberg & Deeming 1971 / Simard 1968, as used in the Fosberg FFWI."""
    t_f = temp_c * 9.0 / 5.0 + 32.0
    h = rh
    if h < 10.0:
        return 0.03229 + 0.281073 * h - 0.000578 * h * t_f
    if h < 50.0:
        return 2.22749 + 0.160107 * h - 0.01478 * t_f
    return 21.0606 + 0.005565 * h * h - 0.00035 * h * t_f - 0.483199 * h


def eta_moisture(emc_pct: float, mx_pct: float) -> float:
    r = min(max(emc_pct / mx_pct, 0.0), 1.0)
    return max(0.0, min(1.0, 1.0 - 2.59 * r + 5.11 * r * r - 3.52 * r ** 3))


def hourly_windows(fire: str, wind_scale: float):
    """Scenario wind entries, one per hour, from the station table."""
    st = json.loads((VAL / "data" / "scenarios" / fire / "station_hourly.json").read_text())
    rows = st["rows"]
    wind = [{"hours": r["hours"], "speed_ms": round(r["speed_ms"] * wind_scale, 3),
             "from_deg": r["from_deg"]} for r in rows]
    return wind, rows, st


def run(fire, p0, dur, label, wind=None, p0_scale=None, extra_env=None):
    tmp = EXP / "tmp_station"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    cfg = json.loads((tmp / "config.json").read_text())
    cfg["model"]["wildfire"]["params"]["p0"] = p0
    cfg["model"]["wildfire"]["params"]["burn_duration"] = dur
    (tmp / "config.json").write_text(json.dumps(cfg))
    sc = json.loads((tmp / "scenario.json").read_text())
    if wind is not None:
        sc["wind"] = wind
        sc["provenance"]["simplifications"].append("wind replaced by hourly ISD station record")
    (tmp / "scenario.json").write_text(json.dumps(sc))
    env = dict(os.environ, **(extra_env or {}))
    if p0_scale is not None:
        assert len(p0_scale) == len(sc["wind"]) - 1
        (tmp / "p0_scale.json").write_text(json.dumps(p0_scale))
        env["EXP_P0_SCALE"] = str(tmp / "p0_scale.json")
    rep = OUT / f"{fire}_{label}.json"
    subprocess.run([str(BIN), str(tmp), str(SEEDS), str(rep)], check=True,
                   capture_output=True, env=env)
    r = json.loads(rep.read_text())
    s, rad = r["model"][1:], r["radial"][1:]
    row = {"fire": fire, "variant": label, "seeds": SEEDS, "p0": p0, "dur": dur,
           "mean_iou": sum(x["iou"] for x in s) / len(s), "final_iou": r["final_iou_model"],
           "mean_iou_radial": sum(x["iou"] for x in rad) / len(rad),
           "area_ratio": s[-1]["sim_burned"] / s[-1]["obs_burned"],
           "arrival_mae": r["model_arrival_mae_hours"]}
    beat = "BEATS" if row["mean_iou"] > row["mean_iou_radial"] else "loses"
    print(f"{fire:14s} {label:16s} mean IoU {row['mean_iou']:.3f} final {row['final_iou']:.3f} "
          f"(circle {row['mean_iou_radial']:.3f} {beat}) area x{row['area_ratio']:.2f} "
          f"MAE {row['arrival_mae']:.0f}h", flush=True)
    return row


def mode_wind():
    rows = []
    for fire, (p0, dur) in BEST.items():
        rows.append(run(fire, p0, dur, "era5_daily"))
        for k in (1.0, 0.5, 2.0):
            wind, _, _ = hourly_windows(fire, k)
            rows.append(run(fire, p0, dur, f"station_x{k:g}", wind=wind))
    (EXP / "exp14_station.json").write_text(json.dumps(rows, indent=1))


def mode_moisture():
    rows = []
    for fire, (p0, dur) in BEST.items():
        wind, st_rows, st = hourly_windows(fire, 1.0)
        n = len(wind) - 1
        t0_hour_utc = 0  # t0 is midnight UTC
        night = []
        for i in range(n):
            local = (t0_hour_utc + i + UTC_OFFSET_H) % 24
            night.append(0.3 if (local >= 20 or local < 8) else 1.0)
        rows.append(run(fire, p0, dur, "station_x1", wind=wind))
        for mult in (1.0, 1.5, 2.0):
            rows.append(run(fire, p0 * mult, dur, f"night_p{mult:g}", wind=wind, p0_scale=night))
        for mx in (25.0, 35.0):
            eta = []
            for r in st_rows[:n]:
                rh, t = r["rh_pct"], r["temp_c"]
                if rh is None or t is None:
                    eta.append(1.0)
                else:
                    eta.append(eta_moisture(emc_fosberg(rh, t), mx))
            mean_eta = float(np.mean(eta))
            for mult in (1.0, 1.5, 2.0, 3.0):
                rows.append(run(fire, p0 * mult, dur, f"moist{mx:g}_p{mult:g}", wind=wind, p0_scale=eta))
            rows[-1]["mean_eta"] = mean_eta
            print(f"   (mean eta {mean_eta:.2f} at M_x {mx:g}%)")
    (EXP / "exp15_moisture.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else "wind"
    OUT = EXP / ("exp14_station" if mode == "wind" else "exp15_moisture")
    OUT.mkdir(parents=True, exist_ok=True)
    (mode_wind if mode == "wind" else mode_moisture)()
