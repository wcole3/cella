#!/usr/bin/env python
"""E43 fix round 1: post-hoc replay diagnostic on the spotting illumination
archives (exp43_spot_illuminate). `elongation()` is a second-moment measure
over every tracked cell with no connectivity distinction, so a round core
plus a few spot-fire embers landed downwind can read as "elongated" the
same as a genuinely stretched single blob. This re-evaluates each fire's
top 5 elites by elongation (among those at/above the observed day-5
growth) for 3 fresh seeds each (`wildfire_smc replay`,
SMC_MAP_REPLAY=<archive>) and reports connected-component stats: total
burned cells, largest-component fraction, component count, and the
largest component's own elongation next to the whole-set elongation.
Output: exp43_replay/<fire>.json (wildfire_smc's own report) and
exp43_replay.json (flattened summary rows, one per fire x elite x seed)."""
import json, os, subprocess, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

OUT = c.EXP / "exp43_replay"
SRC = c.EXP / "exp43_spot_illuminate"


def run(fire):
    OUT.mkdir(parents=True, exist_ok=True)
    src = SRC / f"{fire}.json"
    rep = OUT / f"{fire}.json"
    env = {**os.environ, "SMC_SPOT": "1", "SMC_TAU_OFF": "1", "SMC_MAP_DAYS": "5",
           "SMC_MAP_REPLAY": str(src), "SMC_REPLAY_TOP": "5", "SMC_REPLAY_SEEDS": "3"}
    subprocess.run([str(c.BIN), str(c.VAL / "data" / "scenarios" / fire), "32", "replay", str(rep)],
                   check=True, capture_output=True, env=env)
    r = json.loads(rep.read_text())
    rows = []
    for e in r["elites"]:
        for s in e["seeds"]:
            rows.append({
                "fire": fire, "binary_git": r["binary_git"],
                "coords": e["coords"], "archive_growth": e["archive_descriptor"][0],
                "archive_elongation": e["archive_descriptor"][1],
                "observed_growth_day5": r["observed_growth_day5"],
                "observed_elongation_day5": r["observed_elongation_day5"],
                "seed": s["seed"], "total_burned": s["total_burned"],
                "components": s["components"],
                "largest_component_fraction": s["largest_component_fraction"],
                "whole_set_elongation": s["whole_set_elongation"],
                "largest_component_elongation": s["largest_component_elongation"],
            })
    best_whole = max((row["whole_set_elongation"] for row in rows), default=float("nan"))
    best_largest = max((row["largest_component_elongation"] for row in rows), default=float("nan"))
    min_frac = min((row["largest_component_fraction"] for row in rows), default=float("nan"))
    print(f"{fire:14s} elites {len(r['elites'])} seeds/elite 3 | best whole-set elong {best_whole:.2f} "
          f"best largest-component elong {best_largest:.2f} | min largest-fraction {min_frac:.3f} "
          f"| observed {r['observed_elongation_day5']:.2f}", flush=True)
    return rows


if __name__ == "__main__":
    from concurrent.futures import ThreadPoolExecutor
    with ThreadPoolExecutor(max_workers=4) as ex:
        results = list(ex.map(run, c.FIRES))
    all_rows = [row for rows in results for row in rows]
    (c.EXP / "exp43_replay.json").write_text(json.dumps(all_rows, indent=1))
