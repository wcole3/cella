#!/usr/bin/env python
"""E7: spotting (in the pre-registered search space, implemented, currently
off) and E6: time-resolution probe. Single seed, calibration fires only.

Rationale: 50 steps/day x 30 m caps front speed at 1.5 km/day; real wind
runs move 10-30 km/day. Spotting adds fast wind-aligned jumps without
raising the isotropic spread; more steps/day raises the cap directly.
"""

import json
import shutil
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp3_reports"
FIRES = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016"]

BEST = {r["fire"]: r for r in json.loads((EXP / "exp1_scan.json").read_text())}
for r in json.loads((EXP / "exp1_scan.json").read_text()):
    if r["mean_iou"] > BEST[r["fire"]]["mean_iou"]:
        BEST[r["fire"]] = r

SPOT_GRID = [
    ("spot_lo", {"p_spot": 0.001, "median_distance": 5.0, "sigma": 0.5, "angle_jitter_deg": 15.0}),
    ("spot_mid", {"p_spot": 0.005, "median_distance": 10.0, "sigma": 0.5, "angle_jitter_deg": 15.0}),
    ("spot_far", {"p_spot": 0.002, "median_distance": 20.0, "sigma": 0.8, "angle_jitter_deg": 10.0}),
]


def run(fire, label, p0, dur, spotting=None, steps_mult=None):
    tmp = EXP / "tmp_spot"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    cfg = json.loads((tmp / "config.json").read_text())
    params = cfg["model"]["wildfire"]["params"]
    params["p0"] = p0
    params["burn_duration"] = dur
    if spotting:
        params["spotting"] = spotting
    (tmp / "config.json").write_text(json.dumps(cfg))
    if steps_mult:
        sc = json.loads((tmp / "scenario.json").read_text())
        sc["steps_per_hour"] *= steps_mult
        (tmp / "scenario.json").write_text(json.dumps(sc))
    report_path = OUT / f"{fire}_{label}.json"
    subprocess.run(
        ["cargo", "run", "--release", "--example", "wildfire_experiment",
         "--", str(tmp), "1", str(report_path)],
        cwd=REPO / "cella_lib", check=True, capture_output=True,
    )
    r = json.loads(report_path.read_text())
    series = r["model"][1:]
    radial = r["radial"][1:]
    row = {"fire": fire, "variant": label,
           "mean_iou": sum(s["iou"] for s in series) / len(series),
           "mean_iou_radial": sum(s["iou"] for s in radial) / len(radial),
           "area_ratio": series[-1]["sim_burned"] / series[-1]["obs_burned"]}
    print(f"{fire:15s} {label:14s} mean IoU {row['mean_iou']:.3f} "
          f"(circle {row['mean_iou_radial']:.3f}) area x{row['area_ratio']:.1f}", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    for fire in FIRES:
        b = BEST[fire]
        p0, dur = b["p0"], b["dur"]
        print(f"== {fire} at p0={p0} dur={dur}", flush=True)
        for label, spot in SPOT_GRID:
            rows.append(run(fire, label, p0, dur, spotting=spot))
        # E6: 4x time resolution; p0 scaled down to keep expected
        # ignitions-per-hour roughly constant (p ~ rate * dt).
        rows.append(run(fire, "steps_x4", p0 / 4.0, dur * 4, steps_mult=4))
    (EXP / "exp3_spotting.json").write_text(json.dumps(rows, indent=1))
    print("done")


if __name__ == "__main__":
    main()
