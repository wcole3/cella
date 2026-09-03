#!/usr/bin/env python
"""E11: joint (steps/day, p0, burn_duration) scan — a fair version of E6.

The model's front can advance at most one cell per step, so 50 steps/day
x 30 m caps spread at 1.5 km/day. Observed daily advances on the six-fire
pack reach 2-7 km (obs_front_speed.json). This scan raises steps/day and
re-searches p0 and burn_duration instead of assuming p0/4. Burn duration is
scanned in HOURS (converted to ticks) so the front thickness in real time
is comparable across step rates.

Calibration fires only, 1 seed (scan grade). Outputs exp11_timeres.json.
"""
import itertools, json, os, shutil, subprocess, sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp11_timeres"
BIN = REPO / "cella_lib/target/release/examples/wildfire_experiment"
FIRES = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016"]
STEPS_PER_DAY = [50, 100, 200, 400]
P0 = [0.04, 0.06, 0.08, 0.10, 0.12, 0.16, 0.22, 0.30]
DUR_HOURS = [2.4, 4.8]   # = 5 and 10 ticks at 50/day
SEEDS = 1


def run(fire, spd, p0, dur_h):
    tmp = EXP / "tmp_timeres"
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
                if spd >= 200 and p0 > 0.16:
                    continue
                row = run(fire, spd, p0, dur_h)
                rows.append(row)
                if row["area_ratio"] > 6:
                    break  # larger p0 only burns more
        (EXP / "exp11_timeres.json").write_text(json.dumps(rows, indent=1))
    print("done")


if __name__ == "__main__":
    main()
