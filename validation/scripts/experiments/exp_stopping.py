#!/usr/bin/env python
"""E16: two more ways to make the fire stop, tested against the explosion.

The model either dies or burns everything reachable (percolation cliff,
E1/E11). Two practices from the literature that are not weather:

E16a `hetero`  — spatial heterogeneity of flammability. Draw a per-cell
    density multiplier from a lognormal with mean 1 and spread sigma
    (seeded, so reproducible), leaving the mean p0 unchanged. Percolation
    theory says heterogeneity moves and softens the threshold; real fuel
    beds are patchy at 30 m. sigma in {0.3, 0.6, 1.0}; p0 x {1, 1.5, 2}.
E16b `contain` — a containment proxy. Real fires are fought harder the
    longer they burn; the model has no crews. Scale p0 by exp(-t / tau)
    with tau in {5, 10, 20} days, p0 x {1, 1.5, 2} so the early fire is
    not starved. Crude, but it is the shape every observed area curve
    has (ANALYSIS.md: growth then plateau) and it tests whether a
    time-decay alone can buy IoU or only trades area for timing.

Calibration fires only, 3 seeds, E1 (p0, dur) recipes. Output
exp16_stopping.json. Uses its own tmp dir so it can run beside E15.
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
OUT = EXP / "exp16_stopping"
BIN = REPO / "cella_lib/target/release/examples/wildfire_experiment"
BEST = {"Bear_2020": (0.12, 10), "Brattain_2020": (0.22, 10),
        "Buck_2017": (0.16, 5), "Chimney_2016": (0.30, 5)}
SEEDS = 3


def run(fire, p0, dur, label, density=None, p0_scale=None):
    tmp = EXP / "tmp_stopping"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    cfg = json.loads((tmp / "config.json").read_text())
    cfg["model"]["wildfire"]["params"]["p0"] = p0
    cfg["model"]["wildfire"]["params"]["burn_duration"] = dur
    if density is not None:
        cfg["model"]["wildfire"]["env"]["density"] = density
    (tmp / "config.json").write_text(json.dumps(cfg))
    env = dict(os.environ)
    if p0_scale is not None:
        sc = json.loads((tmp / "scenario.json").read_text())
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
    print(f"{fire:14s} {label:18s} mean IoU {row['mean_iou']:.3f} final {row['final_iou']:.3f} "
          f"(circle {row['mean_iou_radial']:.3f} {beat}) area x{row['area_ratio']:.2f} "
          f"MAE {row['arrival_mae']:.0f}h", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    for fire, (p0, dur) in BEST.items():
        sc = json.loads((VAL / "data" / "scenarios" / fire / "scenario.json").read_text())
        n_cells = sc["grid"]["width"] * sc["grid"]["height"]
        windows = sc["wind"][:-1]
        rows.append(run(fire, p0, dur, "ctrl"))
        # E16a: lognormal density with mean exactly 1 (mu = -sigma^2/2).
        rng = np.random.default_rng(7)
        for sigma in (0.3, 0.6, 1.0):
            dens = rng.lognormal(-sigma * sigma / 2.0, sigma, n_cells).astype(np.float32)
            dens_list = [float(x) for x in dens]
            for mult in (1.0, 1.5, 2.0):
                rows.append(run(fire, p0 * mult, dur, f"hetero{sigma:g}_p{mult:g}", density=dens_list))
        # E16b: containment decay exp(-t/tau), evaluated at each window start.
        for tau_days in (5, 10, 20):
            scale = [float(np.exp(-w["hours"] / (24.0 * tau_days))) for w in windows]
            for mult in (1.0, 1.5, 2.0):
                rows.append(run(fire, p0 * mult, dur, f"contain{tau_days}_p{mult:g}", p0_scale=scale))
        (EXP / "exp16_stopping.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
