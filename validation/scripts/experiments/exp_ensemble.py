#!/usr/bin/env python
"""E8: ensemble burn-probability thresholding (forecast post-processing,
no physics change). Run 5 independent seeds per fire at the E1 operating
point, dump each seed's arrival field, then score the mask "cells burned
in >= q of 5 seeds" for q = 1..5 at each observation time.

If over-burn were a stochastic fringe, high q would cut it. If all seeds
over-burn the same way (deterministic percolation), thresholding won't help
— either answer is informative.
"""

import json
import shutil
import subprocess
import os
from pathlib import Path

import numpy as np

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp4_ensemble"
FIRES = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016"]
NSEEDS = 5

BEST = {}
for r in json.loads((EXP / "exp1_scan.json").read_text()):
    if r["fire"] not in BEST or r["mean_iou"] > BEST[r["fire"]]["mean_iou"]:
        BEST[r["fire"]] = r


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    summary = []
    for fire in FIRES:
        b = BEST[fire]
        tmp = EXP / "tmp_ens"
        if tmp.exists():
            shutil.rmtree(tmp)
        shutil.copytree(VAL / "data" / "scenarios" / fire, tmp)
        cfg = json.loads((tmp / "config.json").read_text())
        cfg["model"]["wildfire"]["params"]["p0"] = b["p0"]
        cfg["model"]["wildfire"]["params"]["burn_duration"] = b["dur"]
        (tmp / "config.json").write_text(json.dumps(cfg))

        arrivals = []
        for s in range(NSEEDS):
            fields_path = OUT / f"{fire}_s{s}_fields.json"
            subprocess.run(
                ["cargo", "run", "--release", "--example", "wildfire_experiment",
                 "--", str(tmp), "1", str(OUT / "tmp_report.json"), str(fields_path)],
                cwd=REPO / "cella_lib", check=True, capture_output=True,
                env={**os.environ, "EXP_SEED_BASE": str(s)},
            )
            f = json.loads(fields_path.read_text())
            arrivals.append(np.array(f["sim_arrival_seed0"]))
            fields_path.unlink()  # keep disk sane
        truth = json.loads((VAL / "data" / "scenarios" / fire / "truth.json").read_text())
        obs_arr = np.array(truth["arrival_hours"])
        times = truth["observed_at"][1:]

        for q in range(1, NSEEDS + 1):
            ious = []
            for t in times:
                votes = sum((a >= 0) & (a <= t) for a in arrivals)
                sim = votes >= q
                obs = (obs_arr >= 0) & (obs_arr <= t)
                inter = (sim & obs).sum()
                union = (sim | obs).sum()
                ious.append(inter / union if union else 1.0)
            mean_iou = float(np.mean(ious))
            summary.append({"fire": fire, "q": q, "mean_iou": mean_iou})
            print(f"{fire:15s} q>={q}/5: mean IoU {mean_iou:.3f} "
                  f"(single-seed scan was {b['mean_iou']:.3f})", flush=True)
    (EXP / "exp4_ensemble.json").write_text(json.dumps(summary, indent=1))
    print("done")


if __name__ == "__main__":
    main()
