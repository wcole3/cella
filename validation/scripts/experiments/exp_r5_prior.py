#!/usr/bin/env python
"""E35: prior width. Same operators, three priors:
  broad      the E25 prior (p0 0.08-0.6 log, dur 5-20, wind 0-1.5) = E33 seed 0
  narrow     +-25 % around the E28/E31 posterior medians (p0 0.18-0.32,
             dur 11-17, wind 0.5-0.9); containment genes at their defaults
  very_broad p0 0.02-0.95 log, dur 2-60, wind 0-1.5
SMC_CONTAIN=1 appends the containment genes and SMC_TAU_OFF drops tau, so
the prior files list the E25 genes only. Output: exp35_prior.json."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

PRIORS = Path(__file__).resolve().parent / "priors"

if __name__ == "__main__":
    jobs = [(f, f"prior_{p}", {"SMC_PRIOR": str(PRIORS / f"{p}.json")}, 32)
            for p in ("narrow", "very_broad") for f in c.FIRES]
    c.run_all(jobs, "exp35_prior.json")
