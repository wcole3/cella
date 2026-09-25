#!/usr/bin/env python
"""E49 (Round 7 Task 7), Phase 2 analysis. Reads reports already on disk;
launches nothing. Three parts:

1. **Sweep table.** For each calibration fire (Bear, Brattain, Buck,
   Chimney) and each growth floor (1e-5, 1e-4, 1e-3), from
   `exp49_containment_4x_clock_sweep/`: mean consensus IoU, mean Brier,
   final contained fraction, the day the in-run contained fraction first
   reaches 0.5, and the medians of the final `contain_a`/`contain_b`.
   Delta against E44's Arm B seed-0 report (the floor-1e-4 twin), plus a
   byte-level check: are the three floor files identical, and is the
   1e-4 file identical to E44's apart from provenance and fields added
   after E44 ran?

2. **Draw diagnostic.** The one `--arm diag` report (Bear seed 0, floor
   1e-3, `SMC_DIAG=1`) records every daily containment draw. Because the
   three sweep runs were identical, this run's draws are the draws of all
   three. For each draw and each floor f: p_f = sigmoid(a + b*ln(max(g,
   f))), where g is the raw growth before the floor. A draw can only come
   out differently under two floors if the floor binds (g below it) AND
   the random number lands between the two probabilities; the chance of
   that is |p_f1 - p_f2|. Summed over draws: the expected number of draws
   that would have flipped; product of (1 - |dp|): the chance none did.

3. **ICS-209 lead.** E42's method (`exp_r6_posterior.py`, imported, not
   copied): each member's FINAL (contain_a, contain_b) fed the REAL fire's
   observed daily growth (floor 1e-4), cumulative containment chance
   averaged over members, 50% day compared with ICS-209's 50%-contained
   day on the same axis (days since the first mask). Lead = ICS-209 day50
   - model day50. Done for E33's five reports (1x clock, what E42 read)
   and E44's five Arm B reports (4x clock), per seed (32 members), the
   mean of the five, and pooled (160 members, E42's own number). A
   second, non-hindsight number: the day the report's own in-run
   `contained_fraction` first reaches 0.5, and its lead on ICS-209.
   Holdout fires (Ferguson, Pier) are left out on purpose.

Usage: python exp_r7_e49_analysis.py
Writes: results/experiments/exp49_analysis.json
"""
import json
import math
import statistics

import exp_r6_posterior as e42
from r5_common import EXP, FIRES, HOLDOUT

SEEDS = range(5)
CALIBRATION_FIRES = [f for f in FIRES if f not in HOLDOUT]
FLOORS = [("1e-05", 1e-5), ("0.0001", 1e-4), ("0.001", 1e-3)]
SWEEP = EXP / "exp49_containment_4x_clock_sweep"
DIAG = EXP / "exp49_containment_4x_clock_diag" / "Bear_2020_armB_seed0_floor0.001_diag.json"
# Fields a report gained after E44 ran (E46's wind source, E49's floor
# echo) plus provenance; everything else must match E44 exactly.
NOT_COMPARED = {"binary_git", "binary_built_utc", "wind_source",
                "station_fallback_windows", "contain_growth_floor"}
SOURCES = {
    "E33 (1x clock)": ("exp33_noise", "{fire}_base_seed{seed}.json"),
    "E44 Arm B (4x clock)": ("exp44_arm_b_5seed", "{fire}_armB_seed{seed}.json"),
}


def days_of(report):
    t0 = report["scores"][0]["hours"]
    return [(s["hours"] - t0) / 24.0 for s in report["scores"]]


def in_run_day50(report):
    return e42.crossing_day(days_of(report), [s["contained_fraction"] for s in report["scores"]])


def gene_median(report, key):
    return statistics.median(g[key]["Float"] for g in report["final_genomes"])


def comparable(report):
    return {k: v for k, v in report.items() if k not in NOT_COMPARED}


