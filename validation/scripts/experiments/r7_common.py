#!/usr/bin/env python
"""Shared runner for the Round 7 experiments (E44-E49), pre-registered in
`validation/TEST_PLAN.md` v1.9. Built on `r5_common`/`r6_common` (imported,
not copied) plus the shared-machine and provenance rules Round 7's plan
(`docs/superpowers/plans/round-7-experiments.md`, "Global constraints")
binds on every task:

- **2 workers by default** (`--workers`, every `exp_r7_e*.py` script) and
  every child process runs under `nice -n 10` -- other people's jobs share
  this box.
- **Load gate.** `uptime`'s 1-minute load average is checked before a
  batch starts; above 8 it sleeps 10 minutes and re-checks, logging each
  wait. The load at batch start and finish is written into the batch's
  summary JSON (never used to compare wall time across batches -- the
  plan says not to; it's provenance, not a benchmark).
- **binary_git check.** `wildfire_smc` has no `--version` flag; the
  cheapest way to read its provenance stamp is to run it once in `nulls`
  mode on Bear (`r6_common.run_nulls`, no ensemble members, niced like
  every other child, landed in a real OS temp directory rather than
  `validation/results/experiments/`) and read the `binary_git` field the
  report already carries (see
  `cella_lib/examples/wildfire_smc/report.rs::provenance`, baked in at
  compile time from `git rev-parse --short HEAD`, suffixed `-dirty` if
  the tree was dirty at build time). `run_all` checks the load gate
  first, then refuses to start a batch if that stamp doesn't match the
  *current* `git rev-parse --short HEAD`, if it carries the `-dirty`
  suffix, or if `git status --porcelain` shows *tracked* changes
  (untracked paths -- e.g. `docs/superpowers/`, `.superpowers/` -- are
  fine and ignored).
- **ARM_B preset** -- the E30b Arm B configuration (validated as a
  one-seed pilot in `exp_r6_arrival_x4_pilot.py` /
  `48-e30b-uncapped-clock-direction-gene-pilot.md`): arrival kernel,
  rear-focus wind law, 4x clock, the widened `arrival_x4` prior, and the
  learned wind-rotation gene at +-90 degrees.
- **Summariser** -- per-fire mean/sd table with a delta-in-sd verdict
  column, worded exactly as `48-e30b-...md`'s tables ("tie" / "beyond 1
  sd (gain|loss)" / "**beyond 2 sd (gain|loss)**").

`--dry-run` in every `exp_r7_e*.py` script must work with nothing built
or running: it only prints jobs and the exact command + env each would
run, via `dry_run()` below, and never calls `run_all` (which is the only
thing that touches the network^H^H^H^H^H the binary).
"""
import argparse
import json
import os
import statistics
import subprocess
import tempfile
import time
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import r5_common as _r5
import r6_common as _r6
from r5_common import BIN, EXP, FIRES, HOLDOUT, VAL

__all__ = [
    "BIN", "EXP", "FIRES", "HOLDOUT", "VAL", "REPO", "PRIOR", "ARM_B", "NICE_PREFIX",
    "arg_parser", "command_for", "dry_run", "run", "run_all",
    "check_binary_git", "load_1min", "wait_for_load",
    "fire_stats", "verdict", "summary_table", "e33_baseline",
]

REPO = Path(__file__).resolve().parents[3]
PRIOR = VAL / "scripts" / "experiments" / "priors" / "arrival_x4.json"

# Prepended to every child's argv (r5_common.run()'s / r6_common.run_nulls()'s
# argv_prefix) so every wildfire_smc process this module launches -- batch
# runs and the binary_git diagnostic alike -- is niced. Shared-machine rule.
NICE_PREFIX = ["nice", "-n", "10"]

