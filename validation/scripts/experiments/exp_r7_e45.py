#!/usr/bin/env python
"""E45 (Round 7 Task 5): mechanism ablations on the `wind_rot_deg` gene --
why does it work? Two arms, run one batch per arm (never together):

- Arm B-sigma0: Arm B preset with `SMC_WIND_ROT_SIGMA=0` -- mutation
  sigma 0 on `wind_rot_deg`, so each member keeps its birth rotation
  (diversity, no learning). `SMC_WIND_ROT_SIGMA` is a Task 5 knob (not
  yet implemented as of Round 7 Task 2 -- this script only declares the
  arm; --dry-run does not need the knob to exist).
- Arm B-20: Arm B preset with `SMC_WIND_ROT_GENE=20` (narrower +-20
  degree range instead of +-90 -- learned-correction-only test).

Seeds (pre-registered both branches; pick the one E44's own result
satisfies and say so in the write-up): 0-4 if E44's Arm B five-seed sd is
<= the E33 sd on >= 4 fires; otherwise 0-2. `--seeds {5,3}` selects which
branch to build jobs for (no default -- the write-up must say which
branch E44 selected before this is run for real).

Six fires, 2 workers, one arm per batch. Prediction (write before the
run): diversity wins -- Arm B-sigma0 within 1 sd of Arm B on >= 4 fires;
Arm B-20 loses to Arm B beyond 1 sd on Bear and Pier (Arm B's own learned
medians there are +-43 degrees). Output exp45_wind_rot_mechanism.json.

`--dry-run` prints both arms' jobs and commands; launches nothing.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_common as c  # noqa: E402

OUT = "exp45_wind_rot_mechanism.json"

ARM_B_SIGMA0 = {**c.ARM_B, "SMC_WIND_ROT_SIGMA": "0"}
ARM_B_20 = {**c.ARM_B, "SMC_WIND_ROT_GENE": "20"}


def jobs_for(seeds):
    out = []
    for fire in c.FIRES:
        for seed in seeds:
            out.append((fire, f"armB_sigma0_seed{seed}", {**ARM_B_SIGMA0, "SMC_SEED": str(seed)}, 32, "assim"))
            out.append((fire, f"armB_20_seed{seed}", {**ARM_B_20, "SMC_SEED": str(seed)}, 32, "assim"))
    return out


if __name__ == "__main__":
    p = c.arg_parser(__doc__)
    p.add_argument("--seeds", choices=["5", "3"], default=None,
                    help="'5' for seeds 0-4 (E44's Arm B sd <= E33's on >= 4 fires), "
                         "'3' for seeds 0-2 (otherwise) -- see TEST_PLAN.md v1.9 E45. "
                         "Required to actually launch a batch; a bare --dry-run with no "
                         "--seeds prints both pre-registered branches.")
    args = p.parse_args()

    if args.dry_run:
        if args.seeds is None:
            print("# both branches pre-registered (TEST_PLAN.md v1.9 E45); "
                  "pick one with --seeds once E44 has a result")
            c.dry_run(jobs_for(range(5)), OUT + " (seeds=5 branch)")
            c.dry_run(jobs_for(range(3)), OUT + " (seeds=3 branch)")
        else:
            c.dry_run(jobs_for(range(int(args.seeds))), OUT)
    else:
        if args.seeds is None:
            p.error("--seeds {5,3} is required to launch a batch -- see TEST_PLAN.md v1.9 E45 "
                     "for which branch E44's result selects")
        c.run_all(jobs_for(range(int(args.seeds))), OUT, workers=args.workers)
