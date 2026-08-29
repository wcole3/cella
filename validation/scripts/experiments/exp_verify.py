#!/usr/bin/env python
"""Final verification: 3 seeds on each calibration fire's best recipe from
the single-seed scans, plus one GLOBAL recipe (one setting for all fires,
the honest headline mode). Calibration fires only — holdout untouched.

Best per-fire recipes (from exp1/exp2b):
  Bear:     p0 0.12 dur 10 + combo_k1.0 schedule
  Brattain: p0 0.22 dur 10 + combo_k1.0 schedule
  Buck:     p0 0.16 dur 5  + temp_vpd schedule
  Chimney:  p0 0.30 dur 5  + temp_vpd schedule
Global recipe: p0 0.22 dur 5 + temp_vpd (temp never hurt any fire).
"""

import json
import shutil
import subprocess
import os
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp5_verify"

RECIPES = {
    "Bear_2020": (0.12, 10, "combo_k1.0.json"),
    "Brattain_2020": (0.22, 10, "combo_k1.0.json"),
    "Buck_2017": (0.16, 5, "temp_vpd.json"),
    "Chimney_2016": (0.30, 5, "temp_vpd.json"),
}
GLOBAL = (0.22, 5, "temp_vpd.json")


def run(fire, p0, dur, sched, label, seeds=3):
    tmp = EXP / "tmp_verify"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    cfg = json.loads((tmp / "config.json").read_text())
    cfg["model"]["wildfire"]["params"]["p0"] = p0
    cfg["model"]["wildfire"]["params"]["burn_duration"] = dur
    (tmp / "config.json").write_text(json.dumps(cfg))
    sc = json.loads((tmp / "scenario.json").read_text())
    vals = json.loads((EXP / "schedules" / fire / sched).read_text())
    (tmp / "p0_scale.json").write_text(json.dumps(vals[: len(sc["wind"]) - 1]))

    report_path = OUT / f"{fire}_{label}.json"
    subprocess.run(
        ["cargo", "run", "--release", "--example", "wildfire_experiment",
         "--", str(tmp), str(seeds), str(report_path)],
        cwd=REPO / "cella_lib", check=True, capture_output=True,
        env={**os.environ, "EXP_P0_SCALE": str(tmp / "p0_scale.json")},
    )
    r = json.loads(report_path.read_text())
    series = r["model"][1:]
    radial = r["radial"][1:]
    row = {
        "fire": fire, "label": label, "p0": p0, "dur": dur, "sched": sched,
        "seeds": seeds,
        "mean_iou": sum(s["iou"] for s in series) / len(series),
        "final_iou": r["final_iou_model"],
        "mean_iou_radial": sum(s["iou"] for s in radial) / len(radial),
        "final_iou_radial": r["final_iou_radial"],
        "area_ratio": series[-1]["sim_burned"] / series[-1]["obs_burned"],
        "arrival_mae": r["model_arrival_mae_hours"],
    }
    beat = "BEATS circle" if row["mean_iou"] > row["mean_iou_radial"] else "loses"
    print(f"{fire:15s} {label:9s} mean IoU {row['mean_iou']:.3f} vs circle "
          f"{row['mean_iou_radial']:.3f} [{beat}] area x{row['area_ratio']:.1f} "
          f"MAE {row['arrival_mae']:.0f}h", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    for fire, (p0, dur, sched) in RECIPES.items():
        rows.append(run(fire, p0, dur, sched, "perfire"))
    for fire in RECIPES:
        p0, dur, sched = GLOBAL
        rows.append(run(fire, p0, dur, sched, "global"))
    (EXP / "exp5_verify.json").write_text(json.dumps(rows, indent=1))
    print("done")


if __name__ == "__main__":
    main()
