#!/usr/bin/env python
"""E49 (Round 7 Task 7): the containment operator under the 4x clock.
Chimney's contained fraction fell to 0.56-0.59 under both E30b arms
(E33: 0.94) while IoU rose; Task 7's first job is to read the operator
and `SMC_STEPS_SCALE` handling and write down which constants (period,
growth-rate window, any per-tick rate constant) do and do not scale with
the clock.

Placeholder scope (Round 7 Task 2): declares the Chimney seeds 0-4 Arm B
re-run only -- the branch Task 7 takes if something is found that does
not scale and should (fixed opt-in via the preset, so E44's own reports
stay reproducible). 5 runs, 2 workers. If Task 7 instead finds everything
already scales correctly, it sweeps the containment threshold on the
four calibration fires only (Bear, Brattain, Buck, Chimney), one seed,
three values, holdout untouched -- that branch has no fixed knob values
yet (the threshold values are Task 7's to choose) so it is not built
here; TEST_PLAN.md v1.9 pre-registers the design, not the sweep values.

Prediction (write before the run): the fraction is correct, not a bug --
the fire really grows for longer under the faster clock, and the
ICS-209 lead E42 measured (13-21 days) shrinks toward the real 5-10
days.

Output exp49_containment_4x_clock.json. `--dry-run` prints the Chimney
re-run's jobs and commands; launches nothing.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_common as c  # noqa: E402

OUT = "exp49_containment_4x_clock.json"
FIRE = "Chimney_2016"


def jobs():
    return [
        (FIRE, f"armB_seed{seed}", {**c.ARM_B, "SMC_SEED": str(seed)}, 32, "assim")
        for seed in range(5)
    ]


if __name__ == "__main__":
    args = c.arg_parser(__doc__).parse_args()
    all_jobs = jobs()

    if args.dry_run:
        c.dry_run(all_jobs, OUT)
    else:
        c.run_all(all_jobs, OUT, workers=args.workers)
