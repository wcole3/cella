#!/usr/bin/env python
"""E46 (Round 7 Task 6): is the coarse ERA5 daily-mean wind the real
problem, or is the gene doing more than repairing a bad input? Three
arms, 3 seeds (0-2), six fires, 2 workers, one arm per batch (never
together -- restructured to `--arm <name>` the same way Task 5
restructured exp_r7_e45.py, one output file per arm):

- (a) `--arm station_a`: E33's own recommended config (r5_common.
  BASE_ENV, Bernoulli spread, no arrival kernel) + SMC_WIND_SOURCE=
  station. Output exp46_station_wind_input_a.json.
- (b) `--arm station_b`: Arm B preset with SMC_WIND_ROT_GENE unset (no
  gene) + SMC_WIND_SOURCE=station. Output
  exp46_station_wind_input_b.json.
- (c) `--arm station_c`: Arm B preset (gene on) + SMC_WIND_SOURCE=
  station. Output exp46_station_wind_input_c.json.

SMC_WIND_SOURCE=era5|station (default era5) is Round 7 Task 6's new
knob (cella_lib/examples/wildfire_smc/knobs.rs's WindSource, applied in
modes/open.rs's assim loop via nulls::wind_schedule_for -- see that
module's doc comments). Under `station`, the driver's per-window forcing
receives the station vector mean over that window instead of the ERA5
daily vector (same daily cadence, no sub-daily change), falling back to
that window's own ERA5 entry -- counted as `station_fallback_windows` in
each raw report -- where the station log has no samples in range.

Prediction (TEST_PLAN.md v1.9 E46, written before the run): (a) ties E33
everywhere (a round Bernoulli blob only cares about wind speed); (b)
recovers most of the gene's gain on Ferguson and Chimney (E41: ERA5
direction is wrong there and direction carries signal on those two); (c)
beats (b) by less than 1 sd on >= 4 fires. If (c) beats (b) beyond 2 sd
on >= 3 fires, the gene does more than repair the input.

Noise floor: arm (a) is judged against E33's five-seed mean/sd
(r7_common.e33_baseline()); arms (b)/(c) are judged against Arm B's own
five-seed mean/sd from E44 (r7_common.arm_b_baseline()); (c) is also
compared directly against (b), using Arm B's sd as the bar (the
pre-registered clause above).

`--dry-run` (with or without `--arm`) prints jobs and the exact
command/env for the selected arm(s); launches nothing. `--arm` is
required to actually launch a batch -- one arm per batch,
TEST_PLAN.md v1.9 E46.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_common as c  # noqa: E402

ARM_A_STATION = {"SMC_WIND_SOURCE": "station"}
ARM_B_NO_GENE_STATION = {k: v for k, v in c.ARM_B.items() if k != "SMC_WIND_ROT_GENE"}
ARM_B_NO_GENE_STATION = {**ARM_B_NO_GENE_STATION, "SMC_WIND_SOURCE": "station"}
ARM_B_GENE_STATION = {**c.ARM_B, "SMC_WIND_SOURCE": "station"}

# arm name -> (env, job label prefix, output json)
ARMS = {
    "station_a": (ARM_A_STATION, "station_a", "exp46_station_wind_input_a.json"),
    "station_b": (ARM_B_NO_GENE_STATION, "station_b", "exp46_station_wind_input_b.json"),
    "station_c": (ARM_B_GENE_STATION, "station_c", "exp46_station_wind_input_c.json"),
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
                    help="which batch to build/run -- 'station_a', 'station_b' or "
                         "'station_c'. Required to actually launch a batch (one arm per "
                         "batch, never together); a bare --dry-run with no --arm prints "
                         "jobs for all three.")
    args = p.parse_args()

    arm_names = [args.arm] if args.arm else list(ARMS)

    if args.dry_run:
        if args.arm is None:
            print("# no --arm given: printing jobs for all three (station_a, station_b, station_c)")
        for arm_name in arm_names:
            env, label_prefix, out_json = ARMS[arm_name]
            c.dry_run(jobs_for(env, label_prefix, range(3)), out_json)
    else:
        if args.arm is None:
            p.error("--arm {station_a,station_b,station_c} is required to launch a batch -- "
                     "one arm per batch, TEST_PLAN.md v1.9 E46")
        env, label_prefix, out_json = ARMS[args.arm]
        c.run_all(jobs_for(env, label_prefix, range(3)), out_json, workers=args.workers)
