#!/usr/bin/env python
"""E39: area-ratio-gated immigrant reset. The base configuration (E33)
with SMC_IMM_RESET_GATE=1.0: an immigrant is reset (fresh, uncontained
state) only if the last assimilation's area ratio (mean member burned
area / observed burned area) is below 1 -- i.e. only while the
population is under-predicting the observed area, which is what E38's
lock-in repair actually needed. Five seeds, matched to E33/E38's, so
each seed is compared with its own E33 twin (and, in the tables, with
its E38 twin). Output: exp39_gated_reset.json."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

if __name__ == "__main__":
    jobs = [(f, f"gated_seed{s}", {"SMC_SEED": str(s), "SMC_IMM_RESET_GATE": "1.0"}, 32) for s in range(5) for f in c.FIRES]
    c.run_all(jobs, "exp39_gated_reset.json", workers=4)
