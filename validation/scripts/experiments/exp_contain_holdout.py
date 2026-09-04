#!/usr/bin/env python
"""E16c: the containment decay as a GLOBAL recipe, reported on all six fires.

Calibration fires picked (tau = 5 d, p0 x2) in E16b using per-fire E1
recipes. The honest headline is one setting for every fire, so this runs
the Round-1 global recipe (p0 0.22, dur 5) with and without the decay on
the four calibration fires AND the two holdout fires (Ferguson, Pier),
which no parameter was chosen on. 3 seeds. Output exp16c_contain_global.json.
"""
import json, os, shutil, subprocess
from pathlib import Path
import numpy as np

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp16c_contain_global"
BIN = REPO / "cella_lib/target/release/examples/wildfire_experiment"
FIRES = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016", "Ferguson_2018", "Pier_2017"]
SEEDS = 3
VARIANTS = {"global_ctrl": (0.22, None), "global_contain5_x2": (0.44, 5.0), "global_contain10_x1.5": (0.33, 10.0)}


def run(fire, label, p0, tau):
    tmp = EXP / "tmp_contain"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    cfg = json.loads((tmp / "config.json").read_text())
    cfg["model"]["wildfire"]["params"]["p0"] = p0
    cfg["model"]["wildfire"]["params"]["burn_duration"] = 5
    (tmp / "config.json").write_text(json.dumps(cfg))
    env = dict(os.environ)
    if tau is not None:
        sc = json.loads((tmp / "scenario.json").read_text())
        scale = [float(np.exp(-w["hours"] / (24.0 * tau))) for w in sc["wind"][:-1]]
        (tmp / "p0_scale.json").write_text(json.dumps(scale))
        env["EXP_P0_SCALE"] = str(tmp / "p0_scale.json")
    rep = OUT / f"{fire}_{label}.json"
    subprocess.run([str(BIN), str(tmp), str(SEEDS), str(rep)], check=True, capture_output=True, env=env)
    r = json.loads(rep.read_text())
    s, rad = r["model"][1:], r["radial"][1:]
    row = {"fire": fire, "variant": label, "seeds": SEEDS, "p0": p0, "tau_days": tau,
           "holdout": fire in ("Ferguson_2018", "Pier_2017"),
           "mean_iou": sum(x["iou"] for x in s) / len(s), "final_iou": r["final_iou_model"],
           "mean_iou_radial": sum(x["iou"] for x in rad) / len(rad),
           "area_ratio": s[-1]["sim_burned"] / s[-1]["obs_burned"], "arrival_mae": r["model_arrival_mae_hours"]}
    print(f"{fire:14s} {label:22s} mean IoU {row['mean_iou']:.3f} final {row['final_iou']:.3f} "
          f"(circle {row['mean_iou_radial']:.3f}) area x{row['area_ratio']:.2f} MAE {row['arrival_mae']:.0f}h", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = [run(f, k, p0, tau) for f in FIRES for k, (p0, tau) in VARIANTS.items()]
    (EXP / "exp16c_contain_global.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