def sweep_table():
    out = {}
    for fire in CALIBRATION_FIRES:
        e44 = json.loads((EXP / "exp44_arm_b_5seed" / f"{fire}_armB_seed0.json").read_text())
        raw = {name: (SWEEP / f"{fire}_armB_seed0_floor{name}.json").read_bytes() for name, _ in FLOORS}
        rows = []
        for name, value in FLOORS:
            r = json.loads(raw[name])
            rows.append({
                "floor": value,
                "binary_git": r["binary_git"],
                "mean_consensus_iou": r["mean_consensus_iou"],
                "mean_brier_ensemble": r["mean_brier_ensemble"],
                "final_contained_fraction": r["scores"][-1]["contained_fraction"],
                "in_run_day50": in_run_day50(r),
                "contain_a_median": gene_median(r, "contain_a"),
                "contain_b_median": gene_median(r, "contain_b"),
                "delta_iou_vs_e44_seed0": r["mean_consensus_iou"] - e44["mean_consensus_iou"],
                "delta_brier_vs_e44_seed0": r["mean_brier_ensemble"] - e44["mean_brier_ensemble"],
            })
        twin = json.loads(raw["0.0001"])
        out[fire] = {
            "rows": rows,
            "three_floor_files_byte_identical": len(set(raw.values())) == 1,
            "floor_1e-4_equals_e44_seed0_except_provenance_and_new_fields":
                comparable(twin) == comparable(e44),
            "e44_seed0_mean_consensus_iou": e44["mean_consensus_iou"],
            "e44_seed0_binary_git": e44["binary_git"],
        }
    return out


def sigmoid(x):
    return 1.0 / (1.0 + math.exp(-x))


def draw_diagnostic():
    if not DIAG.exists():
        return None
    r = json.loads(DIAG.read_text())
    bear_1e3 = json.loads((SWEEP / "Bear_2020_armB_seed0_floor0.001.json").read_text())
    strip = lambda rep: [{k: v for k, v in s.items() if k != "diag"} for s in rep["scores"]]  # noqa: E731
    draws = [d for s in r["scores"] for d in s["diag"]["contain_draws"]]
    growth = [d["growth_raw"] for d in draws]
    out = {
        "binary_git": r["binary_git"],
        "contain_growth_floor_in_report": r.get("contain_growth_floor"),
        "scores_identical_to_sweep_floor_1e-3_run": strip(r) == strip(bear_1e3),
        "final_genomes_identical_to_sweep_floor_1e-3_run": r["final_genomes"] == bear_1e3["final_genomes"],
        "n_draws": len(draws),
        "n_contained_draws": sum(d["contained"] for d in draws),
        "min_growth_raw": min(growth) if growth else None,
        "n_growth_exactly_zero": sum(g == 0.0 for g in growth),
        "n_below": {name: sum(g < v for g in growth) for name, v in FLOORS},
    }
    # Per-draw probabilities under each floor, then pairwise flip odds.
    p = {name: [sigmoid(d["contain_a"] + d["contain_b"] * math.log(max(d["growth_raw"], v)))
                for d in draws] for name, v in FLOORS}
    pairs = {}
    for i, (n1, _) in enumerate(FLOORS):
        for n2, _ in FLOORS[i + 1:]:
            dp = [abs(x - y) for x, y in zip(p[n1], p[n2])]
            pairs[f"{n1} vs {n2}"] = {
                "expected_flipped_draws": sum(dp),
                "p_no_flip": math.prod(1.0 - x for x in dp),
                "max_abs_dp": max(dp) if dp else 0.0,
                "n_draws_with_dp_above_1e-3": sum(x > 1e-3 for x in dp),
            }
    out["flip_odds"] = pairs
    below = [(d, i) for i, d in enumerate(draws) if d["growth_raw"] < 1e-3]
    out["floor_binding_draws_at_1e-3"] = [
        {**d, **{f"p_{name}": p[name][i] for name, _ in FLOORS}} for d, i in below
    ]
    # Where the raw growth of the recorded draws sits.
    if len(growth) > 1:
        deciles = statistics.quantiles(growth, n=10)
        out["growth_quantiles"] = {"min": min(growth), "q10": deciles[0],
                                   "median": statistics.median(growth), "q90": deciles[-1]}
    else:
        out["growth_quantiles"] = None
    return out


def lead_of(reports, obs, ics_day50):
    curve, _, _ = e42.containment_curve(reports, obs)
    day50 = e42.crossing_day([row["day"] for row in obs], curve)
    return day50, (None if day50 is None or ics_day50 is None else ics_day50 - day50)


