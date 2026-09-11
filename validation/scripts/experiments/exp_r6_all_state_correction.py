#!/usr/bin/env python
"""E40b (post-hoc control): state correction applied to EVERY resampled
child, not just the 20% immigrants (E40). `SMC_STATE_CORRECTION=all`: a
child's grid is rebuilt from the observation just scored every window,
for everyone, while it keeps its own resampled/mutated genome -- learning
continues, only the grid is corrected. Same base configuration as
E33/E40 (assim, beta 10, sigma 0.2, immigrants 0.2, containment only),
reset and gate left off. Five seeds matched to E33/E40, all six fires.
Output: exp40b_all_state_correction.json."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

if __name__ == "__main__":
    jobs = [(f, f"all_seed{s}", {"SMC_SEED": str(s), "SMC_STATE_CORRECTION": "all"}, 32)
            for s in range(5) for f in c.FIRES]
    c.run_all(jobs, "exp40b_all_state_correction.json", workers=4)
