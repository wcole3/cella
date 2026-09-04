#!/usr/bin/env python
"""E26: terrain-adjusted wind field (mass-consistent downscaling) vs uniform wind.

Every wind window the harness downscales the uniform wind over the
scenario's elevation with cella_lib::wind_field::mass_consistent (the
two-dimensional WindNinja idea: conserve air flux over terrain, so ridges
speed up and valleys channel) and sets it as a per-cell field. Layer depth
controls how strongly terrain acts (thinner = stronger).

Runs on the E17 working recipe (E1 p0 x2 with the tau-5 decay on ERA5
daily windows, i.e. the E16b setting) and on the plain E1 recipe, with
ERA5 wind x1 and x3 (E9 showed wind is inert at x1; terrain only matters
where the wind has strength). 3 seeds, calibration fires. Output
exp26_windfield.json.
"""
import json, os, shutil, subprocess
from pathlib import Path
import numpy as np

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp26_windfield"
BIN = REPO / "cella_lib/target/release/examples/wildfire_experiment"
BEST = {"Bear_2020": (0.12, 10), "Brattain_2020": (0.22, 10), "Buck_2017": (0.16, 5), "Chimney_2016": (0.30, 5)}
SEEDS = 3


def run(fire, p0, dur, label, env_extra, decay):
    tmp = EXP / "tmp_windfield"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    cfg = json.loads((tmp / "config.json").read_text())
    cfg["model"]["wildfire"]["params"]["p0"] = min(p0, 1.0)
    cfg["model"]["wildfire"]["params"]["burn_duration"] = dur
    (tmp / "config.json").write_text(json.dumps(cfg))
    env = {**os.environ, **env_extra}
    if decay:
        sc = json.loads((tmp / "scenario.json").read_text())
        scale = [float(np.exp(-w["hours"] / 120.0)) for w in sc["wind"][:-1]]
        (tmp / "p0_scale.json").write_text(json.dumps(scale))
        env["EXP_P0_SCALE"] = str(tmp / "p0_scale.json")
    rep = OUT / f"{fire}_{label}.json"
    subprocess.run([str(BIN), str(tmp), str(SEEDS), str(rep)], check=True, capture_output=True, env=env)
    r = json.loads(rep.read_text())
    s, rad = r["model"][1:], r["radial"][1:]
    row = {"fire": fire, "variant": label, "seeds": SEEDS, "p0": p0, "dur": dur,
           "mean_iou": sum(x["iou"] for x in s) / len(s), "final_iou": r["final_iou_model"],
           "mean_iou_radial": sum(x["iou"] for x in rad) / len(rad),
           "area_ratio": s[-1]["sim_burned"] / s[-1]["obs_burned"], "arrival_mae": r["model_arrival_mae_hours"]}
    print(f"{fire:14s} {label:24s} mean IoU {row['mean_iou']:.3f} final {row['final_iou']:.3f} "
          f"(circle {row['mean_iou_radial']:.3f}) area x{row['area_ratio']:.2f} MAE {row['arrival_mae']:.0f}h", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    for fire, (p0, dur) in BEST.items():
        for recipe, mult, decay in (("plain", 1.0, False), ("decay", 2.0, True)):
            for wscale in ("1", "3"):
                base = {"EXP_WIND_SCALE": wscale}
                rows.append(run(fire, p0 * mult, dur, f"{recipe}_w{wscale}_uniform", base, decay))
                for depth in ("150", "300", "600"):
                    rows.append(run(fire, p0 * mult, dur, f"{recipe}_w{wscale}_terrain{depth}",
                                    {**base, "EXP_WIND_FIELD": depth}, decay))
        (EXP / "exp26_windfield.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
