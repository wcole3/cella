#!/usr/bin/env python
"""E47 (Round 7 Task 8): arrival + rear_focus + spotting genes together.
Run only if E44 (Task 4) did not trip its stop rule -- E43 showed
spotting genes widen the reachable wedge under Bernoulli; not yet
combined with the arrival kernel.

Two batches, run one at a time (never together):

1. Illumination: `map` mode, Arm B preset + `SMC_SPOT=1`, six fires, one
   batch. Compared against E44's E37b-at-4x table (exp44_e37b_4x_illuminate.json),
   not re-run here. Output exp47_arrival_spotting_illuminate.json.
2. Forecast: Arm B preset + `SMC_SPOT=1`, seeds 0-2, six fires, 2
   workers. Output exp47_arrival_spotting_forecast.json.

Prediction (write before the run): Ferguson coverage rises above Arm B
alone; forecast IoU ties Arm B on all six (spotting adds reach the
filter rarely needs). A forecast loss beyond 1 sd on any fire means the
wider prior costs more than reach buys.

`--dry-run` prints both batches' jobs and commands; launches nothing.
`--stage {illuminate,forecast,all}` (default all) selects which batch(es)
to build jobs for, so they can be launched separately per the "one batch
at a time" rule. This script does not check E44's stop rule itself --
whoever launches a real (non-dry-run) batch must first confirm from
TEST_PLAN.md v1.9 / the E44 write-up that E47 is still cleared to run.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_common as c  # noqa: E402

ILLUMINATE_OUT = "exp47_arrival_spotting_illuminate.json"
FORECAST_OUT = "exp47_arrival_spotting_forecast.json"

ARM_B_SPOT = {**c.ARM_B, "SMC_SPOT": "1"}


def illuminate_jobs():
    env = {**ARM_B_SPOT, "SMC_MAP_DAYS": "5", "SMC_GENERATIONS": "30", "SMC_POP": "32"}
    return [(fire, "armB_spot_illuminate", env, 32, "map") for fire in c.FIRES]


def forecast_jobs():
    return [
        (fire, f"armB_spot_seed{seed}", {**ARM_B_SPOT, "SMC_SEED": str(seed)}, 32, "assim")
        for fire in c.FIRES
        for seed in range(3)
    ]


if __name__ == "__main__":
    p = c.arg_parser(__doc__)
    p.add_argument("--stage", choices=["illuminate", "forecast", "all"], default="all",
                    help="which batch to build/run (default: all, but batches still run one at a time)")
    args = p.parse_args()

    stages = []
    if args.stage in ("illuminate", "all"):
        stages.append((illuminate_jobs(), ILLUMINATE_OUT))
    if args.stage in ("forecast", "all"):
        stages.append((forecast_jobs(), FORECAST_OUT))

    if args.dry_run:
        for jobs, out_json in stages:
            c.dry_run(jobs, out_json)
    else:
        for jobs, out_json in stages:
            c.run_all(jobs, out_json, workers=args.workers)
