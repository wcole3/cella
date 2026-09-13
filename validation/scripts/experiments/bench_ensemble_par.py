#!/usr/bin/env python
"""Task 11 (2026-09-12): one-off performance study of ensemble stepping
parallelism — is stepping all 32 members concurrently (today) faster than
stepping one or two members at a time, each spread over every thread?

Every run is `wildfire_smc <scenario> 32 open <out.json>`, truncated to the
first `SMC_MAX_DAYS=5` observation days, on `Bear_2020` and `Ferguson_2018`,
under the default Bernoulli spread rule and the arrival rule
(`SMC_SPREAD=arrival SMC_WIND_LAW=rear_focus`). `CELLA_MEMBER_PAR`/
`CELLA_MIN_WORK` (added alongside this script — see
`cella_lib/src/threads.rs` and `cella_lib/src/explore/ensemble.rs`) are set
per the study's config grid; "unset" is a bonus row this script adds beyond
the pre-registered grid, because inspecting `Ensemble::step`'s own size
heuristic (`chunks_for_work(total*12) <= 1`) shows that at these two grids'
sizes and 16 threads, *today's actual behaviour already steps members
sequentially, each fully chunked* — not "all members concurrently" as the
brief's Design section assumed. See docs/performance.md and
task-11-report.md for the write-up; this file only produces the numbers.

Thread count is controlled by a `cella.properties` file in each run's
working directory (never the repo's own, so nothing here can affect any
other work on the machine): `THREADS16_DIR` for the 16-thread configs this
study's grid asks for, `THREADS8_DIR` for the runner-level 2x8 comparison's
8-thread arm. Scenario/output paths are always absolute, so the working
directory only ever matters for that properties-file lookup
(`cella_lib/src/threads.rs`'s `find_properties_file` walks up from cwd).

Usage: python3 bench_ensemble_par.py [--quick]
`--quick` runs 1 repeat instead of 2 and skips Ferguson, for a fast sanity
pass over the plumbing; the real study needs the default (no flags).

python3 bench_ensemble_par.py --runner-level runs the second question
instead: total wall time for 4 Bear/Bernoulli runs (seeds 0-3), four ways,
two repeats each (min reported, both kept):
  (a) 4 concurrent processes x 16 threads, knobs unset (today's engine
      default at 16 threads — the *shape* R6's `run_all(workers=4)` uses,
      see the module-level caveat about the repo's own `threads=4`
      `cella.properties` in task-11-report.md).
  (b) 4 concurrent processes x 16 threads x `CELLA_MEMBER_PAR=16` forced
      in each — today's process shape *plus* the single-run win from the
      main matrix. This is the arm that actually decides whether
      `r5_common.BASE_ENV` should gain the knob (fix round 1, 2026-09-12):
      if it beats (a) by >=20% total wall AND is bit-identical, the
      controller's ruling is to set it; otherwise document and leave
      `BASE_ENV` alone.
  (c) 1 process at a time x 16 threads, the best (member_par, min_work)
      the main matrix found for Bear.
  (d) 2 concurrent processes x 8 threads, 2 rounds, same best config.
Requires the main matrix to have already completed (reads `RESULTS_PATH`
to pick "best" for arms c/d), and merges its own findings into the same
results file under the `"runner_level"` key rather than overwriting it.
Also checks that all four arms, same seed, agree bit-for-bit (provenance
stripped) — the runner-level shape must not be able to change the answer
any more than the single-run knobs can.
"""
import argparse
import json
import os
import statistics
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
BIN = REPO / "cella_lib/target/release/examples/wildfire_smc"
SCENARIOS_DIR = REPO / "validation/data/scenarios"
RESULTS_PATH = REPO / "validation/results/experiments/bench_ensemble_par.json"

# A scratch tree well outside the repo so a `cella.properties` placed here
# can never be mistaken for (or interfere with) the repo's own.
SCRATCH = Path("/tmp/cella_bench_ensemble_par")
THREADS16_DIR = SCRATCH / "threads16"
THREADS8_DIR = SCRATCH / "threads8"
RUNS_DIR = SCRATCH / "runs"

MEMBERS = 32
MAX_DAYS = 5
SEED = 0
REPEATS = 2

FIRES = ["Bear_2020", "Ferguson_2018"]
RULES = {
    "bernoulli": {},
    "arrival": {"SMC_SPREAD": "arrival", "SMC_WIND_LAW": "rear_focus"},
}
# (label, member_par, min_work) — "unset" is the bonus row explained above;
# the other six are exactly the study's pre-registered grid.
CONFIGS = [
    ("unset", None, None),
    ("16_400k", 16, 400_000),
    ("16_50k", 16, 50_000),
    ("8_100k", 8, 100_000),
    ("4_50k", 4, 50_000),
    ("2_25k", 2, 25_000),
    ("1_12500", 1, 12_500),
]

