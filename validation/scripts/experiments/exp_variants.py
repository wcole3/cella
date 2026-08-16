#!/usr/bin/env python
"""E2-E5: physics/input variants at each calibration fire's best (p0, dur)
operating point from the E1 scan. Single seed, calibration fires only.

Variants:
  ctrl        best (p0, dur), nothing else       (reference)
  precip_k*   per-day p0 scale from rain         (E3 moisture proxy)
  temp_vpd    per-day p0 scale from temp anomaly (E3)
  combo_k1.0  rain x temp                        (E3)
  wind_x2/x4  wind speed multiplier              (E5 gust factor)
  veg_wide    wider veg_factor spread            (E4)
  density_cc  per-cell density from canopy cover (E2)

Uses the uncommitted wildfire_experiment example (env-var hooks).
"""

import json
import shutil
import subprocess
from pathlib import Path

import h5py
import numpy as np

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp2_reports"
FIRES = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016"]

VEG_WIDE = {"Grass": 2.0, "GrassShrub": 1.4, "Shrub": 1.0,
            "TimberUnder": 0.7, "TimberLitter": 0.3, "Slash": 0.8}


def best_points():
    rows = json.loads((EXP / "exp1_scan.json").read_text())
    best = {}
    for r in rows:
        if r["fire"] not in best or r["mean_iou"] > best[r["fire"]]["mean_iou"]:
            best[r["fire"]] = r
    return best


def cc_density(fire, h5):
    """Density in [0.5, 1.0]: denser canopy carries fire a bit better.
    Cells with no canopy (grass) stay at 1.0 so grass isn't punished for
    having no trees — CC only downweights SPARSE canopy within forest."""
    cc = h5[fire]["230CC"][()].astype(np.float64)
    fbfm = h5[fire]["230FBFM40"][()]
    dens = np.ones_like(cc)
    forest = cc > 0
    dens[forest] = 0.5 + 0.5 * (cc[forest] / 75.0)
    return dens.ravel().tolist()


def run(fire, p0, dur, label, env_extra=None, cfg_mutate=None, sched=None):
    tmp = EXP / "tmp_variant"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    cfg_path = tmp / "config.json"
    cfg = json.loads(cfg_path.read_text())
    params = cfg["model"]["wildfire"]["params"]
    params["p0"] = p0
    params["burn_duration"] = dur
    if cfg_mutate:
        cfg_mutate(cfg)
    cfg_path.write_text(json.dumps(cfg))

    env = dict(env_extra or {})
    if sched:
        # Slice the per-date schedule to one multiplier per stepped window.
        sc = json.loads((tmp / "scenario.json").read_text())
        vals = json.loads((EXP / "schedules" / fire / sched).read_text())
        need = len(sc["wind"]) - 1
        sched_path = tmp / "p0_scale.json"
        sched_path.write_text(json.dumps(vals[:need]))
        env["EXP_P0_SCALE"] = str(sched_path)

    report_path = OUT / f"{fire}_{label}.json"
    subprocess.run(
        ["cargo", "run", "--release", "--example", "wildfire_experiment",
         "--", str(tmp), "1", str(report_path)],
        cwd=REPO / "cella_lib", check=True, capture_output=True,
        env={**__import__("os").environ, **env},
    )
    r = json.loads(report_path.read_text())
    series = r["model"][1:]
    radial = r["radial"][1:]
    row = {
        "fire": fire, "variant": label,
        "mean_iou": sum(s["iou"] for s in series) / len(series),
        "final_iou": r["final_iou_model"],
        "mean_iou_radial": sum(s["iou"] for s in radial) / len(radial),
        "area_ratio": series[-1]["sim_burned"] / series[-1]["obs_burned"],
    }
    print(f"{fire:15s} {label:12s} mean IoU {row['mean_iou']:.3f} "
          f"(circle {row['mean_iou_radial']:.3f}) area x{row['area_ratio']:.1f}",
        flush=True)
    return row


def main():
    import sys
    moisture_only = len(sys.argv) > 1 and sys.argv[1] == "moisture"
    OUT.mkdir(parents=True, exist_ok=True)
    best = best_points()
    h5 = h5py.File(VAL / "data" / "dataset.hdf5")

    def veg_wide(cfg):
        for f in cfg["model"]["wildfire"]["params"]["fuels"]:
            f["veg_factor"] = VEG_WIDE[f["name"]]

    rows = []
    for fire in FIRES:
        b = best[fire]
        p0, dur = b["p0"], b["dur"]
        print(f"== {fire}: operating point p0={p0} dur={dur} "
              f"(scan mean IoU {b['mean_iou']:.3f})", flush=True)
        if moisture_only:
            rows.append(run(fire, p0, dur, "precip_k0.3", sched="precip_k0.3.json"))
            rows.append(run(fire, p0, dur, "precip_k1.0", sched="precip_k1.0.json"))
            rows.append(run(fire, p0, dur, "temp_vpd", sched="temp_vpd.json"))
            rows.append(run(fire, p0, dur, "combo_k1.0", sched="combo_k1.0.json"))
            continue
        rows.append(run(fire, p0, dur, "ctrl"))
        rows.append(run(fire, p0, dur, "precip_k0.3", sched="precip_k0.3.json"))
        rows.append(run(fire, p0, dur, "precip_k1.0", sched="precip_k1.0.json"))
        rows.append(run(fire, p0, dur, "temp_vpd", sched="temp_vpd.json"))
        rows.append(run(fire, p0, dur, "combo_k1.0", sched="combo_k1.0.json"))
        rows.append(run(fire, p0, dur, "wind_x2", env_extra={"EXP_WIND_SCALE": "2"}))
        rows.append(run(fire, p0, dur, "wind_x4", env_extra={"EXP_WIND_SCALE": "4"}))
        rows.append(run(fire, p0, dur, "veg_wide", cfg_mutate=veg_wide))
        dens = cc_density(fire, h5)
        rows.append(run(fire, p0, dur, "density_cc",
                        cfg_mutate=lambda c, d=dens: c["model"]["wildfire"]["env"].__setitem__("density", d)))
    out_name = "exp2b_moisture.json" if moisture_only else "exp2_variants.json"
    (EXP / out_name).write_text(json.dumps(rows, indent=1))
    print("done")


if __name__ == "__main__":
    main()
