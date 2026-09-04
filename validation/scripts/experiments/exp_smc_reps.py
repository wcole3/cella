#!/usr/bin/env python
"""E25d: replicate seeds (prior draw + member RNG) for the recommended
ensemble (assim, beta 10, sigma 0.2, immigrants 0.2, M 32)."""
import json, sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import exp_smc as base  # noqa: E402

base.CONFIGS = {f"assim_imm0.2_seed{s}": ("assim", {"SMC_BETA": "10", "SMC_SIGMA": "0.2", "SMC_IMMIGRANTS": "0.2", "SMC_SEED": str(s)}) for s in (1, 2)}
base.M = 32

if __name__ == "__main__":
    base.OUT.mkdir(parents=True, exist_ok=True)
    jobs = [(f, c) for c in base.CONFIGS for f in base.FIRES]
    with ThreadPoolExecutor(max_workers=4) as ex:
        rows = list(ex.map(base.run, jobs))
    (base.EXP / "exp25d_smc_reps.json").write_text(json.dumps(rows, indent=1))
