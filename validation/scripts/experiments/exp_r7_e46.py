#!/usr/bin/env python
"""E46 (Round 7 Task 6): is the coarse ERA5 daily-mean wind the real
problem, or is the gene doing more than repairing a bad input? Three
arms, 3 seeds (0-2), six fires, 2 workers, one arm per batch (never
together):

- (a) `station_a`: E33's own recommended config (r5_common.BASE_ENV,
  Bernoulli spread, no arrival kernel) + `SMC_WIND_SOURCE=station`.
- (b) `station_b`: Arm B preset with `SMC_WIND_ROT_GENE` unset (no gene)
  + `SMC_WIND_SOURCE=station`.
- (c) `station_c`: Arm B preset (gene on) + `SMC_WIND_SOURCE=station`.

`SMC_WIND_SOURCE=era5|station` (default era5) is a Task 6 knob (not yet
implemented as of Round 7 Task 2 -- this script only declares the arms;
--dry-run does not need the knob to exist). Under `station`, the driver
would receive the station vector mean over each window instead of the
ERA5 daily vector, falling back to ERA5 for any window with a station
log gap.

Prediction (write before the run): (a) ties E33 everywhere (a round
Bernoulli blob only cares about wind speed); (b) recovers most of the
gene's gain on Ferguson and Chimney (E41: direction carries signal
there); (c) beats (b) by less than 1 sd on >= 4 fires -- if (c) beats
(b) beyond 2 sd on >= 3 fires, the gene does more than repair the input.

Output exp46_station_wind_input.json. `--dry-run` prints all three
arms' jobs and commands; launches nothing.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_common as c  # noqa: E402

OUT = "exp46_station_wind_input.json"

ARM_A_STATION = {"SMC_WIND_SOURCE": "station"}
ARM_B_NO_GENE_STATION = {k: v for k, v in c.ARM_B.items() if k != "SMC_WIND_ROT_GENE"}
ARM_B_NO_GENE_STATION = {**ARM_B_NO_GENE_STATION, "SMC_WIND_SOURCE": "station"}
ARM_B_GENE_STATION = {**c.ARM_B, "SMC_WIND_SOURCE": "station"}

ARMS = {
    "station_a": ARM_A_STATION,
    "station_b": ARM_B_NO_GENE_STATION,
    "station_c": ARM_B_GENE_STATION,
}


def jobs():
    out = []
    for fire in c.FIRES:
        for seed in range(3):
            for label, env in ARMS.items():
                out.append((fire, f"{label}_seed{seed}", {**env, "SMC_SEED": str(seed)}, 32, "assim"))
    return out


if __name__ == "__main__":
    args = c.arg_parser(__doc__).parse_args()
    all_jobs = jobs()

    if args.dry_run:
        c.dry_run(all_jobs, OUT)
    else:
        c.run_all(all_jobs, OUT, workers=args.workers)
