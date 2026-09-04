#!/usr/bin/env python
"""E22: a rate-of-spread-driven clock.

E19 showed the model's front speed barely responds to wind (5-10 %) while
real spread rate grows roughly as wind^1.5 (Rothermel). E11 showed that a
constant tick rate cannot fix it. So let the clock follow the weather:
ticks per hour in each hourly window = base x m(U), with

    m(U) = (1 + k U^1.5) / mean_over_run(1 + k U^1.5)

normalised so the run's TOTAL tick count equals the declared 50/day (the
total burn stays comparable; only the timing of ticks moves to windy
hours). U = hourly station wind (m/s), k in {0.05, 0.1, 0.2, 0.4}
(U = 5 m/s: x1.6 / x2.1 / x3.2 / x5.5 before normalisation).

Run on top of the E17 working recipe (p0 x4 E1, hourly station wind x1,
moisture M_x 35 %, decay tau 5 d) and, as a control, without the decay at
E1 p0 to see the pure clock effect. 3 seeds. Output exp22_clock.json.
"""
import json, os, shutil, subprocess, sys
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).resolve().parent))
from exp_station import emc_fosberg, eta_moisture, hourly_windows, BEST, VAL, EXP, BIN  # noqa: E402

OUT = EXP / "exp22_clock"
SEEDS = 3


def run(fire, p0, dur, label, wind, p0_scale, tick_scale):
    tmp = EXP / "tmp_clock"
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
    env = dict(os.environ)
    if p0_scale is not None:
        (tmp / "p0_scale.json").write_text(json.dumps(p0_scale))
        env["EXP_P0_SCALE"] = str(tmp / "p0_scale.json")
    if tick_scale is not None:
        (tmp / "tick_scale.json").write_text(json.dumps(tick_scale))
        env["EXP_TICK_SCALE"] = str(tmp / "tick_scale.json")
    rep = OUT / f"{fire}_{label}.json"
    subprocess.run([str(BIN), str(tmp), str(SEEDS), str(rep)], check=True, capture_output=True, env=env)
    r = json.loads(rep.read_text())
    s, rad = r["model"][1:], r["radial"][1:]
    row = {"fire": fire, "variant": label, "seeds": SEEDS, "p0": p0, "dur": dur,
           "mean_iou": sum(x["iou"] for x in s) / len(s), "final_iou": r["final_iou_model"],
           "mean_iou_radial": sum(x["iou"] for x in rad) / len(rad),
           "area_ratio": s[-1]["sim_burned"] / s[-1]["obs_burned"], "arrival_mae": r["model_arrival_mae_hours"]}
    print(f"{fire:14s} {label:22s} mean IoU {row['mean_iou']:.3f} final {row['final_iou']:.3f} "
          f"(circle {row['mean_iou_radial']:.3f}) area x{row['area_ratio']:.2f} MAE {row['arrival_mae']:.0f}h", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    for fire, (p0, dur) in BEST.items():
        wind, st_rows, _ = hourly_windows(fire, 1.0)
        n = len(wind) - 1
        U = np.array([w["speed_ms"] for w in wind[:n]])
        hours = np.array([w["hours"] for w in wind[:n]])
        eta = [1.0 if (r["rh_pct"] is None or r["temp_c"] is None) else eta_moisture(emc_fosberg(r["rh_pct"], r["temp_c"]), 35.0)
               for r in st_rows[:n]]
        decay = np.exp(-hours / (24.0 * 5.0))
        recipe = [float(e * d) for e, d in zip(eta, decay)]
        rows.append(run(fire, p0 * 4, dur, "recipe_constclock", wind, recipe, None))
        rows.append(run(fire, p0, dur, "plain_constclock", wind, None, None))
        for k in (0.05, 0.1, 0.2, 0.4):
            m = 1.0 + k * U ** 1.5
            m = (m / m.mean()).tolist()
            rows.append(run(fire, p0 * 4, dur, f"recipe_clock_k{k:g}", wind, recipe, m))
            rows.append(run(fire, p0, dur, f"plain_clock_k{k:g}", wind, None, m))
        (EXP / "exp22_clock.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
