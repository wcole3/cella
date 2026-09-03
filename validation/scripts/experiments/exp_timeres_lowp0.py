#!/usr/bin/env python
"""E11b: low-p0 extension of E11 for 200 and 400 steps/day.

The E11 floor (p0 = 0.04) already burns everything reachable at those tick
rates, so the fair comparison needs p0 in 0.01-0.03. Same protocol as E11
otherwise: calibration fires only, 1 seed, burn duration 4.8 h in ticks.
Outputs exp11b_timeres_lowp0.json.
"""
import itertools, json, os, shutil, subprocess, sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp11b_timeres"
BIN = REPO / "cella_lib/target/release/examples/wildfire_experiment"
FIRES = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016"]
STEPS_PER_DAY = [200, 400]
P0 = [0.01, 0.015, 0.02, 0.03]
DUR_HOURS = [4.8]
SEEDS = 1


def run(fire, spd, p0, dur_h):
    tmp = EXP / "tmp_timeres_b"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    sc = json.loads((tmp / "scenario.json").read_text())
    sc["steps_per_hour"] = spd / 24.0
    (tmp / "scenario.json").write_text(json.dumps(sc))
    dur = max(1, round(dur_h * spd / 24.0))
    cfg = json.loads((tmp / "config.json").read_text())
    cfg["model"]["wildfire"]["params"]["p0"] = p0
    cfg["model"]["wildfire"]["params"]["burn_duration"] = dur
    (tmp / "config.json").write_text(json.dumps(cfg))
    rep = OUT / f"{fire}_s{spd}_p{p0}_d{dur}.json"
    subprocess.run([str(BIN), str(tmp), str(SEEDS), str(rep)], check=True, capture_output=True)
    r = json.loads(rep.read_text())
    s, rad = r["model"][1:], r["radial"][1:]
    row = {"fire": fire, "steps_per_day": spd, "p0": p0, "dur_ticks": dur, "dur_hours": dur_h,
           "mean_iou": sum(x["iou"] for x in s) / len(s), "final_iou": r["final_iou_model"],
           "mean_iou_radial": sum(x["iou"] for x in rad) / len(rad),
           "area_ratio": s[-1]["sim_burned"] / s[-1]["obs_burned"],
           "arrival_mae": r["model_arrival_mae_hours"]}
    print(f"{fire:14s} spd {spd:3d} p0 {p0:.3g} dur {dur:2d} | mean IoU {row['mean_iou']:.3f} "
          f"final {row['final_iou']:.3f} area x{row['area_ratio']:.2f} MAE {row['arrival_mae']:.0f}h", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    for fire in FIRES:
        for spd, dur_h in itertools.product(STEPS_PER_DAY, DUR_HOURS):
            for p0 in P0:
                # Skip obviously-exploding corners to save time: at >=200/day
                # p0 >= 0.22 burns everything reachable.
                if False:
                    continue
                row = run(fire, spd, p0, dur_h)
                rows.append(row)
                if row["area_ratio"] > 6:
                    break  # larger p0 only burns more
        (EXP / "exp11b_timeres_lowp0.json").write_text(json.dumps(rows, indent=1))
    print("done")


if __name__ == "__main__":
    main()
