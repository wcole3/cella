#!/usr/bin/env python
"""E37b (Task 8): the E37 illumination re-run with E30a's recommended
kernel (arrival-time spread, rear-focus wind law) -- the acceptance test
for E30. Identical `wildfire_smc map` settings to E37 (batch 32, 30
generations, 5 days of the scenario's own weather, growth x elongation
axes, no objective, no stopping rule) with SMC_SPREAD=arrival
SMC_WIND_LAW=rear_focus added; the wind x gene range is unchanged
(wind_scale still 0-1.5x the scenario's ERA5 wind). E37's own numbers
(exp37_illuminate.json, exp37_illuminate/<fire>.json) are the
comparison and are not re-run here. Output:
exp30_arrival_illuminate/<fire>.json and exp30_arrival_illuminate.json
(summary rows)."""
import json, os, subprocess, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

OUT = c.EXP / "exp30_arrival_illuminate"


def run(fire):
    OUT.mkdir(parents=True, exist_ok=True)
    rep = OUT / f"{fire}.json"
    env = {**os.environ, "SMC_TAU_OFF": "1", "SMC_SPREAD": "arrival", "SMC_WIND_LAW": "rear_focus",
           "SMC_MAP_DAYS": "5", "SMC_GENERATIONS": "30", "SMC_POP": "32"}
    subprocess.run([str(c.BIN), str(c.VAL / "data" / "scenarios" / fire), "32", "map", str(rep)],
                   check=True, capture_output=True, env=env)
    r = json.loads(rep.read_text())
    a = r["archive"]
    el = [e["descriptor"][1] for e in a["elites"]]
    row = {"fire": fire, "elites": a["stats"]["elites"], "coverage": a["stats"]["coverage"],
           "max_model_elongation": max(el) if el else 1.0,
           "observed": r["observed"], "labels": a["labels"], "ranges": a["ranges"],
           "binary_git": r.get("binary_git", "unknown"), "binary_built_utc": r.get("binary_built_utc", "unknown")}
    obs_el = max(o[2] for o in r["observed"]) if r["observed"] else float("nan")
    print(f"{fire:14s} elites {row['elites']:3d} coverage {row['coverage']:.2f} "
          f"model elongation max {row['max_model_elongation']:.2f} vs observed max {obs_el:.2f}", flush=True)
    return row


if __name__ == "__main__":
    from concurrent.futures import ThreadPoolExecutor
    with ThreadPoolExecutor(max_workers=4) as ex:
        rows = list(ex.map(run, c.FIRES))
    (c.EXP / "exp30_arrival_illuminate.json").write_text(json.dumps(rows, indent=1))
