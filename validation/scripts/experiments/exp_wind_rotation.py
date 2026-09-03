#!/usr/bin/env python
"""E9: wind-direction convention diagnostic.

Rotate the whole wind schedule by 0/90/180/270 degrees, and also run wind
off (speed x0) and wind amplified (x5), at each calibration fire's best
(p0, dur) recipe from E1. If any non-zero rotation scores better than 0,
the model's direction convention (or the converter's) is wrong. If wind
off scores the same as everything else, the kernel is inert at ERA5 speeds.

Calibration fires only, 3 seeds. Uses EXP_WIND_ROT_DEG / EXP_WIND_SCALE
hooks in cella_lib/examples/wildfire_experiment.rs.
"""
import json, os, shutil, subprocess, sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp9_rotation"
BEST = {"Bear_2020": (0.12, 10), "Brattain_2020": (0.22, 10),
        "Buck_2017": (0.16, 5), "Chimney_2016": (0.30, 5)}
SEEDS = int(sys.argv[1]) if len(sys.argv) > 1 else 3


def run(fire, p0, dur, label, env_extra):
    tmp = EXP / "tmp_rot"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    cfg = json.loads((tmp / "config.json").read_text())
    cfg["model"]["wildfire"]["params"]["p0"] = p0
    cfg["model"]["wildfire"]["params"]["burn_duration"] = dur
    (tmp / "config.json").write_text(json.dumps(cfg))
    rep = OUT / f"{fire}_{label}.json"
    subprocess.run([str(REPO / "cella_lib/target/release/examples/wildfire_experiment"),
                    str(tmp), str(SEEDS), str(rep)],
                   check=True, capture_output=True, env={**os.environ, **env_extra})
    r = json.loads(rep.read_text())
    s, rad = r["model"][1:], r["radial"][1:]
    row = {"fire": fire, "variant": label, "seeds": SEEDS,
           "mean_iou": sum(x["iou"] for x in s) / len(s),
           "final_iou": r["final_iou_model"],
           "mean_iou_radial": sum(x["iou"] for x in rad) / len(rad),
           "area_ratio": s[-1]["sim_burned"] / s[-1]["obs_burned"],
           "arrival_mae": r["model_arrival_mae_hours"]}
    print(f"{fire:14s} {label:8s} mean IoU {row['mean_iou']:.3f} final {row['final_iou']:.3f} "
          f"area x{row['area_ratio']:.2f} MAE {row['arrival_mae']:.0f}h", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    for fire, (p0, dur) in BEST.items():
        for rot in (0, 90, 180, 270):
            rows.append(run(fire, p0, dur, f"rot{rot}", {"EXP_WIND_ROT_DEG": str(rot)}))
        rows.append(run(fire, p0, dur, "windoff", {"EXP_WIND_SCALE": "0"}))
        rows.append(run(fire, p0, dur, "windx5", {"EXP_WIND_SCALE": "5"}))
        for rot in (90, 180, 270):
            rows.append(run(fire, p0, dur, f"x5rot{rot}", {"EXP_WIND_SCALE": "5", "EXP_WIND_ROT_DEG": str(rot)}))
    (EXP / "exp9_rotation.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