# The E30b Arm B preset (E30b, exp_r6_arrival_x4_pilot.py's ARM_B_ENV),
# named per the Round 7 plan's Task 2 spec: arrival kernel, rear-focus
# wind law, 4x clock, the arrival_x4 prior, +-90 degree wind-rotation gene.
ARM_B = {
    "SMC_SPREAD": "arrival",
    "SMC_WIND_LAW": "rear_focus",
    "SMC_STEPS_SCALE": "4",
    "SMC_PRIOR": str(PRIOR),
    "SMC_WIND_ROT_GENE": "90",
}

# Five-seed E33 baseline (round-6.md, "Configuration after this round"):
# Bear 0.479, Brattain 0.416, Buck 0.590, Chimney 0.434, Ferguson 0.344,
# Pier 0.535. sd per the Round 7 plan's Global constraints (matches
# statistics.stdev of exp33_noise.json's five seeds per fire, see
# e33_baseline() below): Bear 0.015, Brattain 0.004, Buck 0.039,
# Chimney 0.012, Ferguson 0.007, Pier 0.003. Used only if
# exp33_noise.json is not present on disk (it is gitignored).
_E33_FALLBACK = {
    "Bear_2020": (0.479, 0.015),
    "Brattain_2020": (0.416, 0.004),
    "Buck_2017": (0.590, 0.039),
    "Chimney_2016": (0.434, 0.012),
    "Ferguson_2018": (0.344, 0.007),
    "Pier_2017": (0.535, 0.003),
}


# --------------------------------------------------------------------------
# CLI / dry-run
# --------------------------------------------------------------------------

def arg_parser(description):
    """Every exp_r7_e*.py script's argument parser: `--dry-run` (print
    jobs and commands, launch nothing) and `--workers` (shared-machine
    default: 2)."""
    p = argparse.ArgumentParser(description=description)
    p.add_argument("--dry-run", action="store_true",
                    help="print every job's command and env; launch nothing")
    p.add_argument("--workers", type=int, default=2,
                    help="max parallel wildfire_smc processes (default 2, shared machine)")
    return p


def command_for(out_dir, fire, label, env, members, mode):
    """The exact argv `run()` launches for one job, under `nice -n 10`
    (`NICE_PREFIX`) -- printed by dry_run() for display only; run() itself
    gets its niceness from r5_common.run()'s own argv_prefix, not from
    this function, but the two are the same prefix so a dry run's output
    is never a lie about what the real run would do."""
    rep = out_dir / f"{fire}_{label}.json"
    return [*NICE_PREFIX, str(BIN), str(VAL / "data" / "scenarios" / fire), str(members), mode, str(rep)]


def dry_run(jobs, out_json):
    """jobs: list of (fire, label, env, members, mode). Prints one line
    per job plus its full env and argv; launches nothing."""
    out_dir = EXP / out_json.replace(".json", "")
    print(f"[dry-run] {len(jobs)} job(s) -> {out_json} (raw reports under {out_dir})")
    for fire, label, env, members, mode in jobs:
        full_env = {**_r5.BASE_ENV, **env}
        env_str = " ".join(f"{k}={v}" for k, v in full_env.items())
        argv = command_for(out_dir, fire, label, env, members, mode)
        print(f"  {fire:14s} {label:24s} mode={mode:6s} members={members:3d}")
        print(f"    env: {env_str}")
        print(f"    cmd: {' '.join(argv)}")


# --------------------------------------------------------------------------
# Provenance / shared-machine gates
# --------------------------------------------------------------------------

def _git(*args):
    return subprocess.run(["git", *args], cwd=REPO, capture_output=True, text=True, check=True).stdout


