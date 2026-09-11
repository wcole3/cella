#!/usr/bin/env python
"""E43: does ember spotting extend the reachable shape region (E37's
"wedge")? A thin variant of `exp_r5_illuminate.py`: same `wildfire_smc
map` MAP-Elites run (batch 32, 30 generations, 5 days, growth x
elongation axes), but with `SMC_SPOT=1` so the genes also include
`model.spotting.p_spot` and `model.spotting.median_distance` and the
config's wildfire model has spotting switched on (see `enable_spotting`
in `wildfire_smc.rs`). E37's own numbers (`exp37_illuminate.json`,
`exp37_illuminate/<fire>.json`) are the comparison; they are not
re-run here. Output: exp43_spot_illuminate/<fire>.json and
exp43_spot_illuminate.json (summary rows)."""
import json, os, subprocess, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

OUT = c.EXP / "exp43_spot_illuminate"

def run(fire):
    OUT.mkdir(parents=True, exist_ok=True)
    rep = OUT / f"{fire}.json"
    env = {**os.environ, "SMC_TAU_OFF": "1", "SMC_SPOT": "1",
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
    (c.EXP / "exp43_spot_illuminate.json").write_text(json.dumps(rows, indent=1))
