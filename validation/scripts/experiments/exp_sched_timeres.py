#!/usr/bin/env python
"""E13: does a higher tick rate help once p0 varies day to day?

E11 showed a constant-p0 model is rate-invariant (best p0 ~ 1/steps, same
score). E3 showed a daily temperature schedule helps. Hypothesis: with a
schedule, more ticks/day let hot days run fast without cool days
over-burning. Runs each calibration fire at 50 and 200 steps/day, with and
without the temp_vpd schedule, at the E11 best p0 for that rate
(dur 4.8 h). 3 seeds.
"""
import json, os, shutil, subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp13_sched_timeres"
BIN = REPO / "cella_lib/target/release/examples/wildfire_experiment"
# (fire, steps/day) -> p0 from E11/E11b best at dur 4.8 h
P0 = {("Bear_2020", 50): 0.12, ("Bear_2020", 200): 0.03,
      ("Brattain_2020", 50): 0.22, ("Brattain_2020", 200): 0.04,
      ("Buck_2017", 50): 0.10, ("Buck_2017", 200): 0.02,
      ("Chimney_2016", 50): 0.30, ("Chimney_2016", 200): 0.06}
SEEDS = 3


def run(fire, spd, sched):
    tmp = EXP / "tmp_e13"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    sc = json.loads((tmp / "scenario.json").read_text())
    sc["steps_per_hour"] = spd / 24.0
    (tmp / "scenario.json").write_text(json.dumps(sc))
    cfg = json.loads((tmp / "config.json").read_text())
    cfg["model"]["wildfire"]["params"]["p0"] = P0[(fire, spd)]
    cfg["model"]["wildfire"]["params"]["burn_duration"] = round(4.8 * spd / 24)
    (tmp / "config.json").write_text(json.dumps(cfg))
    env = dict(os.environ)
    label = f"s{spd}_{'temp' if sched else 'const'}"
    if sched:
        vals = json.loads((EXP / "schedules" / fire / "temp_vpd.json").read_text())
        (tmp / "p0_scale.json").write_text(json.dumps(vals[: len(sc["wind"]) - 1]))
        env["EXP_P0_SCALE"] = str(tmp / "p0_scale.json")
    rep = OUT / f"{fire}_{label}.json"
    subprocess.run([str(BIN), str(tmp), str(SEEDS), str(rep)], check=True, capture_output=True, env=env)
    r = json.loads(rep.read_text())
    s, rad = r["model"][1:], r["radial"][1:]
    row = {"fire": fire, "steps_per_day": spd, "schedule": "temp_vpd" if sched else None,
           "p0": P0[(fire, spd)], "seeds": SEEDS,
           "mean_iou": sum(x["iou"] for x in s) / len(s), "final_iou": r["final_iou_model"],
           "mean_iou_radial": sum(x["iou"] for x in rad) / len(rad),
           "area_ratio": s[-1]["sim_burned"] / s[-1]["obs_burned"],
           "arrival_mae": r["model_arrival_mae_hours"]}
    print(f"{fire:14s} {label:10s} mean IoU {row['mean_iou']:.3f} final {row['final_iou']:.3f} "
          f"area x{row['area_ratio']:.2f} MAE {row['arrival_mae']:.0f}h (circle {row['mean_iou_radial']:.3f})", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    for fire in ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016"]:
        for spd in (50, 200):
            for sched in (False, True):
                rows.append(run(fire, spd, sched))
    (EXP / "exp13_sched_timeres.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
