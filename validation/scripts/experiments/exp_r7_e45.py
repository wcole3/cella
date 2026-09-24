#!/usr/bin/env python
"""E45 (Round 7 Task 5): mechanism ablations on the `wind_rot_deg` gene --
why does it work? Two arms, run one batch per arm (never together), plus
an optional third small batch:

- Arm B-sigma0 (`--arm sigma0`): Arm B preset with `SMC_WIND_ROT_SIGMA=0`
  -- mutation sigma 0 on `wind_rot_deg`, so each member keeps its birth
  rotation for the rest of the run (diversity, no learning; resampling
  still copies it and selection still acts on it -- only mutation stops).
  Output exp45_wind_rot_mechanism_sigma0.json.
- Arm B-20 (`--arm 20`): Arm B preset with `SMC_WIND_ROT_GENE=20`
  (narrower +-20 degree range instead of +-90 -- learned-correction-only
  test). Output exp45_wind_rot_mechanism_20.json.
- Arm B diag (`--arm armb_diag`, OPTIONAL, run only if the two batches
  above finish within budget): Arm B preset unchanged, `SMC_DIAG=1` only
  -- E44's own forecast batch did not set SMC_DIAG, so Arm B's per-window
  wind_rot_deg IQR series is otherwise unavailable; this re-runs Arm B
  itself (not a new arm) purely to recover that series, same seeds as
  the other two batches. Output exp45_arm_b_diag.json. Do not overwrite
  or reinterpret E44's own exp44_arm_b_5seed.json/summary.

All three arms/batches set `SMC_DIAG=1` so every raw report carries the
per-window `wind_rot_deg` median and IQR (Round 7 Task 5's new field,
`diag.wind_rot_deg_iqr` on each scored window) needed for the IQR-trend
part of this experiment (TEST_PLAN.md v1.9 E45: "the per-window posterior
spread (IQR) of wind_rot_deg for Arm B, Arm B-sigma0 and Arm B-20").

Seeds (pre-registered both branches; pick the one E44's own result
satisfies and say so in the write-up): 0-4 if E44's Arm B five-seed sd is
<= the E33 sd on >= 4 fires; otherwise 0-2. `--seeds {5,3}` selects which
branch to build jobs for (no default -- the write-up must say which
branch E44 selected before this is run for real).

Six fires, 2 workers, one arm per batch (`--arm`, required to launch;
--dry-run without it prints jobs for every arm x both seed branches).
Prediction (write before the run): diversity wins -- Arm B-sigma0 within
1 sd of Arm B on >= 4 fires; Arm B-20 loses to Arm B beyond 1 sd on Bear
and Pier (Arm B's own learned medians there are +-43 degrees).

`--dry-run` prints jobs and commands for the selected arm(s)/branch(es);
launches nothing.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_common as c  # noqa: E402

# SMC_DIAG=1 on every arm here (including the optional Arm B re-run) --
# see the module doc comment above for why.
ARM_B_SIGMA0 = {**c.ARM_B, "SMC_WIND_ROT_SIGMA": "0", "SMC_DIAG": "1"}
ARM_B_20 = {**c.ARM_B, "SMC_WIND_ROT_GENE": "20", "SMC_DIAG": "1"}
ARM_B_DIAG = {**c.ARM_B, "SMC_DIAG": "1"}

# arm name -> (env, job label prefix, output json)
ARMS = {
    "sigma0": (ARM_B_SIGMA0, "armB_sigma0", "exp45_wind_rot_mechanism_sigma0.json"),
    "20": (ARM_B_20, "armB_20", "exp45_wind_rot_mechanism_20.json"),
    "armb_diag": (ARM_B_DIAG, "armB_diag", "exp45_arm_b_diag.json"),
}


def jobs_for(env, label_prefix, seeds):
    return [
        (fire, f"{label_prefix}_seed{seed}", {**env, "SMC_SEED": str(seed)}, 32, "assim")
        for fire in c.FIRES
        for seed in seeds
    ]


if __name__ == "__main__":
    p = c.arg_parser(__doc__)
    p.add_argument("--arm", choices=list(ARMS), default=None,
                    help="which batch to build/run -- 'sigma0', '20', or the optional "
                         "'armb_diag' re-run. Required to actually launch a batch (one arm "
                         "per batch, never together); a bare --dry-run with no --arm prints "
                         "jobs for all three.")
    p.add_argument("--seeds", choices=["5", "3"], default=None,
                    help="'5' for seeds 0-4 (E44's Arm B sd <= E33's on >= 4 fires), "
                         "'3' for seeds 0-2 (otherwise) -- see TEST_PLAN.md v1.9 E45. "
                         "Required to actually launch a batch; a bare --dry-run with no "
                         "--seeds prints both pre-registered branches.")
    args = p.parse_args()

    arm_names = [args.arm] if args.arm else list(ARMS)
    seed_branches = [int(args.seeds)] if args.seeds else [5, 3]

    if args.dry_run:
        if args.arm is None:
            print("# no --arm given: printing jobs for all three (sigma0, 20, armb_diag)")
        if args.seeds is None:
            print("# both seed branches pre-registered (TEST_PLAN.md v1.9 E45); "
                  "pick one with --seeds once E44 has a result")
        for arm_name in arm_names:
            env, label_prefix, out_json = ARMS[arm_name]
            for n_seeds in seed_branches:
                suffix = "" if args.seeds else f" (seeds={n_seeds} branch)"
                c.dry_run(jobs_for(env, label_prefix, range(n_seeds)), out_json + suffix)
    else:
        if args.arm is None:
            p.error("--arm {sigma0,20,armb_diag} is required to launch a batch -- one arm "
                     "per batch, TEST_PLAN.md v1.9 E45")
        if args.seeds is None:
            p.error("--seeds {5,3} is required to launch a batch -- see TEST_PLAN.md v1.9 E45 "
                     "for which branch E44's result selects")
        env, label_prefix, out_json = ARMS[args.arm]
        c.run_all(jobs_for(env, label_prefix, range(int(args.seeds))), out_json, workers=args.workers)
