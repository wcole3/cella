#!/usr/bin/env python
"""E42: what the particle filter's posterior learns over time, and whether
the learned containment operator agrees with ICS-209's reported percent
contained. Analysis only — no new `wildfire_smc` runs. Everything needed
is already on disk from E33 (five seeds of the recommended 32-member
assimilating ensemble): `results/experiments/exp33_noise/<fire>_base_seed
<s>.json` carries per-observation `p0_mean`, `dur_mean`,
`wind_scale_mean`, `contained_fraction`, `area_ratio_mean` and
`obs_burned` (the truth mask's cell count at that observation — verified
against `truth.json`'s `arrival_hours` directly, so it is used here
in place of re-deriving the same count from the raw arrival-time array),
plus each member's FINAL genome (`contain_a`, `contain_b`, ...).

Two things this script computes, per fire, aligned by "day since the
first mask" (day 0 = the report's first observation; the ignition-to-
first-mask window is never scored, same convention as GLOSSARY.md's
"Observation day / window"):

1. Posterior trajectories. Cross-seed mean, sd and median of p0, burn
   duration, wind multiplier, contained fraction and area ratio at each
   observation.

2. The "model containment curve". A **posterior** is where the filter's
   knobs end up after learning (GLOSSARY.md); a **hazard** here means
   the day's own probability of a member getting contained, not the
   cumulative chance. For every member's FINAL `(contain_a, contain_b)`
   (all 5 seeds x 32 members = 160 members per fire), the daily hazard
   `sigmoid(a + b * ln(g_d))`, where `g_d` is the OBSERVED daily growth
   from truth, `(obs_burned[d] - obs_burned[d-1]) / obs_burned[d-1]`,
   floored at 1e-4 exactly as
   `cella_lib/src/wildfire/driver.rs:226` floors it
   (`.max(1e-4)`) — this is the same formula the driver itself evaluates
   once per simulated day (`driver.rs:205-236`, `period_end`), just fed
   the real fire's growth instead of a simulated member's. The
   **cumulative probability** of having been contained by day d is then
   `1 - product((1 - hazard_i) for i in 1..=d)` (an absorbing chain: once
   contained, a member stays contained, so "not yet contained" only ever
   shrinks) — this is a plain description of what "cumulative probability"
   means, not a new formula. Averaging that over all 160 members gives
   the model containment curve, compared against ICS-209's
   `PCT_CONTAINED_COMPLETED` from `containment.json` on the same day axis
   (`containment.json`'s `hours` field is already "hours since scenario
   t0" per its own provenance note, the same timebase as the E33 reports'
   `hours`, so day = (hours - hours_of_first_mask) / 24 for both curves;
   no separate date parsing was needed).

Two honesty points carried into the E42 experiment file:
- The hazard uses each member's genome AS IT ENDED, not the genome it
  had on the day in question — the per-day genome is not stored. This is
  what the FINAL population (selected by the whole observation series,
  hindsight included) would have done if exposed to the real growth
  sequence, not what any member actually did while the filter was
  running.
- A few observation gaps in the E33 reports are wider than 24 hours
  (recorded below as `wide_gaps`); those are still treated as a single
  hazard-evaluation step, which likely UNDER-counts how many daily rolls
  the real per-day driver would have made inside that gap, biasing the
  model curve toward LESS containment across a gap than the driver would
  actually produce.

Usage: python exp_r6_posterior.py
Writes: results/experiments/exp42_posterior.json
"""
import json
import math

from r5_common import EXP, FIRES, HOLDOUT, VAL

METRICS = ["p0_mean", "dur_mean", "wind_scale_mean", "contained_fraction", "area_ratio_mean"]
SEEDS = range(5)
# cella_lib/src/wildfire/driver.rs:226 — `.max(1e-4)` on the relative
# growth before taking its log, so a flat or shrinking truth mask cannot
# send ln(growth) to -infinity.
GROWTH_FLOOR = 1e-4


def sigmoid(x):
    return 1.0 / (1.0 + math.exp(-x))


