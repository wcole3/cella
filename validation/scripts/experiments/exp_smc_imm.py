#!/usr/bin/env python
"""E25b: assimilating ensemble with immigration (20 % of each generation
re-drawn from the prior) to hold diversity. M = 32, beta 10, sigma 0.2."""
import json, os, subprocess, sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import exp_smc as base  # noqa: E402

base.CONFIGS = {"assim_b10_s0.2_imm0.2": ("assim", {"SMC_BETA": "10", "SMC_SIGMA": "0.2", "SMC_IMMIGRANTS": "0.2"})}
base.M = 32

if __name__ == "__main__":
    base.OUT.mkdir(parents=True, exist_ok=True)
    jobs = [(f, c) for c in base.CONFIGS for f in base.FIRES]
    with ThreadPoolExecutor(max_workers=2) as ex:
        rows = list(ex.map(base.run, jobs))
    (base.EXP / "exp25b_smc_imm.json").write_text(json.dumps(rows, indent=1))
