#!/usr/bin/env python
"""E30 (Task 8): the arrival-time kernel on the six real fires. Same
recommended configuration as E33 (r5_common.BASE_ENV: assim, beta 10,
sigma 0.2, immigrants 0.2, containment-only stopping), five seeds (0-4)
x six fires, but with SMC_SPREAD=arrival SMC_WIND_LAW=rear_focus added
-- E30a's recommended kernel (minimum-travel-time arrival, the
rear-focus wind law, validated for LB <= 1.5, which covers these six
fires' ERA5 wind x the ensemble's own wind_scale gene ceiling). `c2` is
not used by rear_focus and is left at its default (0.131);
`arrival_jitter` is also left at its default (0.2) -- E30a's
recommendation was the rule+law pair, not a tuned c2. E39's gate and
E40/E40b's state correction are both left off (BASE_ENV sets neither),
matching E33's twins seed for seed so the kernel change is isolated.
Output: exp30_arrival_fires.json (+ raw dir exp30_arrival_fires/)."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

ARRIVAL_ENV = {"SMC_SPREAD": "arrival", "SMC_WIND_LAW": "rear_focus"}

if __name__ == "__main__":
    jobs = [(f, f"arrival_seed{s}", {**ARRIVAL_ENV, "SMC_SEED": str(s)}, 32)
             for s in range(5) for f in c.FIRES]
    c.run_all(jobs, "exp30_arrival_fires.json", workers=4)