# Fields that legitimately differ run to run (wall clock, and which binary
# build produced the file — irrelevant once we've confirmed it's HEAD).
PROVENANCE_FIELDS = ("wall_time_secs", "binary_git", "binary_built_utc")


def setup_dirs():
    for d in (THREADS16_DIR, THREADS8_DIR, RUNS_DIR):
        d.mkdir(parents=True, exist_ok=True)
    (THREADS16_DIR / "cella.properties").write_text("threads=16\n")
    (THREADS8_DIR / "cella.properties").write_text("threads=8\n")


def idle_snapshot():
    up = subprocess.run(["uptime"], capture_output=True, text=True).stdout.strip()
    pg = subprocess.run(["pgrep", "-f", "wildfire_smc"], capture_output=True, text=True).stdout.strip()
    return {"uptime": up, "other_wildfire_smc_pids": pg.split() if pg else []}


def strip_provenance(report):
    return {k: v for k, v in report.items() if k not in PROVENANCE_FIELDS}


def run_once(scenario_dir, rule_env, member_par, min_work, out_path, cwd, seed=SEED):
    env = dict(os.environ)
    env["SMC_MAX_DAYS"] = str(MAX_DAYS)
    env["SMC_SEED"] = str(seed)
    for k in ("SMC_SPREAD", "SMC_WIND_LAW", "CELLA_MEMBER_PAR", "CELLA_MIN_WORK"):
        env.pop(k, None)
    env.update(rule_env)
    if member_par is not None:
        env["CELLA_MEMBER_PAR"] = str(member_par)
    if min_work is not None:
        env["CELLA_MIN_WORK"] = str(min_work)
    idle = idle_snapshot()
    t0 = time.perf_counter()
    proc = subprocess.run(
        [str(BIN), str(scenario_dir), str(MEMBERS), "open", str(out_path)],
        cwd=str(cwd), env=env, capture_output=True, text=True,
    )
    elapsed = time.perf_counter() - t0
    if proc.returncode != 0:
        raise RuntimeError(f"wildfire_smc failed ({proc.returncode}): {proc.stderr[-4000:]}")
    report = json.loads(out_path.read_text())
    return {
        "elapsed_secs": elapsed,
        "idle_before": idle,
        "binary_git": report.get("binary_git"),
        "binary_built_utc": report.get("binary_built_utc"),
    }, report


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--quick", action="store_true")
    args = ap.parse_args()
    repeats = 1 if args.quick else REPEATS
    fires = ["Bear_2020"] if args.quick else FIRES

    setup_dirs()
    matrix = []
    determinism_groups = {}  # (fire, rule) -> {config_label: stripped_report}

    for fire in fires:
        sdir = SCENARIOS_DIR / fire
        for rule, renv in RULES.items():
            key = f"{fire}|{rule}"
            determinism_groups[key] = {}
            for label, mp, mw in CONFIGS:
                reps = []
                stripped = None
                for rep in range(repeats):
                    out_path = RUNS_DIR / f"{fire}_{rule}_{label}_rep{rep}.json"
                    info, report = run_once(sdir, renv, mp, mw, out_path, cwd=THREADS16_DIR)
                    info["out_path"] = str(out_path)
                    reps.append(info)
                    s = strip_provenance(report)
                    if stripped is None:
                        stripped = s
                    elif s != stripped:
                        info["WARNING"] = "differs from this config's own rep 0 after stripping provenance"
                    print(
                        f"{fire:14s} {rule:10s} {label:8s} rep{rep} "
                        f"{info['elapsed_secs']:7.2f}s git={info['binary_git']}",
                        flush=True,
                    )
                determinism_groups[key][label] = stripped
                times = [r["elapsed_secs"] for r in reps]
                matrix.append({
                    "fire": fire, "rule": rule, "config": label,
                    "member_par": mp, "min_work": mw,
                    "reps": reps, "elapsed_secs": times,
                    "min_elapsed_secs": min(times),
                })
                # Flush progress after every cell so a killed/backgrounded
                # run leaves a readable partial result.
                RESULTS_PATH.parent.mkdir(parents=True, exist_ok=True)
                RESULTS_PATH.write_text(json.dumps({"matrix": matrix, "status": "in_progress"}, indent=1))

    # Determinism check: within each (fire, rule), every config's
    # provenance-stripped report must equal every other's.
    determinism = {}
    for key, by_label in determinism_groups.items():
        labels = list(by_label.keys())
        base_label, base = labels[0], by_label[labels[0]]
        mismatches = [lbl for lbl in labels[1:] if by_label[lbl] != base]
        determinism[key] = {
            "reference_config": base_label,
            "all_bit_identical": len(mismatches) == 0,
            "mismatched_configs": mismatches,
        }

    RESULTS_PATH.write_text(json.dumps({
        "matrix": matrix,
        "determinism": determinism,
        "members": MEMBERS,
        "max_days": MAX_DAYS,
        "seed": SEED,
        "repeats": repeats,
        "status": "complete",
    }, indent=1))
    print(f"wrote {RESULTS_PATH}")


