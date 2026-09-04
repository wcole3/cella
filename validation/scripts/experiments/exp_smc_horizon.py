#!/usr/bin/env python
"""E25c: forecast horizon. Assimilate only every 2nd observation, so half
the scores are 2-window-ahead forecasts; and every 3rd. With immigration
0.2 (E25b setting). M = 32."""
import json, sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import exp_smc as base  # noqa: E402

base.CONFIGS = {
    "assim_imm0.2_every2": ("assim", {"SMC_BETA": "10", "SMC_SIGMA": "0.2", "SMC_IMMIGRANTS": "0.2", "SMC_ASSIM_EVERY": "2"}),
    "assim_imm0.2_every3": ("assim", {"SMC_BETA": "10", "SMC_SIGMA": "0.2", "SMC_IMMIGRANTS": "0.2", "SMC_ASSIM_EVERY": "3"}),
}
base.M = 32

if __name__ == "__main__":
    base.OUT.mkdir(parents=True, exist_ok=True)
    jobs = [(f, c) for c in base.CONFIGS for f in base.FIRES]
    with ThreadPoolExecutor(max_workers=3) as ex:
        rows = list(ex.map(base.run, jobs))
    (base.EXP / "exp25c_smc_horizon.json").write_text(json.dumps(rows, indent=1))
