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
instead: total wall time for 4 Bear/Bernoulli runs (seeds 0-3, `unset`
config, i.e. today's engine default) as 4 concurrent processes at 16
threads each (today's `run_all(workers=4)` shape) vs the same 4 runs one
at a time at 16 threads using the best (member_par, min_work) the main
matrix found for Bear vs 2 concurrent at a time at 8 threads each, same
best config. Requires the main matrix to have already completed (reads
`RESULTS_PATH` to pick "best"), and merges its own findings into the same
results file under the `"runner_level"` key rather than overwriting it.
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

    def one(seed, member_par, min_work, cwd, tag):
        out_path = RUNS_DIR / f"runner_level_{tag}_seed{seed}.json"
        info, _report = run_once(sdir, RULES["bernoulli"], member_par, min_work, out_path, cwd=cwd, seed=seed)
        return info

    idle = idle_snapshot()

    # (a) 4x16: 4 concurrent processes, 16 threads each, today's engine
    # default (knobs unset) — the shape of today's run_all(workers=4).
    t0 = time.perf_counter()
    with ThreadPoolExecutor(max_workers=4) as ex:
        four_by_16 = list(ex.map(lambda s: one(s, None, None, THREADS16_DIR, "4x16"), seeds))
    four_by_16_total = time.perf_counter() - t0

    # (b) 1x16: the same 4 runs, one at a time, 16 threads, best config.
    t0 = time.perf_counter()
    one_by_16 = [one(s, best["member_par"], best["min_work"], THREADS16_DIR, "1x16") for s in seeds]
    one_by_16_total = time.perf_counter() - t0

    # (c) 2x8: two concurrent processes at a time (two rounds), 8 threads
    # each, best config.
    t0 = time.perf_counter()
    with ThreadPoolExecutor(max_workers=2) as ex:
        two_by_8 = list(ex.map(lambda s: one(s, best["member_par"], best["min_work"], THREADS8_DIR, "2x8"), seeds))
    two_by_8_total = time.perf_counter() - t0

    runner_level = {
        "idle_before": idle,
        "best_bear_config": {"label": best_label, "member_par": best["member_par"], "min_work": best["min_work"]},
        "seeds": seeds,
        "4x16_oversubscribed": {"total_wall_secs": four_by_16_total, "per_run": four_by_16, "config": "unset (today's default)"},
        "1x16_sequential": {"total_wall_secs": one_by_16_total, "per_run": one_by_16, "config": best_label},
        "2x8": {"total_wall_secs": two_by_8_total, "per_run": two_by_8, "config": best_label},
    }
    existing["runner_level"] = runner_level
    RESULTS_PATH.write_text(json.dumps(existing, indent=1))
    print(json.dumps({k: v for k, v in runner_level.items() if k.endswith("_secs") or "total_wall_secs" in str(v)}, indent=1))
    print(f"4x16 oversubscribed: {four_by_16_total:.2f}s | 1x16 sequential: {one_by_16_total:.2f}s | 2x8: {two_by_8_total:.2f}s")
    print(f"wrote runner_level into {RESULTS_PATH}")


if __name__ == "__main__":
    ap = argparse.ArgumentParser(add_help=False)
    ap.add_argument("--runner-level", action="store_true")
    known, _ = ap.parse_known_args()
    if known.runner_level:
        run_runner_level()
    else:
        main()
