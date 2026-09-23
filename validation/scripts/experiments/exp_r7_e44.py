#!/usr/bin/env python
"""E44 (Round 7 Task 4): the promotion test for the E30b Arm B
configuration -- five-seed forecast, plus E37b re-run at the 4x clock.

TEST_PLAN v1.9. Two batches, run one at a time (never together):

1. Forecast: `ARM_B` preset (r7_common), seeds 0-4, six fires, `assim`
   mode -- 30 runs at 2 workers (~10h on this box; do not compare that
   wall time across batches). Judged against the E33 five-seed baseline
   (r7_common.e33_baseline()) with its sd as the tie bar. Output
   exp44_arm_b_5seed.json.
2. E37b at the 4x clock: `map` mode (MAP-Elites illumination), 960
   evaluations per fire (SMC_GENERATIONS=30 x SMC_POP=32, identical to
   E37/E37b), Arm B preset, six fires -- one batch, run only after the
   forecast batch above. Output exp44_e37b_4x_illuminate.json.

Prediction (write before the run): five-seed mean beats E33 beyond 2 sd
on Brattain, Chimney, Ferguson; ties Bear and Pier; Buck within its own
sd. Stop rule: if the five-seed mean loses to E33 beyond 1 sd on any
fire, Arm B is not promoted -- E45/E46 still run (they explain the
pilot), E47 does not.

`--dry-run` prints both batches' jobs and commands; launches nothing.
`--stage {forecast,map,all}` (default all) selects which batch(es) to
build jobs for, so the forecast and map batches can be launched
separately per the "one batch at a time" rule.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_common as c  # noqa: E402

FORECAST_OUT = "exp44_arm_b_5seed.json"
MAP_OUT = "exp44_e37b_4x_illuminate.json"


def forecast_jobs():
    return [
        (fire, f"armB_seed{seed}", {**c.ARM_B, "SMC_SEED": str(seed)}, 32, "assim")
        for fire in c.FIRES
        for seed in range(5)
    ]


def map_jobs():
    env = {**c.ARM_B, "SMC_MAP_DAYS": "5", "SMC_GENERATIONS": "30", "SMC_POP": "32"}
    return [(fire, "armB_e37b_4x", env, 32, "map") for fire in c.FIRES]


if __name__ == "__main__":
    p = c.arg_parser(__doc__)
    p.add_argument("--stage", choices=["forecast", "map", "all"], default="all",
                    help="which batch to build/run (default: all, but batches still run one at a time)")
    args = p.parse_args()

    stages = []
    if args.stage in ("forecast", "all"):
        stages.append((forecast_jobs(), FORECAST_OUT))
    if args.stage in ("map", "all"):
        stages.append((map_jobs(), MAP_OUT))

    if args.dry_run:
        for jobs, out_json in stages:
            c.dry_run(jobs, out_json)
    else:
        for jobs, out_json in stages:
            c.run_all(jobs, out_json, workers=args.workers)
