#!/usr/bin/env python
"""E17: periodic AND monotone — hourly fuel-moisture damping x containment decay.

E15 (moisture, periodic) could not cap the burn; E16b (decay, monotone)
could but has no physics for the day/night rhythm. Both effects are real,
so test them together: per hourly window p0_scale = eta_moisture(RH, T;
M_x = 35 %) x exp(-t / tau). Hourly station wind x1 (E14) throughout.
Because both factors shrink p0, the multiplier is scanned wider. Controls:
decay alone on the same hourly windows, and moisture alone (from E15).
3 seeds, E1 (p0, dur), calibration fires. Output exp17_combined.json.
"""
import json, os, shutil, subprocess, sys
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).resolve().parent))
from exp_station import emc_fosberg, eta_moisture, hourly_windows, BEST, VAL, EXP, BIN  # noqa: E402

OUT = EXP / "exp17_combined"
SEEDS = 3


def run(fire, p0, dur, label, wind, p0_scale):
    tmp = EXP / "tmp_combined"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    cfg = json.loads((tmp / "config.json").read_text())
    cfg["model"]["wildfire"]["params"]["p0"] = min(p0, 1.0)
    cfg["model"]["wildfire"]["params"]["burn_duration"] = dur
    (tmp / "config.json").write_text(json.dumps(cfg))
    sc = json.loads((tmp / "scenario.json").read_text())
    sc["wind"] = wind
    (tmp / "scenario.json").write_text(json.dumps(sc))
    assert len(p0_scale) == len(wind) - 1
    (tmp / "p0_scale.json").write_text(json.dumps(p0_scale))
    env = dict(os.environ, EXP_P0_SCALE=str(tmp / "p0_scale.json"))
    rep = OUT / f"{fire}_{label}.json"
    subprocess.run([str(BIN), str(tmp), str(SEEDS), str(rep)], check=True, capture_output=True, env=env)
    r = json.loads(rep.read_text())
    s, rad = r["model"][1:], r["radial"][1:]
    row = {"fire": fire, "variant": label, "seeds": SEEDS, "p0": p0, "dur": dur,
           "mean_iou": sum(x["iou"] for x in s) / len(s), "final_iou": r["final_iou_model"],
           "mean_iou_radial": sum(x["iou"] for x in rad) / len(rad),
           "area_ratio": s[-1]["sim_burned"] / s[-1]["obs_burned"], "arrival_mae": r["model_arrival_mae_hours"]}
    print(f"{fire:14s} {label:20s} mean IoU {row['mean_iou']:.3f} final {row['final_iou']:.3f} "
          f"(circle {row['mean_iou_radial']:.3f}) area x{row['area_ratio']:.2f} MAE {row['arrival_mae']:.0f}h", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    for fire, (p0, dur) in BEST.items():
        wind, st_rows, _ = hourly_windows(fire, 1.0)
        n = len(wind) - 1
        eta = [1.0 if (r["rh_pct"] is None or r["temp_c"] is None) else eta_moisture(emc_fosberg(r["rh_pct"], r["temp_c"]), 35.0)
               for r in st_rows[:n]]
        hours = [w["hours"] for w in wind[:n]]
        for tau in (5.0, 10.0):
            decay = [float(np.exp(-h / (24.0 * tau))) for h in hours]
            for mult in (2.0, 3.0):
                rows.append(run(fire, p0 * mult, dur, f"decay{tau:g}_p{mult:g}", wind, decay))
            for mult in (2.0, 3.0, 4.0):
                rows.append(run(fire, p0 * mult, dur, f"moist35xdecay{tau:g}_p{mult:g}", wind, [e * d for e, d in zip(eta, decay)]))
        (EXP / "exp17_combined.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