def check_binary_git():
    """Refuse to run if the binary wasn't built from the current, clean
    HEAD. Reads `binary_git` off a one-run `nulls`-mode report on Bear
    (cheapest fire, no ensemble members -- there is no `--version` flag),
    niced like every other child (`NICE_PREFIX`) and landed in a real OS
    temp directory (never under `validation/results/experiments/` -- this
    is a provenance probe, not a batch result), and compares it against
    `git rev-parse --short HEAD`; also refuses if `git status --porcelain`
    shows *tracked* changes (untracked paths are fine -- e.g. this
    campaign's own doc scratch space)."""
    head = _git("rev-parse", "--short", "HEAD").strip()
    dirty_tracked = [
        line for line in _git("status", "--porcelain").splitlines()
        if not line.startswith("??")
    ]
    if dirty_tracked:
        raise RuntimeError(
            "refusing to run: tracked working-tree changes present:\n" + "\n".join(dirty_tracked)
        )
    with tempfile.TemporaryDirectory(prefix="r7_binary_check_") as tmp:
        report = _r6.run_nulls(Path(tmp), "Bear_2020", argv_prefix=NICE_PREFIX)
    stamp = report.get("binary_git", "unknown")
    stamp_sha = stamp[: -len("-dirty")] if stamp.endswith("-dirty") else stamp
    if stamp == "unknown" or stamp.endswith("-dirty") or stamp_sha != head:
        raise RuntimeError(
            f"refusing to run: wildfire_smc's binary_git={stamp!r} does not match a clean "
            f"HEAD={head}; rebuild wildfire_smc (cargo build --release --example wildfire_smc)"
        )
    return stamp, head


def load_1min():
    """Parse `uptime`'s 1-minute load average."""
    out = subprocess.run(["uptime"], capture_output=True, text=True, check=True).stdout
    la = out.split("load average:")[-1]
    return float(la.split(",")[0].strip())


def wait_for_load(threshold=8.0, sleep_s=600):
    """Block until the 1-minute load average is at or below `threshold`,
    sleeping and re-checking every `sleep_s` seconds; logs each wait.
    Returns the load average it finally proceeds at."""
    load = load_1min()
    while load > threshold:
        print(f"[r7] load {load:.2f} > {threshold}, waiting {sleep_s}s before re-checking...", flush=True)
        time.sleep(sleep_s)
        load = load_1min()
    return load


# --------------------------------------------------------------------------
# Run / batch
# --------------------------------------------------------------------------

def run(out_dir, fire, label, env, members=32, mode="assim"):
    """`r5_common.run()`, launched under `nice -n 10` (`NICE_PREFIX`) so
    batches share the box politely -- everything else (row shape, JSON
    field meanings, the printed one-line summary) is `r5_common.run()`
    itself; this is a thin wrapper, not a copy, so the two never drift
    apart. `r5_common.run()` unconditionally parses an `assim`-shaped
    report (`r["scores"][-1]`, `r["mean_consensus_iou"]`, ...), which a
    `map`-mode `MapReport` (`cella_lib/examples/wildfire_smc/modes/
    map.rs`) does not have at all (no `scores` key) -- so `mode="map"`
    is routed to `_run_map` below instead, the same archive-parsing shape
    every Round 6 illuminate script used by hand (e.g.
    `exp_r6_arrival_illuminate.py`)."""
    if mode == "map":
        return _run_map(out_dir, fire, label, env, members)
    return _r5.run(out_dir, fire, label, env, members, mode, argv_prefix=NICE_PREFIX)


