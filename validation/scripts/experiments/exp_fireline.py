#!/usr/bin/env python
"""E18: a dynamic fire-line agent instead of a decay.

cella lets any cell be repainted between steps, so containment need not be
a pre-drawn line: after each wind window the harness paints `EXP_LINE_RATE`
cells per day of the model's OWN active edge to Inactive, nearest the
ignition (heel) first, starting after 24 h. No truth is used. Rates
bracket real line production for a large incident (hand crews ~0.3-1 km/day
each, dozers 2-5 km/day; a 30 m cell is 30 m of line): 100 / 300 / 1000
cells/day = 3 / 9 / 30 km of line per day. Also with p0 x1.5 so the early
fire is not starved by the line. ERA5 daily wind, E1 (p0, dur), 3 seeds.
"""
import json, os, shutil, subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp18_fireline"
BIN = REPO / "cella_lib/target/release/examples/wildfire_experiment"
BEST = {"Bear_2020": (0.12, 10), "Brattain_2020": (0.22, 10), "Buck_2017": (0.16, 5), "Chimney_2016": (0.30, 5)}
SEEDS = 3


def run(fire, p0, dur, label, env_extra):
    tmp = EXP / "tmp_fireline"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    cfg = json.loads((tmp / "config.json").read_text())
    cfg["model"]["wildfire"]["params"]["p0"] = p0
    cfg["model"]["wildfire"]["params"]["burn_duration"] = dur
    (tmp / "config.json").write_text(json.dumps(cfg))
    rep = OUT / f"{fire}_{label}.json"
    subprocess.run([str(BIN), str(tmp), str(SEEDS), str(rep)], check=True, capture_output=True,
                   env={**os.environ, **env_extra})
    r = json.loads(rep.read_text())
    s, rad = r["model"][1:], r["radial"][1:]
    row = {"fire": fire, "variant": label, "seeds": SEEDS, "p0": p0, "dur": dur,
           "mean_iou": sum(x["iou"] for x in s) / len(s), "final_iou": r["final_iou_model"],
           "mean_iou_radial": sum(x["iou"] for x in rad) / len(rad),
           "area_ratio": s[-1]["sim_burned"] / s[-1]["obs_burned"], "arrival_mae": r["model_arrival_mae_hours"]}
    print(f"{fire:14s} {label:16s} mean IoU {row['mean_iou']:.3f} final {row['final_iou']:.3f} "
          f"(circle {row['mean_iou_radial']:.3f}) area x{row['area_ratio']:.2f} MAE {row['arrival_mae']:.0f}h", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    for fire, (p0, dur) in BEST.items():
        rows.append(run(fire, p0, dur, "ctrl", {}))
        for rate in (100, 300, 1000):
            rows.append(run(fire, p0, dur, f"line{rate}", {"EXP_LINE_RATE": str(rate)}))
            rows.append(run(fire, p0 * 1.5, dur, f"line{rate}_p1.5", {"EXP_LINE_RATE": str(rate)}))
        (EXP / "exp18_fireline.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
