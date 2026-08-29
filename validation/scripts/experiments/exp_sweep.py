#!/usr/bin/env python
"""E1: coarse p0 x burn_duration scan on the four CALIBRATION fires only.

Single seed per combo (scan-grade). Objective per test plan section 6:
mean IoU over the observation series (t=0 excluded; it is 1.0 by
construction). Circle-null mean computed from the same report for
comparison. Results -> scratchpad exp1_scan.json.
"""

import itertools
import json
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
FIRES = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016"]  # calibration set ONLY
P0S = [0.08, 0.12, 0.16, 0.22, 0.30]
DURS = [2, 5, 10]


def run_one(fire, p0, dur, tmp, out_dir):
    src = VAL / "data" / "scenarios" / fire
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(src, tmp)
    cfg_path = tmp / "config.json"
    cfg = json.loads(cfg_path.read_text())
    cfg["model"]["wildfire"]["params"]["p0"] = p0
    cfg["model"]["wildfire"]["params"]["burn_duration"] = dur
    cfg_path.write_text(json.dumps(cfg))
    report_path = out_dir / f"{fire}_p{p0}_d{dur}.json"
    subprocess.run(
        ["cargo", "run", "--release", "--example", "wildfire_validate",
         "--", str(tmp), "1", str(report_path)],
        cwd=REPO / "cella_lib", check=True, capture_output=True,
    )
    r = json.loads(report_path.read_text())
    series = r["model"][1:]          # skip t=0 (1.0 by construction)
    radial = r["radial"][1:]
    return {
        "fire": fire, "p0": p0, "dur": dur,
        "mean_iou": sum(s["iou"] for s in series) / len(series),
        "final_iou": r["final_iou_model"],
        "mean_iou_radial": sum(s["iou"] for s in radial) / len(radial),
        "final_iou_radial": r["final_iou_radial"],
        "area_ratio": series[-1]["sim_burned"] / series[-1]["obs_burned"],
    }


def main():
    out_dir = EXP / "exp1_reports"
    out_dir.mkdir(parents=True, exist_ok=True)
    tmp = EXP / "tmp_scenario"
    rows = []
    for fire, (p0, dur) in itertools.product(FIRES, itertools.product(P0S, DURS)):
        row = run_one(fire, p0, dur, tmp, out_dir)
        rows.append(row)
        print(f"{fire} p0={p0} dur={dur}: mean IoU {row['mean_iou']:.3f} "
              f"(circle {row['mean_iou_radial']:.3f}), area x{row['area_ratio']:.1f}",
              flush=True)
    shutil.rmtree(tmp, ignore_errors=True)
    (EXP / "exp1_scan.json").write_text(json.dumps(rows, indent=1))
    print("done")


if __name__ == "__main__":
    main()
