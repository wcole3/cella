#!/usr/bin/env python
"""Sweep the p0 knob on one fire to map the knife edge for the figures.

For each p0 value this copies the fire's scenario into a temp directory,
rewrites config.json with that p0, runs the harness once (single seed —
this maps a qualitative cliff, not a reported score), and records the
final simulated burned area. Results go to validation/results/p0_sweep.json,
which make_figures.py turns into the knife-edge chart.

Run (the harness must be run from cella_lib/, its own build root):
    validation/.venv/bin/python validation/scripts/p0_sweep.py
"""

import json
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parent
FIRE = "Bear_2020"
P0_VALUES = [0.08, 0.10, 0.12, 0.15, 0.20, 0.30, 0.45, 0.58]


def main():
    src = ROOT / "data" / "scenarios" / FIRE
    tmp = ROOT / "results" / "tmp_sweep"
    out_dir = ROOT / "results" / "sweep_reports"
    out_dir.mkdir(parents=True, exist_ok=True)

    runs = []
    observed_final = None
    for p0 in P0_VALUES:
        if tmp.exists():
            shutil.rmtree(tmp)
        shutil.copytree(src, tmp)
        cfg_path = tmp / "config.json"
        cfg = json.loads(cfg_path.read_text())
        cfg["model"]["wildfire"]["params"]["p0"] = p0
        cfg_path.write_text(json.dumps(cfg))

        report_path = out_dir / f"{FIRE}_p0_{p0}.json"
        subprocess.run(
            ["cargo", "run", "--release", "--example", "wildfire_validate",
             "--", str(tmp), "1", str(report_path)],
            cwd=REPO / "cella_lib", check=True,
        )
        report = json.loads(report_path.read_text())
        final = report["model"][-1]
        observed_final = final["obs_burned"]
        runs.append({"p0": p0,
                     "final_sim_burned_cells": final["sim_burned"],
                     "final_iou": final["iou"]})
        print(f"p0={p0}: burned {final['sim_burned']:.0f} cells, "
              f"IoU {final['iou']:.3f}")

    shutil.rmtree(tmp)
    out = {"fire": FIRE, "seeds_per_point": 1,
           "observed_final_cells": observed_final, "runs": runs}
    (ROOT / "results" / "p0_sweep.json").write_text(json.dumps(out, indent=2))
    print("sweep written to validation/results/p0_sweep.json")


if __name__ == "__main__":
    main()