def _run_map(out_dir, fire, label, env, members=32):
    """One `wildfire_smc map` run (MAP-Elites illumination), niced like
    every other Round 7 child. Reports land in
    `out_dir/<fire>_<label>.json`. Row shape mirrors what the Round 6
    illuminate scripts pulled out of a `MapReport` by hand: elite count
    and coverage off `archive.stats`, `labels`/`ranges` off the archive,
    the observed fire's own (hours, growth, elongation) series, plus two
    convenience fields the E37b-style write-up tables need -- the
    model's maximum elongation at any size, and at a size at least as
    big as the real fire's day-5 growth (same elite filter
    `wildfire_smc replay` uses), `None` if no elite reaches that growth
    at all (nothing to take a maximum of)."""
    out_dir.mkdir(parents=True, exist_ok=True)
    rep = out_dir / f"{fire}_{label}.json"
    full_env = {**os.environ, **_r5.BASE_ENV, **env}
    argv = [*NICE_PREFIX, str(BIN), str(VAL / "data" / "scenarios" / fire), str(members), "map", str(rep)]
    subprocess.run(argv, check=True, capture_output=True, env=full_env)
    r = json.loads(rep.read_text())
    a = r["archive"]
    growths = [e["descriptor"][0] for e in a["elites"]]
    elongs = [e["descriptor"][1] for e in a["elites"]]
    observed = r.get("observed", [])
    obs_growth_day5 = observed[-1][1] if observed else float("nan")
    obs_elong_day5 = observed[-1][2] if observed else float("nan")
    at_size = [el for g, el in zip(growths, elongs) if g >= obs_growth_day5]
    row = {
        "fire": fire, "config": label, "mode": "map", "members": members,
        "holdout": fire in HOLDOUT,
        "binary_git": r.get("binary_git", "unknown"), "binary_built_utc": r.get("binary_built_utc", "unknown"),
        "elites": a["stats"]["elites"], "coverage": a["stats"]["coverage"],
        "labels": a["labels"], "ranges": a["ranges"],
        "max_elongation_any_size": max(elongs) if elongs else None,
        "max_elongation_at_size": max(at_size) if at_size else None,
        "observed_growth_day5": obs_growth_day5, "observed_elongation_day5": obs_elong_day5,
        "observed": observed,
    }
    at_size_str = "None" if row["max_elongation_at_size"] is None else f"{row['max_elongation_at_size']:.2f}"
    any_size_str = "None" if row["max_elongation_any_size"] is None else f"{row['max_elongation_any_size']:.2f}"
    print(f"{fire:14s} {label:24s} map elites {row['elites']:3d} coverage {row['coverage']:.2f} "
          f"max elong any {any_size_str} at-size {at_size_str} "
          f"(observed g={obs_growth_day5:.3f} e={obs_elong_day5:.2f})", flush=True)
    return row


def run_all(jobs, out_json, workers=2, load_gate=8.0, gate_sleep=600, skip_binary_check=False):
    """jobs: list of (fire, label, env, members, mode). Runs the shared-
    machine gates in order -- the load gate first (so the box isn't
    touched at all, not even by the binary_git diagnostic, while it's
    over threshold), then binary_git + dirty tree -- before launching
    anything, then the batch itself at `workers` parallelism (default 2).
    Writes rows to `out_json` and a `<out_json>` sibling `..._summary.json`
    carrying `binary_git`, `workers`, wall time and the 1-minute load
    average logged at batch start and finish."""
    out_dir = EXP / out_json.replace(".json", "")
    load_start = wait_for_load(load_gate, gate_sleep)
    if not skip_binary_check:
        check_binary_git()
    print(f"[r7] batch start: {len(jobs)} job(s), workers={workers}, load(1m)={load_start:.2f}", flush=True)
    t0 = time.time()
    with ThreadPoolExecutor(max_workers=workers) as ex:
        rows = list(ex.map(lambda j: run(out_dir, j[0], j[1], j[2], j[3], j[4]), jobs))
    wall_s = time.time() - t0
    load_end = load_1min()
    (EXP / out_json).write_text(json.dumps(rows, indent=1))
    summary = {
        "out_json": out_json, "n_jobs": len(jobs), "workers": workers,
        "wall_s": round(wall_s, 1),
        "load_1min_start": load_start, "load_1min_end": load_end,
        "binary_git": rows[0]["binary_git"] if rows else "unknown",
    }
    (EXP / out_json.replace(".json", "_summary.json")).write_text(json.dumps(summary, indent=1))
    print(f"[r7] batch done: {len(jobs)} job(s), {wall_s:.1f}s, load(1m) {load_start:.2f} -> {load_end:.2f}",
          flush=True)
    return rows, summary


