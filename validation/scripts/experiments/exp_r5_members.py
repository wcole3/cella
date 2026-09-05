#!/usr/bin/env python
"""E32: ensemble size. M = 8, 16, 64, 128 with the recommended operators
(M = 32 is E33 seed 0). Output: exp32_members.json. The 128-member runs
take ~4x the time and memory of 32; three in parallel fit in 62 GB."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

if __name__ == "__main__":
    jobs = [(f, f"members{m}", {}, m) for m in (8, 16, 64, 128) for f in c.FIRES]
    c.run_all(jobs, "exp32_members.json")
