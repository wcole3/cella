#!/usr/bin/env python
"""E24 / E25: Monte Carlo burn probability (open) and the generational,
assimilating ensemble (assim) on all six fires.

Runs cella_lib/examples/wildfire_smc with M = 32 members from one broad
prior (no per-fire tuning; the holdout fires are legitimately included
because nothing is chosen on them):

  open              plain Monte Carlo, scored as a probability map (E24)
  assim_b10_s0.2    particle filter, beta 10, mutation sigma 0.2 (E25)
  assim_b30_s0.2    sharper selection
  assim_b10_s0.05   weaker mutation (closer to plain resampling)

Scores at each observation time are one-window-ahead forecasts. Three
processes in parallel. Output exp24_smc.json (one row per fire x config).
"""
import json, os, subprocess, sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
OUT = EXP / "exp24_smc"
BIN = REPO / "cella_lib/target/release/examples/wildfire_smc"
FIRES = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016", "Ferguson_2018", "Pier_2017"]
M = int(sys.argv[1]) if len(sys.argv) > 1 else 32
CONFIGS = {"open": ("open", {}), "assim_b10_s0.2": ("assim", {"SMC_BETA": "10", "SMC_SIGMA": "0.2"}),
           "assim_b30_s0.2": ("assim", {"SMC_BETA": "30", "SMC_SIGMA": "0.2"}),
           "assim_b10_s0.05": ("assim", {"SMC_BETA": "10", "SMC_SIGMA": "0.05"})}


def run(job):
    fire, label = job
    mode, env = CONFIGS[label]
    rep = OUT / f"{fire}_{label}.json"
    subprocess.run([str(BIN), str(VAL / "data" / "scenarios" / fire), str(M), mode, str(rep)],
                   check=True, capture_output=True, env={**os.environ, **env})
    r = json.loads(rep.read_text())
    row = {"fire": fire, "config": label, "members": M, "holdout": fire in ("Ferguson_2018", "Pier_2017"),
           "mean_consensus_iou": r["mean_consensus_iou"], "mean_best_threshold_iou": r["mean_best_threshold_iou"],
           "mean_member_iou": r["mean_member_iou"], "mean_radial_iou": r["mean_radial_iou"],
           "final_consensus_iou": r["final_consensus_iou"], "final_best_threshold_iou": r["final_best_threshold_iou"],
           "final_radial_iou": r["final_radial_iou"], "mean_brier_ensemble": r["mean_brier_ensemble"],
           "mean_brier_radial": r["mean_brier_radial"],
           "final_p0_mean": r["scores"][-1]["p0_mean"], "final_tau_mean": r["scores"][-1]["tau_mean"],
           "final_dur_mean": r["scores"][-1]["dur_mean"], "final_wind_scale_mean": r["scores"][-1]["wind_scale_mean"]}
    print(f"{fire:14s} {label:16s} consensus {row['mean_consensus_iou']:.3f} bestthr {row['mean_best_threshold_iou']:.3f} "
          f"member {row['mean_member_iou']:.3f} (circle {row['mean_radial_iou']:.3f}) final {row['final_consensus_iou']:.3f} "
          f"Brier {row['mean_brier_ensemble']:.4f} vs {row['mean_brier_radial']:.4f} | p0 {row['final_p0_mean']:.2f} tau {row['final_tau_mean']:.0f}", flush=True)
    return row


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    jobs = [(f, c) for c in CONFIGS for f in FIRES]
    with ThreadPoolExecutor(max_workers=3) as ex:
        rows = list(ex.map(run, jobs))
    (EXP / "exp24_smc.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
