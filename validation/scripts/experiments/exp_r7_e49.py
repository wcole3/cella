#!/usr/bin/env python
"""E49 (Round 7 Task 7): the containment operator under the 4x clock.
Chimney's contained fraction fell to 0.56-0.59 under both E30b arms
(E33: 0.94) while IoU rose; Task 7's first job is to read the operator
and `SMC_STEPS_SCALE` handling and write down which constants (period,
growth-rate window, any per-tick rate constant) do and do not scale with
the clock.

Audit result (`validation/experiments/53-e49-containment-under-4x-clock.md`,
constants table): everything the containment operator itself reads
scales with `SMC_STEPS_SCALE`, either automatically (`period_steps` via
`steps_per_day_from`; the growth window, since it is exactly one period
regardless of tick count; `tau_days`'s decay, computed from hours, not
ticks; `SMC_ASSIM_EVERY`/`SMC_MAX_DAYS`, both counted in observation
windows/days, never steps) or by construction (`model.burn_duration`'s
gene range, widened from [5, 20] to [20, 80] -- exactly x4 -- in
`priors/arrival_x4.json`, the prior Arm B already uses). The `.max(1e-4)`
growth floor is dimensionless (a fraction of a period's own growth, not a
tick count) and needs no clock-dependent scaling at all. So this runner
takes the **sweep** branch (TEST_PLAN.md v1.9 E49, "if everything scales:
... sweep the containment threshold on the four calibration fires only").

Two arms, `--arm {fix, sweep}`, one arm per batch, same restructuring
Tasks 5/6 used for `exp_r7_e45.py`/`exp_r7_e46.py`:

- `--arm fix` (branch 2, NOT launched -- kept only so the pre-registered
  branch this task's original placeholder declared stays buildable and
  documented; the audit found no scaling bug to fix). Chimney seeds 0-4,
  Arm B preset unchanged, 5 runs. Output
  exp49_containment_4x_clock_fix.json.
- `--arm sweep` (branch 3, the one actually launched). Four calibration
  fires only (Bear, Brattain, Buck, Chimney -- Ferguson and Pier are
  holdout and stay untouched by this experiment), seed 0, three values
  of the new opt-in `SMC_CONTAIN_GROWTH_FLOOR` knob
  (`cella_lib::wildfire::driver::WildfireDriver::contain_growth_floor`,
  `default_contain_growth_floor() == 1e-4`, unset/default byte-identical
  to every report before this knob existed): 1e-5, 1e-4 (baseline), 1e-3
  -- one order of magnitude below and above the operator's own
  pre-existing hard-coded floor. This is the operator's one genuine
  scalar "threshold" (a floor `period_end` applies to a period's growth
  ratio before its `ln`, in the containment logit `a + b*ln(growth)`);
  `GENE_CONTAIN_A`/`GENE_CONTAIN_B` are free genes the filter fits per
  member, not a single fixed threshold value a sweep could hold
  constant, so they are not the sweep target. Arm B preset otherwise
  unchanged (arrival kernel, rear-focus wind law, 4x clock, arrival_x4
  prior, wind-rotation gene). 4 fires x 3 values = 12 runs. Output
  exp49_containment_4x_clock_sweep.json.

Prediction (write before the run, TEST_PLAN.md v1.9 verbatim): the
fraction is correct, not a bug -- the fire really grows for longer under
the faster clock, and the ICS-209 lead E42 measured (13-21 days) shrinks
toward the real 5-10 days. (Checked in Phase 2 against E44's own Chimney
Arm B reports -- already on disk, no re-run needed for that clause.)

`--dry-run` (with or without `--arm`) prints jobs and the exact
command/env for the selected arm(s); launches nothing. `--arm sweep` is
the only arm this task actually runs; `--arm fix` is provided for
completeness/documentation only, per the branch decision above.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_common as c  # noqa: E402

FIRE = "Chimney_2016"
CALIBRATION_FIRES = [f for f in c.FIRES if f not in c.HOLDOUT]
# The operator's own pre-existing hard-coded value (`default_contain_growth_floor`
# in `cella_lib/src/wildfire/driver.rs`) sits in the middle, one order of
# magnitude away from each swept endpoint.
SWEEP_VALUES = [1e-5, 1e-4, 1e-3]

ARMS = {
    "fix": ("exp49_containment_4x_clock_fix.json",),
    "sweep": ("exp49_containment_4x_clock_sweep.json",),
}


def fix_jobs():
    """Branch 2 (NOT launched -- see module docstring). Chimney seeds 0-4,
    Arm B preset unchanged; kept buildable/dry-runnable only."""
    return [
        (FIRE, f"armB_seed{seed}", {**c.ARM_B, "SMC_SEED": str(seed)}, 32, "assim")
        for seed in range(5)
    ]


def sweep_jobs():
    """Branch 3 (the one actually launched). Four calibration fires, seed
    0, three SMC_CONTAIN_GROWTH_FLOOR values, Arm B preset otherwise
    unchanged."""
    return [
        (
            fire,
            f"armB_seed0_floor{floor:g}",
            {**c.ARM_B, "SMC_SEED": "0", "SMC_CONTAIN_GROWTH_FLOOR": str(floor)},
            32,
            "assim",
        )
        for fire in CALIBRATION_FIRES
        for floor in SWEEP_VALUES
    ]


JOBS_FOR = {"fix": fix_jobs, "sweep": sweep_jobs}


if __name__ == "__main__":
    p = c.arg_parser(__doc__)
    p.add_argument("--arm", choices=list(ARMS), default=None,
                    help="which batch to build/run -- 'fix' (branch 2, not launched -- "
                         "kept for documentation) or 'sweep' (branch 3, the one this task "
                         "actually runs). Required to actually launch a batch; a bare "
                         "--dry-run with no --arm prints jobs for both.")
    args = p.parse_args()

    arm_names = [args.arm] if args.arm else list(ARMS)

    if args.dry_run:
        if args.arm is None:
            print("# no --arm given: printing jobs for both ('fix' is not launched; see module docstring)")
        for arm_name in arm_names:
            (out_json,) = ARMS[arm_name]
            c.dry_run(JOBS_FOR[arm_name](), out_json)
    else:
        if args.arm is None:
            p.error("--arm {fix,sweep} is required to launch a batch -- 'sweep' is the "
                     "branch this task actually runs (TEST_PLAN.md v1.9 E49: everything "
                     "scales, so sweep the containment threshold)")
        (out_json,) = ARMS[args.arm]
        c.run_all(JOBS_FOR[args.arm](), out_json, workers=args.workers)
