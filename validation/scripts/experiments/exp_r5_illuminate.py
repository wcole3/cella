#!/usr/bin/env python
"""E37: illuminate the fire model. `wildfire_smc map`: MAP-Elites over the
spread genes (p0, burn_duration, wind_scale; no stopping rule) with growth
x elongation as behaviour axes and no objective, 5 days of each fire's
weather, batch 32, 30 generations. The report is the archive next to the
observed fire's growth and elongation on the same days. Output:
exp37_illuminate/<fire>.json and exp37_illuminate.json (summary rows)."""
import json, os, subprocess, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

OUT = c.EXP / "exp37_illuminate"

def run(fire):
    OUT.mkdir(parents=True, exist_ok=True)
    rep = OUT / f"{fire}.json"
    env = {**os.environ, "SMC_TAU_OFF": "1", "SMC_MAP_DAYS": "5", "SMC_GENERATIONS": "30", "SMC_POP": "32"}
    subprocess.run([str(c.BIN), str(c.VAL / "data" / "scenarios" / fire), "32", "map", str(rep)],
                   check=True, capture_output=True, env=env)
    r = json.loads(rep.read_text())
    a = r["archive"]
    el = [e["descriptor"][1] for e in a["elites"]]
    row = {"fire": fire, "elites": a["stats"]["elites"], "coverage": a["stats"]["coverage"],
           "max_model_elongation": max(el) if el else 1.0,
           "observed": r["observed"], "labels": a["labels"], "ranges": a["ranges"]}
    obs_el = max(o[2] for o in r["observed"]) if r["observed"] else float("nan")
    print(f"{fire:14s} elites {row['elites']:3d} coverage {row['coverage']:.2f} "
          f"model elongation max {row['max_model_elongation']:.2f} vs observed max {obs_el:.2f}", flush=True)
    return row

if __name__ == "__main__":
    from concurrent.futures import ThreadPoolExecutor
    with ThreadPoolExecutor(max_workers=3) as ex:
        rows = list(ex.map(run, c.FIRES))
    (c.EXP / "exp37_illuminate.json").write_text(json.dumps(rows, indent=1))
