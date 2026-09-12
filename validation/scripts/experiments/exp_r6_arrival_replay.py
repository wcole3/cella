#!/usr/bin/env python
"""E30/Task 8, optional acceptance detail: post-hoc connected-component
replay on the arrival-kernel illumination archives
(exp30_arrival_illuminate), for the three fires E37 could not reach
(Brattain, Ferguson, Pier). Same concern E43's own replay raised for
spotting: the archive's elongation is a second-moment measure over
every tracked cell with no connectivity distinction, so an "inside the
wedge" claim needs checking against whether the shape is one coherent
blob or a round core plus scattered outliers. `wildfire_smc replay`
(SMC_MAP_REPLAY=<archive>), SMC_SPREAD=arrival SMC_WIND_LAW=rear_focus
(must match the archive being replayed), top 5 elites by elongation
at/above the observed day-5 size, 3 fresh seeds each. Output:
exp30_arrival_replay/<fire>.json (wildfire_smc's own report) and
exp30_arrival_replay.json (flattened summary rows, one per fire x elite
x seed)."""
import json, os, subprocess, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

OUT = c.EXP / "exp30_arrival_replay"
SRC = c.EXP / "exp30_arrival_illuminate"
FIRES = ["Brattain_2020", "Ferguson_2018", "Pier_2017"]


def run(fire):
    OUT.mkdir(parents=True, exist_ok=True)
    src = SRC / f"{fire}.json"
    rep = OUT / f"{fire}.json"
    env = {**os.environ, "SMC_SPREAD": "arrival", "SMC_WIND_LAW": "rear_focus", "SMC_TAU_OFF": "1",
           "SMC_MAP_DAYS": "5", "SMC_MAP_REPLAY": str(src), "SMC_REPLAY_TOP": "5", "SMC_REPLAY_SEEDS": "3"}
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
    best_largest = max((row["largest_component_elongation"] for row in rows), default=float("nan"))
    min_frac = min((row["largest_component_fraction"] for row in rows), default=float("nan"))
    print(f"{fire:14s} elites {len(r['elites'])} seeds/elite 3 | best largest-component elong {best_largest:.2f} "
          f"| min largest-fraction {min_frac:.3f} | observed {r['observed_elongation_day5']:.2f}", flush=True)
    return rows


if __name__ == "__main__":
    from concurrent.futures import ThreadPoolExecutor
    with ThreadPoolExecutor(max_workers=3) as ex:
        results = list(ex.map(run, FIRES))
    all_rows = [row for rows in results for row in rows]
    (c.EXP / "exp30_arrival_replay.json").write_text(json.dumps(all_rows, indent=1))
