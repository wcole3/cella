#!/usr/bin/env python
"""E23: the fire-line agent done less naively — ramped, breachable, anchor-and-flank.

E18's agent (constant rate from hour 24, perfect Inactive line, heel-first)
was all-or-nothing. Three changes, each from how real incidents work:

  ramp     — resources arrive over days: rate x (1 - e^(-t/tau_r)), tau_r 3 d
  breach   — line is a low-flammability fuel class "Line" (veg_factor 0.1),
             so wind and slope can push fire across it (E18 change 2)
  upwind   — build on the up-wind side of the fire centre first (heel and
             flanks); the head is ranked last (E18 change 3)

Variants combine them at three peak rates (300 / 1000 / 3000 cells per
day = 9 / 30 / 90 km of line per day at full strength) and p0 x1 or x2.
ERA5 daily wind, E1 (p0, dur), 3 seeds. Output exp23_fireline2.json.
Containment calibration (E23b) compares the painted line length against
observed percent-contained once ICS-209 data is loaded.
"""
import json, os, shutil, subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp23_fireline2"
BIN = REPO / "cella_lib/target/release/examples/wildfire_experiment"
BEST = {"Bear_2020": (0.12, 10), "Brattain_2020": (0.22, 10), "Buck_2017": (0.16, 5), "Chimney_2016": (0.30, 5)}
SEEDS = 3


def run(fire, p0, dur, label, env_extra):
    tmp = EXP / "tmp_fireline2"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    cfg = json.loads((tmp / "config.json").read_text())
    pr = cfg["model"]["wildfire"]["params"]
    pr["p0"] = p0
    pr["burn_duration"] = dur
    if env_extra.get("EXP_LINE_TYPE") == "Line":
        pr["fuels"].append({"name": "Line", "veg_factor": 0.1})
    (tmp / "config.json").write_text(json.dumps(cfg))
    rep = OUT / f"{fire}_{label}.json"
    subprocess.run([str(BIN), str(tmp), str(SEEDS), str(rep)], check=True, capture_output=True,
                   env={**os.environ, **env_extra})
    r = json.loads(rep.read_text())
    s, rad = r["model"][1:], r["radial"][1:]
    row = {"fire": fire, "variant": label, "seeds": SEEDS, "p0": p0, "dur": dur, "env": env_extra,
           "mean_iou": sum(x["iou"] for x in s) / len(s), "final_iou": r["final_iou_model"],
           "mean_iou_radial": sum(x["iou"] for x in rad) / len(rad),
           "area_ratio": s[-1]["sim_burned"] / s[-1]["obs_burned"], "arrival_mae": r["model_arrival_mae_hours"],
           "area_curve": [x["sim_burned"] for x in r["model"]], "obs_curve": [x["obs_burned"] for x in r["model"]]}
    print(f"{fire:14s} {label:26s} mean IoU {row['mean_iou']:.3f} final {row['final_iou']:.3f} "
          f"(circle {row['mean_iou_radial']:.3f}) area x{row['area_ratio']:.2f} MAE {row['arrival_mae']:.0f}h", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    full = {"EXP_LINE_RAMP_DAYS": "3", "EXP_LINE_TYPE": "Line", "EXP_LINE_TACTIC": "upwind"}
    for fire, (p0, dur) in BEST.items():
        rows.append(run(fire, p0, dur, "ctrl", {}))
        for rate in (300, 1000, 3000):
            base = {"EXP_LINE_RATE": str(rate)}
            # one change at a time at 1000/day, all three at every rate
            if rate == 1000:
                rows.append(run(fire, p0, dur, f"r{rate}_ramp", {**base, "EXP_LINE_RAMP_DAYS": "3"}))
                rows.append(run(fire, p0, dur, f"r{rate}_breach", {**base, "EXP_LINE_TYPE": "Line"}))
                rows.append(run(fire, p0, dur, f"r{rate}_upwind", {**base, "EXP_LINE_TACTIC": "upwind"}))
            for mult in (1.0, 2.0):
                rows.append(run(fire, p0 * mult, dur, f"r{rate}_all_p{mult:g}", {**base, **full}))
        (EXP / "exp23_fireline2.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
