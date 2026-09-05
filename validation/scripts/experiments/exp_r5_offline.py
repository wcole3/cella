#!/usr/bin/env python
"""E36: offline evolution, then forecast. `wildfire_smc evolve`: a GA fits
the genes to the first 3 observed perimeters (mean IoU, population 24,
20 generations, 2 repeats), then the winner runs forward as a 32-member
open ensemble scored on every day. Days 4+ are honest forecasts; the
analysis compares them with the filter's forecasts on the same days
(E33 seed 0). Output: exp36_offline.json (rows carry per_day_* lists and
the fit report fields)."""
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

OUT = c.EXP / "exp36_offline"

def run(fire):
    row = c.run(OUT, fire, "evolve_fit3", {"SMC_FIT_DAYS": "3", "SMC_GENERATIONS": "20", "SMC_POP": "24", "SMC_REPEATS": "2"},
                members=32, mode="evolve")
    rep = json.loads((OUT / f"{fire}_evolve_fit3.json").read_text())
    row["fit"] = rep["fit"]
    return row

if __name__ == "__main__":
    from concurrent.futures import ThreadPoolExecutor
    with ThreadPoolExecutor(max_workers=3) as ex:
        rows = list(ex.map(run, c.FIRES))
    (c.EXP / "exp36_offline.json").write_text(json.dumps(rows, indent=1))
