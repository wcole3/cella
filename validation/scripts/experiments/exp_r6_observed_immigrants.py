#!/usr/bin/env python
"""E40: immigrants seeded from the observed perimeter. The base
configuration (E33) with SMC_IMM_SOURCE=observed: instead of inheriting a
resampled parent's grid, an immigrant's grid is rebuilt straight from the
observed mask just scored -- burned interior becomes the burned type,
the rim of still-unburned fuel next to a burned cell becomes the burning
type at age 0, everything else is untouched. This is state correction
(Rochoux et al. 2014; Xue, Gu & Hu 2012), the standard particle-filter
move E38/E39 approximated by resetting an immigrant's *state* without
touching its grid. Reset and gate (SMC_IMM_RESET / SMC_IMM_RESET_GATE)
are left off so this effect is isolated from theirs. Five seeds, matched
to E33's (exp33_noise.json), on all six fires. Output:
exp40_observed_immigrants.json."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

if __name__ == "__main__":
    jobs = [(f, f"observed_seed{s}", {"SMC_SEED": str(s), "SMC_IMM_SOURCE": "observed"}, 32)
            for s in range(5) for f in c.FIRES]
    c.run_all(jobs, "exp40_observed_immigrants.json", workers=4)
