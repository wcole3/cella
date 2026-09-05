#!/usr/bin/env python
"""E38: immigrant reset. The base configuration (E33) with SMC_IMM_RESET=1:
an immigrant starts uncontained with p0 from its own genome instead of
inheriting its parent's contained flag. Five seeds, matched to E33's, so
each seed is compared with its own E33 twin. Output: exp38_immreset.json."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

if __name__ == "__main__":
    jobs = [(f, f"immreset_seed{s}", {"SMC_SEED": str(s), "SMC_IMM_RESET": "1"}, 32) for s in range(5) for f in c.FIRES]
    c.run_all(jobs, "exp38_immreset.json")
