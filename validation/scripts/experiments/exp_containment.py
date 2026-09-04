#!/usr/bin/env python
"""E21: observed containment (ICS-209 daily percent-contained) replaces the fitted decay.

E16/E17 capped the burn with p0 x exp(-t/tau), tau fitted. Here the same
role is played by real incident data: the daily percent-contained the
incident management team reported (ICS-209-PLUS, in
scenario/containment.json). Per window:

    p0_scale(t) = (1 - C(t)/100)^gamma,   C interpolated in time

gamma 1 = spread proportional to the uncontained fraction of perimeter;
gamma 2 = stronger. p0 x{2, 3, 4} because the schedule lowers mean p0.
Two settings: ERA5 daily windows (plain) and the E17 hourly recipe
(station wind x1, moisture M_x 35) with containment instead of the decay.
Compared with the fitted tau 5 decay on the same windows. 3 seeds, E1
(p0, dur), calibration fires + holdout reported separately (no parameter
chosen here; gamma and the multiplier are scanned only on calibration).
"""
import json, os, shutil, subprocess, sys
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).resolve().parent))
from exp_station import emc_fosberg, eta_moisture, hourly_windows, BEST, VAL, EXP, BIN  # noqa: E402

OUT = EXP / "exp21_containment"
SEEDS = 3
HOLD = {"Ferguson_2018": (0.22, 5), "Pier_2017": (0.22, 5)}


def containment(fire, hours):
    c = json.loads((VAL / "data" / "scenarios" / fire / "containment.json").read_text())
    pts = [(r["hours"], r["pct_contained"]) for r in c["rows"] if r["pct_contained"] is not None]
    xs = np.array([p[0] for p in pts]); ys = np.array([p[1] for p in pts])
    # Before the first report: 0 %. After the last: hold. Monotone (cummax) to remove report noise.
    ys = np.maximum.accumulate(ys)
    out = np.interp(hours, xs, ys, left=0.0, right=ys[-1])
    return out / 100.0


def run(fire, p0, dur, label, wind, p0_scale):
    tmp = EXP / "tmp_containment"
    if tmp.exists():
        shutil.rmtree(tmp)
    shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
    cfg = json.loads((tmp / "config.json").read_text())
    cfg["model"]["wildfire"]["params"]["p0"] = min(p0, 1.0)
    cfg["model"]["wildfire"]["params"]["burn_duration"] = dur
    (tmp / "config.json").write_text(json.dumps(cfg))
    sc = json.loads((tmp / "scenario.json").read_text())
    if wind is not None:
        sc["wind"] = wind
        (tmp / "scenario.json").write_text(json.dumps(sc))
    env = dict(os.environ)
    if p0_scale is not None:
        assert len(p0_scale) == len(sc["wind"]) - 1
        (tmp / "p0_scale.json").write_text(json.dumps([float(x) for x in p0_scale]))
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
    fires = dict(BEST) if len(sys.argv) < 2 or sys.argv[1] != "holdout" else HOLD
    for fire, (p0, dur) in fires.items():
        sc = json.loads((VAL / "data" / "scenarios" / fire / "scenario.json").read_text())
        dh = np.array([w["hours"] for w in sc["wind"][:-1]])
        C = containment(fire, dh)
        decay = np.exp(-dh / (24.0 * 5.0))
        rows.append(run(fire, p0 * 2, dur, "daily_decay5_p2", None, decay))
        for gamma in (1.0, 2.0):
            for mult in (2.0, 3.0, 4.0):
                rows.append(run(fire, p0 * mult, dur, f"daily_contain_g{gamma:g}_p{mult:g}", None, (1 - C) ** gamma))
        wind, st_rows, _ = hourly_windows(fire, 1.0)
        n = len(wind) - 1
        hh = np.array([w["hours"] for w in wind[:n]])
        eta = np.array([1.0 if (r["rh_pct"] is None or r["temp_c"] is None) else eta_moisture(emc_fosberg(r["rh_pct"], r["temp_c"]), 35.0) for r in st_rows[:n]])
        Ch = containment(fire, hh)
        rows.append(run(fire, p0 * 4, dur, "hourly_moist_decay5_p4", wind, eta * np.exp(-hh / 120.0)))
        for gamma in (1.0, 2.0):
            for mult in (3.0, 4.0, 6.0):
                rows.append(run(fire, p0 * mult, dur, f"hourly_moist_contain_g{gamma:g}_p{mult:g}", wind, eta * (1 - Ch) ** gamma))
        (EXP / ("exp21_containment_holdout.json" if fires is HOLD else "exp21_containment.json")).write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
