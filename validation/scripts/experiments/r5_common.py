#!/usr/bin/env python
"""Shared runner for the Round 5 method experiments (E32–E37).

Every run is `cella_lib/examples/wildfire_smc` in `assim` mode with the
recommended operators (beta 10, sigma 0.2, immigrants 0.2) and the E28
containment-only stopping rule (SMC_CONTAIN=1, SMC_TAU_OFF=1), unless a
job overrides an env knob. `run()` returns one row per fire x config with
the same fields as exp_smc.py plus `members` and `seed`.
"""
import json, os, subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
EXP = VAL / "results" / "experiments"
BIN = REPO / "cella_lib/target/release/examples/wildfire_smc"
FIRES = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016", "Ferguson_2018", "Pier_2017"]
HOLDOUT = ("Ferguson_2018", "Pier_2017")
BASE_ENV = {"SMC_BETA": "10", "SMC_SIGMA": "0.2", "SMC_IMMIGRANTS": "0.2", "SMC_CONTAIN": "1", "SMC_TAU_OFF": "1"}


def run(out_dir, fire, label, env, members=32, mode="assim", argv_prefix=()):
    """One wildfire_smc run; the report lands in out_dir/<fire>_<label>.json.

    `argv_prefix` (default empty) is prepended to the argv verbatim --
    e.g. `["nice", "-n", "10"]` for a caller that needs the child niced
    (r7_common.run(), for the shared-machine rule). No Round 5/6 script
    passes it, so their behaviour is exactly what it was before this
    parameter existed.
    """
    out_dir.mkdir(parents=True, exist_ok=True)
    rep = out_dir / f"{fire}_{label}.json"
    full_env = {**os.environ, **BASE_ENV, **env}
    argv = [*argv_prefix, str(BIN), str(VAL / "data" / "scenarios" / fire), str(members), mode, str(rep)]
    subprocess.run(argv, check=True, capture_output=True, env=full_env)
    r = json.loads(rep.read_text())
    last = r["scores"][-1]
    row = {"fire": fire, "config": label, "members": members, "seed": int(full_env.get("SMC_SEED", "0")),
           "holdout": fire in HOLDOUT,
           # Which binary produced this report; older reports predate these
           # fields, so fall back to "unknown" rather than raising.
           "binary_git": r.get("binary_git", "unknown"), "binary_built_utc": r.get("binary_built_utc", "unknown"),
           "mean_consensus_iou": r["mean_consensus_iou"], "mean_best_threshold_iou": r["mean_best_threshold_iou"],
           "mean_member_iou": r["mean_member_iou"], "mean_radial_iou": r["mean_radial_iou"],
           "final_consensus_iou": r["final_consensus_iou"], "final_radial_iou": r["final_radial_iou"],
           "mean_brier_ensemble": r["mean_brier_ensemble"], "mean_brier_radial": r["mean_brier_radial"],
           # Lagged nulls (E40 controller finding): "yesterday's mask, as
           # is, is today's forecast" / "...grown to today's true area".
           # Absent in reports from before this field existed.
           "mean_lagged_persistence_iou": r.get("mean_lagged_persistence_iou"),
           "mean_brier_lagged_persistence": r.get("mean_brier_lagged_persistence"),
           "mean_lagged_circle_iou": r.get("mean_lagged_circle_iou"),
           "mean_brier_lagged_circle": r.get("mean_brier_lagged_circle"),
           "mean_ess": sum(s["ess"] for s in r["scores"]) / len(r["scores"]),
           "final_p0_mean": last["p0_mean"], "final_dur_mean": last["dur_mean"],
           "final_wind_scale_mean": last["wind_scale_mean"], "final_contained": last["contained_fraction"],
           "per_day_consensus": [s["consensus_iou"] for s in r["scores"]],
           "per_day_brier": [s["brier_ensemble"] for s in r["scores"]],
           "per_day_ess": [s["ess"] for s in r["scores"]]}
    print(f"{fire:14s} {label:22s} M={members:3d} consensus {row['mean_consensus_iou']:.3f} "
          f"member {row['mean_member_iou']:.3f} (circle {row['mean_radial_iou']:.3f}) "
          f"Brier {row['mean_brier_ensemble']:.4f} vs {row['mean_brier_radial']:.4f} ESS {row['mean_ess']:.1f} "
          f"| p0 {row['final_p0_mean']:.2f} wind {row['final_wind_scale_mean']:.2f}", flush=True)
    return row


def run_all(jobs, out_json, workers=3):
    """jobs: list of (fire, label, env, members). Writes rows to out_json."""
    from concurrent.futures import ThreadPoolExecutor
    out_dir = EXP / out_json.replace(".json", "")
    with ThreadPoolExecutor(max_workers=workers) as ex:
        rows = list(ex.map(lambda j: run(out_dir, j[0], j[1], j[2], j[3]), jobs))
    (EXP / out_json).write_text(json.dumps(rows, indent=1))
    return rows
