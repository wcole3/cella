#!/usr/bin/env python
"""E27: painted retardant — a per-cell multiplier that dries out — instead of a fence.

E18/E23 lines were either perfect (Inactive: strangles) or ignored
(veg_factor 0.1 type). Real retardant is neither: it makes treated fuel
much harder to ignite (PROPAGATOR sets treated cells near-inert; Gimenez
2004: effectiveness depends on coverage and fades as the coating breaks
up, until rain). WildfireModel::set_density paints that multiplier and can
restore it. The line agent (ramp 3 d, upwind tactic, 1000 cells/day)
paints density d in {0.02, 0.05, 0.1}, recovering to 1.0 after
{24, 72, never} hours. ERA5 daily wind, E1 (p0, dur), 3 seeds. Output
exp27_retardant.json.
"""
import json, os, shutil, subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp27_retardant"
BIN = REPO / "cella_lib/target/release/examples/wildfire_experiment"
BEST = {"Bear_2020": (0.12, 10), "Brattain_2020": (0.22, 10), "Buck_2017": (0.16, 5), "Chimney_2016": (0.30, 5)}
SEEDS = 3


def run(fire, p0, dur, label, env_extra):
    tmp = EXP / "tmp_retardant"
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
    row = {"fire": fire, "variant": label, "seeds": SEEDS, "p0": p0, "dur": dur, "env": env_extra,
           "mean_iou": sum(x["iou"] for x in s) / len(s), "final_iou": r["final_iou_model"],
           "mean_iou_radial": sum(x["iou"] for x in rad) / len(rad),
           "area_ratio": s[-1]["sim_burned"] / s[-1]["obs_burned"], "arrival_mae": r["model_arrival_mae_hours"]}
    print(f"{fire:14s} {label:22s} mean IoU {row['mean_iou']:.3f} final {row['final_iou']:.3f} "
          f"(circle {row['mean_iou_radial']:.3f}) area x{row['area_ratio']:.2f} MAE {row['arrival_mae']:.0f}h", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    agent = {"EXP_LINE_RATE": "1000", "EXP_LINE_RAMP_DAYS": "3", "EXP_LINE_TACTIC": "upwind"}
    for fire, (p0, dur) in BEST.items():
        rows.append(run(fire, p0, dur, "ctrl", {}))
        for dens in ("0.02", "0.05", "0.1"):
            for rec in ("24", "72", "inf"):
                env = {**agent, "EXP_LINE_TYPE": f"density:{dens}"}
                if rec != "inf":
                    env["EXP_LINE_RECOVER_H"] = rec
                rows.append(run(fire, p0, dur, f"ret{dens}_rec{rec}", env))
        (EXP / "exp27_retardant.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
