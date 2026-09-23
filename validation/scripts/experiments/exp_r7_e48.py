#!/usr/bin/env python
"""E48 (Round 7 Task 3): why does Brattain fail under arrival without the
gene? Re-run seed 0 on Brattain only, Arm A and Arm B (2 runs, 2
workers), with per-window diagnostics enabled via `SMC_DIAG=1`.

`SMC_DIAG=1` is a Task 3 knob (not yet implemented as of Round 7 Task 2
-- this script only declares the arms; --dry-run does not need the knob
to exist). Task 3's job, once it lands the knob, is to add (opt-in, no
existing field changed): consensus perimeter vs truth per window,
learned p0 and wind_scale (and wind_rot_deg for Arm B) trajectories, the
ERA5 wind vector per window, the station vector mean per window, and a
head-vs-flank decomposition of the miss.

Arm A: the arrival kernel alone (Arm B preset minus SMC_WIND_ROT_GENE).
Arm B: the full Arm B preset (arrival kernel + wind-rotation gene).

No score family -- this is a finding, compared against the E41 Ellipse
null's own per-window series on Brattain (not re-run here). Prediction
(write before the run): ERA5 direction is right on the daily mean but
wrong on the two or three windows that carry most of the burned area,
and Arm B's gene diversity covers exactly those windows.

Output exp48_brattain_arrival_diagnosis.json. `--dry-run` prints both
runs' jobs and commands; launches nothing.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_common as c  # noqa: E402

OUT = "exp48_brattain_arrival_diagnosis.json"
FIRE = "Brattain_2020"

ARM_A = {k: v for k, v in c.ARM_B.items() if k != "SMC_WIND_ROT_GENE"}
ARM_B = dict(c.ARM_B)


def jobs():
    return [
        (FIRE, "armA_seed0_diag", {**ARM_A, "SMC_SEED": "0", "SMC_DIAG": "1"}, 32, "assim"),
        (FIRE, "armB_seed0_diag", {**ARM_B, "SMC_SEED": "0", "SMC_DIAG": "1"}, 32, "assim"),
    ]


if __name__ == "__main__":
    args = c.arg_parser(__doc__).parse_args()
    all_jobs = jobs()

    if args.dry_run:
        c.dry_run(all_jobs, OUT)
    else:
        c.run_all(all_jobs, OUT, workers=args.workers)