def mean_sd_median(xs):
    n = len(xs)
    m = sum(xs) / n
    var = sum((x - m) ** 2 for x in xs) / n if n > 1 else 0.0
    sd = math.sqrt(var)
    s = sorted(xs)
    med = s[n // 2] if n % 2 else (s[n // 2 - 1] + s[n // 2]) / 2.0
    return m, sd, med


def load_reports(fire):
    return [json.loads((EXP / "exp33_noise" / f"{fire}_base_seed{s}.json").read_text()) for s in SEEDS]


def posterior_trajectory(fire, reports):
    scores0 = reports[0]["scores"]
    n = len(scores0)
    hours0 = [s["hours"] for s in scores0]
    for r in reports[1:]:
        hrs = [s["hours"] for s in r["scores"]]
        obsb = [s["obs_burned"] for s in r["scores"]]
        assert hrs == hours0, f"{fire}: seed hours mismatch"
        assert obsb == [s["obs_burned"] for s in scores0], f"{fire}: seed obs_burned mismatch"
    day0 = hours0[0]
    obs = []
    for i in range(n):
        row = {
            "day": (hours0[i] - day0) / 24.0,
            "hours": hours0[i],
            "obs_burned": scores0[i]["obs_burned"],
        }
        for metric in METRICS:
            vals = [r["scores"][i][metric] for r in reports]
            m, sd, med = mean_sd_median(vals)
            row[metric + "_mean"] = m
            row[metric + "_sd"] = sd
            row[metric + "_median"] = med
        obs.append(row)
    return obs


def wide_gaps(obs):
    """Observation gaps wider than 24h (one hazard step covers >1 real
    day) — see the module docstring's second honesty point."""
    out = []
    for i in range(1, len(obs)):
        gap_hours = obs[i]["hours"] - obs[i - 1]["hours"]
        if gap_hours > 24.0 + 1e-6:
            out.append({"day": obs[i]["day"], "gap_hours": gap_hours})
    return out


def containment_curve(reports, obs):
    """The model containment curve: cumulative probability of having been
    contained by day d, averaged over every member's final genome, with
    the hazard driven by the real fire's observed growth (from `obs`,
    itself read from truth). See module docstring for the formula and
    its floor.
    """
    members = []
    for r in reports:
        for g in r["final_genomes"]:
            members.append((g["contain_a"]["Float"], g["contain_b"]["Float"]))
    n = len(obs)
    floor_hits = 0
    growths = []
    for d in range(1, n):
        before = obs[d - 1]["obs_burned"]
        burned = obs[d]["obs_burned"]
        raw = (burned - before) / before
        g = max(raw, GROWTH_FLOOR)
        if g == GROWTH_FLOOR and raw < GROWTH_FLOOR:
            floor_hits += 1
        growths.append(g)
    survival = [1.0] * len(members)
    # Day 0 (the first mask): the driver's own guard is `before > 0.0`,
    # true only from the *second* observation onward (state.get defaults
    # to 0.0 before the first period), so no member can be contained yet.
    curve = [0.0]
    for gd in growths:
        lg = math.log(gd)
        for i, (a, b) in enumerate(members):
            p = sigmoid(a + b * lg)
            survival[i] *= (1.0 - p)
        curve.append(1.0 - sum(survival) / len(survival))
    return curve, len(members), floor_hits


def crossing_day(days, values, threshold=0.5):
    """First day `values` reaches `threshold`, linearly interpolated
    between the point before and the point at/after the crossing. None
    if it never reaches it."""
    if not values:
        return None
    if values[0] >= threshold:
        return days[0]
    for i in range(1, len(values)):
        if values[i] >= threshold:
            d0, d1, v0, v1 = days[i - 1], days[i], values[i - 1], values[i]
            frac = (threshold - v0) / (v1 - v0) if v1 != v0 else 0.0
            return d0 + frac * (d1 - d0)
    return None


def ics209_curve(fire, hours_of_first_mask):
    path = VAL / "data" / "scenarios" / fire / "containment.json"
    data = json.loads(path.read_text())
    rows = sorted(data["rows"], key=lambda r: r["hours"])
    days = [(r["hours"] - hours_of_first_mask) / 24.0 for r in rows]
    pct = [r["pct_contained"] for r in rows]
    return {
        "incident_name": data.get("incident_name"),
        "incident_id": data.get("incident_id"),
        "provenance_note": data.get("provenance", {}).get("note"),
        "days": days,
        "pct_contained": pct,
    }


def main():
    out = {}
    for fire in FIRES:
        reports = load_reports(fire)
        obs = posterior_trajectory(fire, reports)
        curve, n_members, floor_hits = containment_curve(reports, obs)
        model_days = [row["day"] for row in obs]
        ics = ics209_curve(fire, obs[0]["hours"])
        model_day50 = crossing_day(model_days, curve)
        ics_day50 = crossing_day(ics["days"], [p / 100.0 for p in ics["pct_contained"]])
        out[fire] = {
            "holdout": fire in HOLDOUT,
            "n_members": n_members,
            "growth_floor_hits": floor_hits,
            "wide_gaps": wide_gaps(obs),
            "obs": obs,
            "model_containment_curve": [{"day": d, "contained_fraction_model": c}
                                         for d, c in zip(model_days, curve)],
            "model_day50": model_day50,
            "ics209": ics,
            "ics_day50": ics_day50,
            "day50_lead_days": (None if model_day50 is None or ics_day50 is None
                                 else ics_day50 - model_day50),
        }
    (EXP / "exp42_posterior.json").write_text(json.dumps(out, indent=1))
    print(f"wrote exp42_posterior.json, {len(out)} fires")
    for fire, r in out.items():
        tag = "*" if r["holdout"] else " "
        m50 = f"{r['model_day50']:.1f}" if r["model_day50"] is not None else "never"
        i50 = f"{r['ics_day50']:.1f}" if r["ics_day50"] is not None else "never"
        lead = f"{r['day50_lead_days']:+.1f}" if r["day50_lead_days"] is not None else "n/a"
        print(f"  {fire:14s}{tag} model day50 {m50:>6s}  ics day50 {i50:>6s}  lead {lead:>6s}")


if __name__ == "__main__":
    main()