def pick_best_bear_config(matrix):
    """The (member_par, min_work) from the study's official six-config grid
    (i.e. not the bonus "unset" row) with the lowest combined Bear
    Bernoulli + Bear arrival min-of-2-reps wall time."""
    totals = {}
    for row in matrix:
        if row["fire"] != "Bear_2020" or row["config"] == "unset":
            continue
        totals.setdefault(row["config"], {"member_par": row["member_par"], "min_work": row["min_work"], "secs": 0.0})
        totals[row["config"]]["secs"] += row["min_elapsed_secs"]
    best_label = min(totals, key=lambda k: totals[k]["secs"])
    return best_label, totals[best_label]


# Fix round 1 (2026-09-12 controller review): these three main-matrix cells
# disagreed by >10% between their two repeats (Ferguson/bernoulli/16_50k
# 10.6%, Ferguson/arrival/2_25k 10.9%, Ferguson/arrival/1_12500 13.0%),
# including the cell behind the "(16,50k) beats (16,400k) by 8.4%" claim in
# docs/performance.md §9. `--extra-rep` adds a third repeat to exactly these
# cells so the min (and the noise band) rests on 3 points, not 2.
NOISY_CELLS = [
    ("Ferguson_2018", "bernoulli", "16_50k"),
    ("Ferguson_2018", "arrival", "2_25k"),
    ("Ferguson_2018", "arrival", "1_12500"),
]


def run_extra_rep():
    if not RESULTS_PATH.exists():
        sys.exit(f"{RESULTS_PATH} does not exist yet — run the main matrix first")
    existing = json.loads(RESULTS_PATH.read_text())
    setup_dirs()
    by_key = {(r["fire"], r["rule"], r["config"]): r for r in existing["matrix"]}
    for fire, rule, label in NOISY_CELLS:
        row = by_key[(fire, rule, label)]
        rep_n = len(row["reps"])
        renv = RULES[rule]
        out_path = RUNS_DIR / f"{fire}_{rule}_{label}_rep{rep_n}.json"
        info, report = run_once(SCENARIOS_DIR / fire, renv, row["member_par"], row["min_work"], out_path, cwd=THREADS16_DIR)
        info["out_path"] = str(out_path)
        row["reps"].append(info)
        row["elapsed_secs"].append(info["elapsed_secs"])
        row["min_elapsed_secs"] = min(row["elapsed_secs"])
        spread_pct = (max(row["elapsed_secs"]) - min(row["elapsed_secs"])) / min(row["elapsed_secs"]) * 100.0
        row["repeat_spread_pct_after_extra_rep"] = spread_pct
        print(f"{fire:14s} {rule:10s} {label:8s} rep{rep_n} {info['elapsed_secs']:7.2f}s "
              f"(now {len(row['elapsed_secs'])} reps, spread {spread_pct:.1f}%)", flush=True)
        RESULTS_PATH.write_text(json.dumps(existing, indent=1))
    print(f"wrote extra reps into {RESULTS_PATH}")


RUNNER_LEVEL_REPEATS = 2


