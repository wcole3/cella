#!/usr/bin/env python
"""E22b: the wind clock WITHOUT total-tick normalisation.

E22 kept the run's total tick count fixed (only the timing moved) and was
null at daily truth. Here windy hours add ticks on top of the declared
50/day — m(U) = 1 + k U^1.5 — so the fire is genuinely faster on run days,
with the E17 recipe's moisture x decay capping the total burn. k 0.1 / 0.2
(U = 5 m/s: x2.1 / x3.2). 3 seeds. Output exp22b_clock_unnorm.json.
"""
import json, sys
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).resolve().parent))
import exp_clock as ec  # noqa: E402
from exp_station import emc_fosberg, eta_moisture, hourly_windows, BEST, EXP  # noqa: E402

ec.OUT = EXP / "exp22b_clock_unnorm"


def main():
    ec.OUT.mkdir(parents=True, exist_ok=True)
    rows = []
    for fire, (p0, dur) in BEST.items():
        wind, st_rows, _ = hourly_windows(fire, 1.0)
        n = len(wind) - 1
        U = np.array([w["speed_ms"] for w in wind[:n]]); hours = np.array([w["hours"] for w in wind[:n]])
        eta = [1.0 if (r["rh_pct"] is None or r["temp_c"] is None) else eta_moisture(emc_fosberg(r["rh_pct"], r["temp_c"]), 35.0) for r in st_rows[:n]]
        recipe = [float(e * d) for e, d in zip(eta, np.exp(-hours / 120.0))]
        for k in (0.1, 0.2):
            m = (1.0 + k * U ** 1.5).tolist()
            rows.append(ec.run(fire, p0 * 4, dur, f"recipe_unnorm_k{k:g}", wind, recipe, m))
            rows[-1]["mean_tick_mult"] = float(np.mean(m))
        (EXP / "exp22b_clock_unnorm.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