def ics209_leads():
    out = {}
    for fire in CALIBRATION_FIRES:
        out[fire] = {}
        for source, (folder, pattern) in SOURCES.items():
            reports = [json.loads((EXP / folder / pattern.format(fire=fire, seed=s)).read_text())
                       for s in SEEDS]
            obs = e42.posterior_trajectory(fire, reports)
            ics = e42.ics209_curve(fire, obs[0]["hours"])
            ics_day50 = e42.crossing_day(ics["days"], [x / 100.0 for x in ics["pct_contained"]])
            pooled_day50, pooled_lead = lead_of(reports, obs, ics_day50)
            per_seed = []
            for seed, r in zip(SEEDS, reports):
                d50, lead = lead_of([r], obs, ics_day50)
                live = in_run_day50(r)
                per_seed.append({
                    "seed": seed,
                    "binary_git": r.get("binary_git"),
                    "model_day50": d50,
                    "lead_days": lead,
                    "in_run_day50": live,
                    "in_run_lead_days": None if live is None or ics_day50 is None else ics_day50 - live,
                    "final_contained_fraction": r["scores"][-1]["contained_fraction"],
                })
            leads = [s["lead_days"] for s in per_seed if s["lead_days"] is not None]
            live = [s["in_run_lead_days"] for s in per_seed if s["in_run_lead_days"] is not None]
            finals = [s["final_contained_fraction"] for s in per_seed]
            out[fire][source] = {
                "ics_day50": ics_day50,
                "pooled_model_day50": pooled_day50,
                "pooled_lead_days": pooled_lead,
                "per_seed": per_seed,
                "mean_lead_days": statistics.mean(leads) if leads else None,
                "sd_lead_days": statistics.stdev(leads) if len(leads) > 1 else None,
                "n_seeds_with_lead": len(leads),
                "mean_in_run_lead_days": statistics.mean(live) if live else None,
                "sd_in_run_lead_days": statistics.stdev(live) if len(live) > 1 else None,
                "n_seeds_with_in_run_lead": len(live),
                "mean_final_contained_fraction": statistics.mean(finals),
                "sd_final_contained_fraction": statistics.stdev(finals),
            }
    return out


def main():
    out = {"sweep": sweep_table(), "draw_diagnostic": draw_diagnostic(), "ics209": ics209_leads()}
    (EXP / "exp49_analysis.json").write_text(json.dumps(out, indent=1))
    print("wrote exp49_analysis.json")
    for fire, s in out["sweep"].items():
        print(f"{fire}: 3 files identical {s['three_floor_files_byte_identical']}, "
              f"1e-4 == E44 seed0 {s['floor_1e-4_equals_e44_seed0_except_provenance_and_new_fields']}")
        for row in s["rows"]:
            print(f"  floor {row['floor']:g}: IoU {row['mean_consensus_iou']:.4f} Brier "
                  f"{row['mean_brier_ensemble']:.4f} final contained {row['final_contained_fraction']:.3f} "
                  f"day50 {row['in_run_day50']} a {row['contain_a_median']:.3f} b "
                  f"{row['contain_b_median']:.3f} dIoU {row['delta_iou_vs_e44_seed0']:+.4f}")
    d = out["draw_diagnostic"]
    if d:
        print("draw diagnostic:", json.dumps({k: v for k, v in d.items()
                                              if k != "floor_binding_draws_at_1e-3"}, indent=1))
        print("  binding draws at 1e-3:", len(d["floor_binding_draws_at_1e-3"]))
    fmt = lambda v: "never" if v is None else f"{v:+.1f}"  # noqa: E731
    for fire, by_source in out["ics209"].items():
        for source, r in by_source.items():
            seeds = " ".join(fmt(s["lead_days"]) for s in r["per_seed"])
            live = " ".join(fmt(s["in_run_lead_days"]) for s in r["per_seed"])
            print(f"{fire:14s} {source:22s} ICS day50 {r['ics_day50']:.1f} | pooled "
                  f"{fmt(r['pooled_lead_days'])} | per seed {seeds} mean {fmt(r['mean_lead_days'])} "
                  f"sd {r['sd_lead_days']:.1f} | in-run {live} mean {fmt(r['mean_in_run_lead_days'])} "
                  f"| final contained {r['mean_final_contained_fraction']:.3f}±"
                  f"{r['sd_final_contained_fraction']:.3f}")


if __name__ == "__main__":
    main()