# --------------------------------------------------------------------------
# Summariser
# --------------------------------------------------------------------------

def fire_stats(rows, value_key="mean_consensus_iou"):
    """{fire: (mean, sd, n)} from a list of row dicts (one row per
    fire x seed, e.g. one arm's rows out of run_all)."""
    by_fire = defaultdict(list)
    for row in rows:
        by_fire[row["fire"]].append(row[value_key])
    out = {}
    for fire, vals in by_fire.items():
        mean = statistics.mean(vals)
        sd = statistics.stdev(vals) if len(vals) > 1 else 0.0
        out[fire] = (mean, sd, len(vals))
    return out


def e33_baseline(value_key="mean_consensus_iou"):
    """{fire: (mean, sd)}, the E33 five-seed noise floor every Round 7 arm
    is judged against until E44 (Task 4) produces Arm B's own sd. Loaded
    from exp33_noise.json on disk if present (gitignored, so not always
    there), else the hard-coded _E33_FALLBACK above."""
    path = EXP / "exp33_noise.json"
    if path.exists():
        rows = json.loads(path.read_text())
        stats = fire_stats(rows, value_key)
        return {fire: (mean, sd) for fire, (mean, sd, _n) in stats.items()}
    return dict(_E33_FALLBACK)


def verdict(delta_sd):
    """'tie' / 'beyond 1 sd (gain|loss)' / '**beyond 2 sd (gain|loss)**' --
    exactly the wording 48-e30b-uncapped-clock-direction-gene-pilot.md
    uses in its Delta (sd) columns. 'Tie' = within 1 sd; 'beyond 2 sd' is
    the bar for a claimed gain or loss (Round 7 plan, Global constraints)."""
    a = abs(delta_sd)
    if a < 1.0:
        return "tie"
    direction = "gain" if delta_sd > 0 else "loss"
    label = f"beyond {2 if a >= 2.0 else 1} sd ({direction})"
    return f"**{label}**" if a >= 2.0 else label


def summary_table(arm_stats_by_label, baseline, fires=FIRES):
    """arm_stats_by_label: {label: {fire: (mean, sd, n)}} (fire_stats() on
    each arm's rows). baseline: {fire: (mean, sd)} (e33_baseline(), or
    Arm B's own sd once E44 has run). Markdown table: Fire | baseline
    mean | baseline sd | <label> mean | <label> sd | Delta <label> (sd) |
    verdict <label>, one column block per arm, same shape and wording as
    48-e30b-...md's per-fire tables."""
    labels = list(arm_stats_by_label)
    header = "| Fire | Baseline mean | Baseline sd |" + "".join(
        f" {l} mean | {l} sd | Delta {l} (sd) | verdict {l} |" for l in labels
    )
    sep = "|---|---|---|" + "---|---|---|---|" * len(labels)
    lines = [header, sep]
    for fire in fires:
        b_mean, b_sd = baseline[fire]
        name = fire.split("_")[0] + ("*" if fire in HOLDOUT else "")
        cells = [name, f"{b_mean:.3f}", f"{b_sd:.3f}"]
        for label in labels:
            mean, sd, _n = arm_stats_by_label[label].get(fire, (float("nan"), float("nan"), 0))
            delta = mean - b_mean
            if b_sd:
                delta_str = f"{delta:+.3f} ({delta / b_sd:+.2f} sd)"
                v = verdict(delta / b_sd)
            else:
                # A zero baseline sd makes "sd units" meaningless -- say so
                # rather than reporting a false "beyond 2 sd" from a delta
                # divided by zero read as +-inf.
                delta_str = f"{delta:+.3f} (sd=0)"
                v = "undefined (zero sd)"
            cells += [f"{mean:.3f}", f"{sd:.3f}", delta_str, v]
        lines.append("| " + " | ".join(cells) + " |")
    return "\n".join(lines)
