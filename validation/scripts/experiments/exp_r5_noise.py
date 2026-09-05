#!/usr/bin/env python
"""E33: noise floor. Five seeds (prior draw + member RNG + containment
rolls) of the recommended configuration (assim, beta 10, sigma 0.2,
immigrants 0.2, containment only, M 32) on all six fires. Seed 0 is the
E31 row. Output: exp33_noise.json."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

if __name__ == "__main__":
    jobs = [(f, f"base_seed{s}", {"SMC_SEED": str(s)}, 32) for s in range(5) for f in c.FIRES]
    c.run_all(jobs, "exp33_noise.json")
