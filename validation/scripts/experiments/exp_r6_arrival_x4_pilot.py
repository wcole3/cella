#!/usr/bin/env python
"""E30b (Task 10): a one-seed, two-arm pilot of the arrival-time kernel
with a faster (4x) clock, a wider p0 prior, and (Arm B only) a learned
per-member wind-direction offset gene.

Same recommended configuration as E33/E30 (r5_common.BASE_ENV: assim mode,
beta 10, sigma 0.2, immigrants 0.2, containment-only stopping) plus
SMC_SPREAD=arrival SMC_WIND_LAW=rear_focus (E30's kernel), with two changes
E30's own diagnosis called for:

- SMC_STEPS_SCALE=4: the scenario's steps_per_hour x4 (200 ticks/day
  instead of 50), which raises the arrival rule's one-cell-per-tick front
  cap from 1.5 km/day to 6 km/day -- E30's own cap was below what
  Brattain's rear-focus head needed on day 5.
- SMC_PRIOR=priors/arrival_x4.json: model.p0 widens to log-uniform
  [0.02, 0.6] (per-day head speed 4-120 cells/day at 200 ticks/day, versus
  E30's 4-31) and model.burn_duration widens to [20, 80] (lifetime in
  hours is unchanged since ticks/day quadrupled); tau_days and wind_scale
  are the unchanged E25 prior.

Arm A is the above alone. Arm B adds SMC_WIND_ROT_GENE=90: a free,
per-member gene `wind_rot_deg` uniform on [-90, 90] degrees, added to the
forcing's wind from-bearing in the driver (see
cella_lib::wildfire::driver::GENE_WIND_ROT_DEG) -- each member can learn
its own correction to the reported wind direction, testing whether E41's
finding (the ERA5 daily direction is wrong on Chimney/Bear, right on
Ferguson/Brattain) is fixable by the filter itself.

Pilot scope: seed 0 only, six fires, two arms (12 runs), judged against
exp33_noise.json seed 0 and exp30_arrival_fires.json seed 0 with the E33
sd. No E37b re-run here (see task-10-brief.md / TEST_PLAN v1.8 addendum).

Each row also carries `wall_s`, the run's own wall-clock time (this pilot
is the first place that number is asked for; r5_common.run() itself is
left unchanged so every experiment that already depends on its return
shape keeps working -- this script just times the call from outside).

Output: exp30b_arrival_x4_pilot.json (+ raw dir
exp30b_arrival_x4_pilot/)."""
import json
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r5_common as c  # noqa: E402

PRIOR = Path(__file__).resolve().parent / "priors" / "arrival_x4.json"

BASE_KERNEL_ENV = {
    "SMC_SPREAD": "arrival",
    "SMC_WIND_LAW": "rear_focus",
    "SMC_STEPS_SCALE": "4",
    "SMC_PRIOR": str(PRIOR),
}
ARM_A_ENV = dict(BASE_KERNEL_ENV)
ARM_B_ENV = {**BASE_KERNEL_ENV, "SMC_WIND_ROT_GENE": "90"}


def timed_run(out_dir, fire, label, env, members):
    """Same as r5_common.run, plus the run's own wall-clock seconds."""
    t0 = time.time()
    row = c.run(out_dir, fire, label, env, members)
    row["wall_s"] = round(time.time() - t0, 1)
    return row


def run_all_timed(jobs, out_json, workers=4):
    out_dir = c.EXP / out_json.replace(".json", "")
    with ThreadPoolExecutor(max_workers=workers) as ex:
        rows = list(ex.map(lambda j: timed_run(out_dir, j[0], j[1], j[2], j[3]), jobs))
    (c.EXP / out_json).write_text(json.dumps(rows, indent=1))
    return rows


if __name__ == "__main__":
    jobs = [(f, "armA_seed0", {**ARM_A_ENV, "SMC_SEED": "0"}, 32) for f in c.FIRES]
    jobs += [(f, "armB_seed0", {**ARM_B_ENV, "SMC_SEED": "0"}, 32) for f in c.FIRES]
    t0 = time.time()
    run_all_timed(jobs, "exp30b_arrival_x4_pilot.json", workers=4)
    print(f"total wall time: {time.time() - t0:.1f}s for {len(jobs)} runs", flush=True)
