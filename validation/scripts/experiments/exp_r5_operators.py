#!/usr/bin/env python
"""E34: operator ablation inside the filter. One knob moved at a time from
the recommended configuration (beta 10, sigma 0.2, immigrants 0.2,
containment only, M 32, seed 0 = E33 seed 0):
  immigrants 0 / 0.1 / 0.4      sigma 0.1 / 0.4      beta 5 / 20
  crossover 0.5 / 1.0           (a new ensemble option: children may take
                                 each gene from either of two resampled parents)
Output: exp34_operators.json."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

CONFIGS = {
    "imm0": {"SMC_IMMIGRANTS": "0"}, "imm0.1": {"SMC_IMMIGRANTS": "0.1"}, "imm0.4": {"SMC_IMMIGRANTS": "0.4"},
    "sigma0.1": {"SMC_SIGMA": "0.1"}, "sigma0.4": {"SMC_SIGMA": "0.4"},
    "beta5": {"SMC_BETA": "5"}, "beta20": {"SMC_BETA": "20"},
    "cross0.5": {"SMC_CROSSOVER": "0.5"}, "cross1.0": {"SMC_CROSSOVER": "1.0"},
}

if __name__ == "__main__":
    jobs = [(f, label, env, 32) for label, env in CONFIGS.items() for f in c.FIRES]
    c.run_all(jobs, "exp34_operators.json")
