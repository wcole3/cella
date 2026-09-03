#!/usr/bin/env python
"""E10: wind-direction ORACLE — an upper bound, not a model.

Replaces each day's ERA5 domain-mean wind with the direction the observed
fire actually grew that day (centroid of the day's new burn relative to
the previous burned set), at a fixed strong speed. This CHEATS by reading
the truth, so it is never a result to report as skill. Its only purpose is
to answer: if we had a perfect wind input, how much would this wind kernel
buy? If the oracle barely beats ERA5, better wind data is not the priority.

Variants at each calibration fire's E1 (p0, dur):
  era5        as converted (control)
  oracle_v{V} truth growth direction, speed V m/s, on days with >=50 new
              cells; calm days keep ERA5 wind.
Calibration fires only, 3 seeds.
"""
import json, os, shutil, subprocess, sys
from pathlib import Path
import numpy as np

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp10_oracle"
BIN = REPO / "cella_lib/target/release/examples/wildfire_experiment"
BEST = {"Bear_2020": (0.12, 10), "Brattain_2020": (0.22, 10),
        "Buck_2017": (0.16, 5), "Chimney_2016": (0.30, 5)}
SEEDS = 3
SPEEDS = [2.0, 5.0, 8.0]


def growth_dirs(fire):
    d = VAL / "data" / "scenarios" / fire
    sc = json.loads((d / "scenario.json").read_text())
    tr = json.loads((d / "truth.json").read_text())
    w, h = sc["grid"]["width"], sc["grid"]["height"]
    arr = np.array(tr["arrival_hours"]).reshape(h, w)
    ys, xs = np.mgrid[0:h, 0:w]
    dirs = {}
    obs = tr["observed_at"]
    for a, b in zip(obs[:-1], obs[1:]):
        prev = (arr >= 0) & (arr <= a)
        new = arr == b
        if new.sum() < 50:
            continue
        gdir = np.degrees(np.arctan2(ys[new].mean() - ys[prev].mean(),
                                     xs[new].mean() - xs[prev].mean())) % 360
        dirs[round(a)] = float(gdir)
    return dirs


def run(fire, p0, dur, label, speed=None, dirs=None):
    tmp = EXP / "tmp_oracle"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    cfg = json.loads((tmp / "config.json").read_text())
    cfg["model"]["wildfire"]["params"]["p0"] = p0
    cfg["model"]["wildfire"]["params"]["burn_duration"] = dur
    (tmp / "config.json").write_text(json.dumps(cfg))
    if dirs is not None:
        sc = json.loads((tmp / "scenario.json").read_text())
        for wnd in sc["wind"]:
            k = round(wnd["hours"])
            if k in dirs:
                # dirs[] is the grid angle the fire moved TOWARD (0 = +x,
                # 90 = +y); the schedule wants the bearing the wind comes FROM.
                wnd["from_deg"] = round((dirs[k] - 90.0) % 360.0, 2)
                wnd["speed_ms"] = speed
        (tmp / "scenario.json").write_text(json.dumps(sc))
    rep = OUT / f"{fire}_{label}.json"
    subprocess.run([str(BIN), str(tmp), str(SEEDS), str(rep)], check=True, capture_output=True)
    r = json.loads(rep.read_text())
    s, rad = r["model"][1:], r["radial"][1:]
    row = {"fire": fire, "variant": label, "seeds": SEEDS,
           "mean_iou": sum(x["iou"] for x in s) / len(s), "final_iou": r["final_iou_model"],
           "mean_iou_radial": sum(x["iou"] for x in rad) / len(rad),
           "area_ratio": s[-1]["sim_burned"] / s[-1]["obs_burned"],
           "arrival_mae": r["model_arrival_mae_hours"]}
    print(f"{fire:14s} {label:10s} mean IoU {row['mean_iou']:.3f} final {row['final_iou']:.3f} "
          f"(circle {row['mean_iou_radial']:.3f}) area x{row['area_ratio']:.2f} MAE {row['arrival_mae']:.0f}h", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    for fire, (p0, dur) in BEST.items():
        dirs = growth_dirs(fire)
        rows.append(run(fire, p0, dur, "era5"))
        for v in SPEEDS:
            rows.append(run(fire, p0, dur, f"oracle_v{v:g}", v, dirs))
    (EXP / "exp10_oracle.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
