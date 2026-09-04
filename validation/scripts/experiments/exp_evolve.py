#!/usr/bin/env python
"""E20: evolutionary per-fire calibration, then a transfer test.

Differential evolution (scipy) over the TEST_PLAN v1.3 search space, one
fire at a time, 1 seed per evaluation, objective = -mean IoU. The point is
not the per-fire score (we are optimising against the answer) but the
LEARNING: which parameters land in the same place on every fire, and does
the median recipe transfer to the holdout? Winners are re-run with 3 seeds.

Genome: [p0, dur, log10(tau_days), wind_scale, M_x, log2(grass/timber)].
Inputs: hourly station wind (x wind_scale), hourly moisture damping at M_x
(>= 150 -> off), containment decay exp(-t/tau) (tau >= 150 -> off).

Usage: exp_evolve.py [maxiter] [popsize] [fire ...]
"""
import json, os, shutil, subprocess, sys, tempfile
from pathlib import Path
import numpy as np
from scipy.optimize import differential_evolution
sys.path.insert(0, str(Path(__file__).resolve().parent))
from exp_station import emc_fosberg, eta_moisture, hourly_windows, VAL, EXP, BIN  # noqa: E402

OUT = EXP / "exp20_evolve"
CAL = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016"]
HOLD = ["Ferguson_2018", "Pier_2017"]
BOUNDS = [(0.03, 0.6), (2, 20), (np.log10(2), np.log10(200)), (0.0, 4.0), (10.0, 200.0), (np.log2(0.3), np.log2(4.0))]
NAMES = ["p0", "dur", "log10_tau", "wind_scale", "Mx", "log2_grass_timber"]


def evaluate(genome, fire, seeds=1, label=None):
    p0, dur, ltau, wscale, mx, lgt = genome
    dur = int(round(dur)); tau = 10 ** ltau; gt = 2 ** lgt
    wind, st_rows, _ = hourly_windows(fire, wscale)
    n = len(wind) - 1
    scale = []
    for r in st_rows[:n]:
        e = 1.0
        if mx < 150 and r["rh_pct"] is not None and r["temp_c"] is not None:
            e = eta_moisture(emc_fosberg(r["rh_pct"], r["temp_c"]), mx)
        d = 1.0 if tau >= 150 else float(np.exp(-r["hours"] / (24.0 * tau)))
        scale.append(e * d)
    tmp = Path(tempfile.mkdtemp(prefix="evo_", dir=EXP))
    try:
        shutil.copytree(VAL / "data" / "scenarios" / fire, tmp, dirs_exist_ok=True)
        cfg = json.loads((tmp / "config.json").read_text())
        pr = cfg["model"]["wildfire"]["params"]
        pr["p0"] = float(min(p0, 1.0)); pr["burn_duration"] = dur
        # veg ratio: scale grass-family classes up and timber-family down by sqrt(gt) each
        for f in pr["fuels"]:
            if f["name"] in ("Grass", "GrassShrub"):
                f["veg_factor"] = f["veg_factor"] * float(np.sqrt(gt))
            elif f["name"] in ("TimberUnder", "TimberLitter"):
                f["veg_factor"] = f["veg_factor"] / float(np.sqrt(gt))
        (tmp / "config.json").write_text(json.dumps(cfg))
        sc = json.loads((tmp / "scenario.json").read_text()); sc["wind"] = wind
        (tmp / "scenario.json").write_text(json.dumps(sc))
        (tmp / "p0_scale.json").write_text(json.dumps(scale))
        rep = tmp / "report.json"
        env = dict(os.environ, EXP_P0_SCALE=str(tmp / "p0_scale.json"))
        subprocess.run([str(BIN), str(tmp), str(seeds), str(rep)], check=True, capture_output=True, env=env)
        r = json.loads(rep.read_text())
        s = r["model"][1:]
        mean_iou = sum(x["iou"] for x in s) / len(s)
        if label:
            shutil.copy(rep, OUT / f"{fire}_{label}.json")
        return mean_iou, {"final_iou": r["final_iou_model"], "area_ratio": s[-1]["sim_burned"] / s[-1]["obs_burned"],
                          "circle": sum(x["iou"] for x in r["radial"][1:]) / len(s), "arrival_mae": r["model_arrival_mae_hours"]}
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def objective(genome, fire):
    return -evaluate(genome, fire)[0]


def decode(g):
    return {"p0": round(float(g[0]), 4), "dur": int(round(g[1])), "tau_days": round(float(10 ** g[2]), 1),
            "wind_scale": round(float(g[3]), 2), "Mx": round(float(g[4]), 1), "grass_timber": round(float(2 ** g[5]), 2)}


def main():
    maxiter = int(sys.argv[1]) if len(sys.argv) > 1 else 12
    popsize = int(sys.argv[2]) if len(sys.argv) > 2 else 6
    fires = sys.argv[3:] or CAL
    OUT.mkdir(parents=True, exist_ok=True)
    results = {}
    for fire in fires:
        evals = []
        def cb(xk, convergence=0.0, fire=fire, evals=evals):
            evals.append(decode(xk)); print(f"  {fire} gen {len(evals)} best {decode(xk)}", flush=True)
        res = differential_evolution(objective, BOUNDS, args=(fire,), maxiter=maxiter, popsize=popsize,
                                     seed=1, workers=8, updating="deferred", polish=False, tol=0, callback=cb)
        best = decode(res.x)
        iou3, extra = evaluate(res.x, fire, seeds=3, label="best3")
        results[fire] = {"best": best, "search_mean_iou_1seed": -res.fun, "verified_mean_iou_3seed": iou3,
                         "nfev": res.nfev, **extra, "generations": evals}
        print(f"{fire:14s} best {best} search {-res.fun:.3f} verified(3) {iou3:.3f} circle {extra['circle']:.3f} "
              f"area x{extra['area_ratio']:.2f}", flush=True)
        (EXP / "exp20_evolve.json").write_text(json.dumps(results, indent=1))
    # Transfer: median genome over calibration fires -> all six, 3 seeds.
    if set(fires) >= set(CAL):
        genomes = np.array([[results[f]["best"]["p0"], results[f]["best"]["dur"], np.log10(results[f]["best"]["tau_days"]),
                             results[f]["best"]["wind_scale"], results[f]["best"]["Mx"], np.log2(results[f]["best"]["grass_timber"])]
                            for f in CAL])
        med = np.median(genomes, axis=0)
        results["transfer_recipe"] = decode(med)
        results["transfer"] = {}
        for f in CAL + HOLD:
            iou, extra = evaluate(med, f, seeds=3, label="transfer3")
            results["transfer"][f] = {"mean_iou": iou, **extra, "holdout": f in HOLD}
            print(f"transfer {f:14s} mean IoU {iou:.3f} (circle {extra['circle']:.3f}) area x{extra['area_ratio']:.2f}", flush=True)
        (EXP / "exp20_evolve.json").write_text(json.dumps(results, indent=1))


if __name__ == "__main__":
    main()