def run_runner_level():
    if not RESULTS_PATH.exists():
        sys.exit(f"{RESULTS_PATH} does not exist yet — run the main matrix first")
    existing = json.loads(RESULTS_PATH.read_text())
    if existing.get("status") != "complete" or not existing.get("matrix"):
        sys.exit("main matrix is not complete yet — run without --runner-level first")
    best_label, best = pick_best_bear_config(existing["matrix"])
    print(f"best Bear config from the main matrix: {best_label} "
          f"(member_par={best['member_par']}, min_work={best['min_work']}, "
          f"combined min secs={best['secs']:.2f})")

    setup_dirs()
    sdir = SCENARIOS_DIR / "Bear_2020"
    seeds = [0, 1, 2, 3]

    def one(seed, member_par, min_work, cwd, tag, rep):
        out_path = RUNS_DIR / f"runner_level_{tag}_rep{rep}_seed{seed}.json"
        info, report = run_once(sdir, RULES["bernoulli"], member_par, min_work, out_path, cwd=cwd, seed=seed)
        return info, strip_provenance(report)

    def run_arm(tag, member_par, min_work, cwd, workers):
        """`RUNNER_LEVEL_REPEATS` rounds of `workers`-way concurrency over
        `seeds`; returns (rounds, min_total_wall_secs, {seed: stripped_report}
        from the first round, for the cross-arm determinism check)."""
        rounds = []
        by_seed = {}
        for rep in range(RUNNER_LEVEL_REPEATS):
            t0 = time.perf_counter()
            with ThreadPoolExecutor(max_workers=workers) as ex:
                results = list(ex.map(
                    lambda s: one(s, member_par, min_work, cwd, tag, rep), seeds))
            total = time.perf_counter() - t0
            per_run = [r[0] for r in results]
            rounds.append({"total_wall_secs": total, "per_run": per_run})
            if rep == 0:
                by_seed = dict(zip(seeds, (r[1] for r in results)))
            print(f"  {tag:18s} rep{rep} total {total:6.2f}s", flush=True)
        min_total = min(r["total_wall_secs"] for r in rounds)
        return rounds, min_total, by_seed

    idle = idle_snapshot()

    # (a) 4x16: 4 concurrent processes, 16 threads each, today's engine
    # default (knobs unset) — the shape of today's run_all(workers=4).
    print("arm (a) 4x16 oversubscribed, knobs unset:", flush=True)
    a_rounds, a_min, a_by_seed = run_arm("4x16", None, None, THREADS16_DIR, 4)

    # (b) 4x16 + CELLA_MEMBER_PAR=16: same process shape as (a), plus the
    # single-run win — the arm that decides the BASE_ENV question (fix
    # round 1, 2026-09-12 controller review).
    print("arm (b) 4x16 + CELLA_MEMBER_PAR=16:", flush=True)
    b_rounds, b_min, b_by_seed = run_arm("4x16_member_par", 16, None, THREADS16_DIR, 4)

    # (c) 1x16: the same 4 runs, one at a time, 16 threads, best config.
    print("arm (c) 1x16 sequential, best config:", flush=True)
    c_rounds, c_min, c_by_seed = run_arm("1x16", best["member_par"], best["min_work"], THREADS16_DIR, 1)

    # (d) 2x8: two concurrent processes at a time (two rounds per repeat), 8
    # threads each, best config.
    print("arm (d) 2x8, best config:", flush=True)
    d_rounds, d_min, d_by_seed = run_arm("2x8", best["member_par"], best["min_work"], THREADS8_DIR, 2)

    # Cross-arm determinism: same seed, four different concurrency shapes
    # (today's default, forced full member-parallelism, sequential, 2x8) —
    # all must agree bit-for-bit once provenance is stripped.
    cross_arm_determinism = {}
    for seed in seeds:
        by_arm = {"4x16": a_by_seed[seed], "4x16_member_par": b_by_seed[seed],
                  "1x16": c_by_seed[seed], "2x8": d_by_seed[seed]}
        arms = list(by_arm.keys())
        base = by_arm[arms[0]]
        mismatches = [a for a in arms[1:] if by_arm[a] != base]
        cross_arm_determinism[str(seed)] = {"all_bit_identical": len(mismatches) == 0, "mismatched_arms": mismatches}

    speedup_b_vs_a = (a_min - b_min) / a_min * 100.0
    bit_identical = all(v["all_bit_identical"] for v in cross_arm_determinism.values())

    runner_level = {
        "idle_before": idle,
        "best_bear_config": {"label": best_label, "member_par": best["member_par"], "min_work": best["min_work"]},
        "seeds": seeds,
        "repeats": RUNNER_LEVEL_REPEATS,
        "4x16_oversubscribed": {"rounds": a_rounds, "min_total_wall_secs": a_min, "config": "unset (today's default)"},
        "4x16_member_par": {"rounds": b_rounds, "min_total_wall_secs": b_min, "config": "CELLA_MEMBER_PAR=16"},
        "1x16_sequential": {"rounds": c_rounds, "min_total_wall_secs": c_min, "config": best_label},
        "2x8": {"rounds": d_rounds, "min_total_wall_secs": d_min, "config": best_label},
        "cross_arm_determinism": cross_arm_determinism,
        "cross_arm_bit_identical": bit_identical,
        "member_par_arm_speedup_vs_today_pct": speedup_b_vs_a,
        "member_par_arm_meets_20pct_bar": bool(speedup_b_vs_a >= 20.0 and bit_identical),
    }
    existing["runner_level"] = runner_level
    RESULTS_PATH.write_text(json.dumps(existing, indent=1))
    print(f"4x16 (unset): {a_min:.2f}s | 4x16+member_par: {b_min:.2f}s ({speedup_b_vs_a:+.1f}%) | "
          f"1x16 sequential: {c_min:.2f}s | 2x8: {d_min:.2f}s | bit-identical: {bit_identical}")
    print(f"wrote runner_level into {RESULTS_PATH}")


if __name__ == "__main__":
    ap = argparse.ArgumentParser(add_help=False)
    ap.add_argument("--runner-level", action="store_true")
    ap.add_argument("--extra-rep", action="store_true")
    known, _ = ap.parse_known_args()
    if known.runner_level:
        run_runner_level()
    elif known.extra_rep:
        run_extra_rep()
    else:
        main()
