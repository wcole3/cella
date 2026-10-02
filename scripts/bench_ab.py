#!/usr/bin/env python3
"""Interleaved A/B timing of the cella_lib long_suite benches (Python 3 stdlib only).

Why this exists
---------------
Timing on a shared machine drifts: the CPU clock, other processes and the WSL2
host all change over minutes. If you time build A for five minutes and then
build B for five minutes, any drift shows up as a fake difference between them.
"Interleaving" means running A, B, A, B, ... in turns, so both builds sample
the same stretches of machine weather and the drift cancels out.

What it does
------------
1. Builds the `long_suite` test binary for git ref A (in a temporary
   `git worktree`) and for the current working tree B (uncommitted edits
   included), locating each binary through `cargo --message-format=json`.
2. Runs A, B, A, B, ... for ROUNDS rounds, each with `CELLA_BENCH=1` and the
   name filter, and parses the per-run times the harness prints.
3. Reports, per bench: min and median of each side, the change in min and in
   median, how many outlier runs each side had, and a Mann-Whitney U p-value.
   By default the samples are one median per round per side ("--unit rounds").
   Runs inside one process share the same moment of machine weather, so they
   are correlated, and pooling them gives p-values that are far too small.
   "--unit runs" pools every run and is kept only as an optimistic screen.
4. Verdict: "significant" only if ALL hold: p < 0.01, |change in median| > 2 %,
   |change in min| > 2 %, and the min and median changes have the same sign.
   (Noise mostly inflates medians, not mins, so a real change must show in both.)
   Rows that fail say which gate failed: p, med, min, sign.

Mann-Whitney U in plain words: pool all runs of A and B, sort them, and give
each a rank (1 = fastest). If B is really slower, B's runs get the high ranks.
U measures how lopsided that is. It looks only at ordering, not at the actual
values, so one wild run cannot fake a result. p is the chance of ranks at least
this lopsided if A and B were really the same; small p = unlikely to be luck.
It is computed here with the normal approximation (tie-corrected, with
continuity correction), no scipy needed.

Usage
-----
    python3 scripts/bench_ab.py --a <git ref> --filter <name pattern> [--rounds 8]
    make bench-ab A=<git ref> FILTER=<pattern>

`--filter` is a libtest name filter (substring match on the test function
name, e.g. `stress_2d_three_state`). It may be given several times or contain
spaces; libtest ORs them. Run from anywhere inside the repo.

Why 8 rounds: in rounds mode there are only `rounds` samples per side. With 5 per
side the smallest possible p is about 0.012, so p < 0.01 can never be reached.
6 rounds is the minimum (best p about 0.005); the script warns below that.

Warm-up check: if A's long_suite.rs has no CELLA_BENCH_WARMUP, A's first run in
each process is cold and slow, which biases the result toward "B faster". The
script then warns, and drops the first timed run of every bench in every process
on BOTH sides, so the comparison stays symmetric.

Pass `--flock PATH` to wrap every timed run in `flock PATH` (serialises
timing against other benchmark users). Builds are never wrapped and all
finish before the first timed run starts.
"""

import argparse
import json
import math
import os
import re
import shlex
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
LOAD_WARN = 4.0  # 1-min load average above this makes timings suspect
P_THRESHOLD = 0.01
MEDIAN_THRESHOLD_PCT = 2.0

# A per-run line printed by the harness under CELLA_BENCH=1, e.g.
#   [bench]       2d_three_state_cycle_t4: 36.092307 ms
RUN_RE = re.compile(r"^\[bench\]\s+(\S+):\s+([0-9]+(?:\.[0-9]+)?) ms\s*$")


# ---------------------------------------------------------------- statistics


def median_sorted(s):
    n = len(s)
    if n == 0:
        return 0.0
    return s[n // 2] if n % 2 else (s[n // 2 - 1] + s[n // 2]) / 2


def find_outliers(times):
    """Same fence as the Rust harness: median +/- max(3*1.4826*MAD, 2% of median).

    MAD (median absolute deviation) = median of |x - median|. Fewer than 5
    runs: nothing is flagged.
    """
    if len(times) < 5:
        return []
    s = sorted(times)
    med = median_sorted(s)
    mad = median_sorted(sorted(abs(t - med) for t in times))
    half = max(3 * 1.4826 * mad, 0.02 * med)
    return [t for t in times if abs(t - med) > half]


def mann_whitney_p(a, b):
    """Two-sided Mann-Whitney U p-value, normal approximation.

    Mid-ranks for ties, tie-corrected variance, continuity correction.
    Returns 1.0 when the data have no spread at all.
    """
    n1, n2 = len(a), len(b)
    if n1 == 0 or n2 == 0:
        return 1.0
    pooled = sorted([(v, 0) for v in a] + [(v, 1) for v in b])
    n = n1 + n2
    ranks = [0.0] * n
    tie_term = 0.0
    i = 0
    while i < n:
        j = i
        while j + 1 < n and pooled[j + 1][0] == pooled[i][0]:
            j += 1
        mid = (i + j) / 2 + 1  # average of 1-based ranks i+1 .. j+1
        for k in range(i, j + 1):
            ranks[k] = mid
        t = j - i + 1
        tie_term += t**3 - t
        i = j + 1
    r1 = sum(r for r, (_, side) in zip(ranks, pooled) if side == 0)
    u1 = r1 - n1 * (n1 + 1) / 2
    mu = n1 * n2 / 2
    var = n1 * n2 / 12 * ((n + 1) - tie_term / (n * (n - 1))) if n > 1 else 0.0
    if var <= 0:
        return 1.0
    diff = abs(u1 - mu) - 0.5  # continuity correction
    z = max(diff, 0.0) / math.sqrt(var)
    return math.erfc(z / math.sqrt(2))  # two-sided


# ------------------------------------------------------------------- helpers


def load1():
    try:
        return float(Path("/proc/loadavg").read_text().split()[0])
    except (OSError, ValueError, IndexError):
        return None


def load_text(v):
    if v is None:
        return "n/a"
    return f"{v:.2f}" + (f" (HIGH, > {LOAD_WARN:g})" if v > LOAD_WARN else "")


def run(cmd, **kw):
    return subprocess.run(cmd, check=True, **kw)


def build_and_locate(cella_lib_dir, label, target_dir=None):
    """Build the long_suite test binary in release mode and return its path."""
    print(f"[ab] building {label} in {cella_lib_dir} ...", flush=True)
    proc = subprocess.run(
        [
            "cargo", "test", "--release", "--no-run", "--test", "long_suite",
            "--message-format=json",
        ],
        cwd=cella_lib_dir,
        env=dict(os.environ, CARGO_TARGET_DIR=str(target_dir)) if target_dir else None,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if proc.returncode != 0:
        sys.exit(f"[ab] build of {label} failed:\n{proc.stderr[-3000:]}")
    exe = None
    for line in proc.stdout.splitlines():
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (
            msg.get("reason") == "compiler-artifact"
            and msg.get("target", {}).get("name") == "long_suite"
            and msg.get("profile", {}).get("test")
            and msg.get("executable")
        ):
            exe = msg["executable"]
    if not exe:
        sys.exit(f"[ab] could not locate the long_suite binary for {label}")
    return exe


def run_once(exe, cwd, filters, runs, warmup, flock, drop_first=False):
    """One timed process; returns {bench_name: [ms, ...]}."""
    cmd = [exe, "--ignored", "--nocapture", "--test-threads=1"] + filters
    if flock:
        cmd = ["flock", flock] + cmd
    env = dict(os.environ, CELLA_BENCH="1", CELLA_BENCH_RUNS=str(runs),
               CELLA_BENCH_WARMUP=str(warmup))
    proc = subprocess.run(cmd, cwd=cwd, env=env, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, text=True)
    if proc.returncode != 0:
        sys.exit(f"[ab] bench process failed ({exe}):\n"
                 f"{proc.stdout[-2000:]}\n{proc.stderr[-2000:]}")
    out = {}
    for line in proc.stdout.splitlines():
        m = RUN_RE.match(line)
        if m:
            out.setdefault(m.group(1), []).append(float(m.group(2)))
    if drop_first:  # cold first run of each bench in this process
        out = {k: v[1:] for k, v in out.items() if len(v) > 1}
    return out


def pct(new, old):
    return (new - old) * 100.0 / old if old else float("nan")


def verdict(p, d_med, d_min):
    """All four gates must hold; otherwise name the ones that failed."""
    failed = []
    if not p < P_THRESHOLD:
        failed.append("p")
    if not abs(d_med) > MEDIAN_THRESHOLD_PCT:
        failed.append("med")
    if not abs(d_min) > MEDIAN_THRESHOLD_PCT:
        failed.append("min")
    if d_min * d_med <= 0:
        failed.append("sign")
    if not failed:
        return "significant " + ("SLOWER" if d_med > 0 else "FASTER")
    return "no change (failed: " + ",".join(failed) + ")"


# ---------------------------------------------------------------------- main


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--a", required=True, help="git ref for side A (B is the working tree)")
    ap.add_argument("--filter", action="append", default=[], help="libtest name filter (repeatable)")
    ap.add_argument("--rounds", type=int, default=8,
                    help="A/B rounds (default 8; rounds mode needs >= 6 to ever reach p < 0.01)")
    ap.add_argument("--runs", type=int, default=10, help="timed runs per bench per round (CELLA_BENCH_RUNS)")
    ap.add_argument("--warmup", type=int, default=1, help="untimed warm-up runs (CELLA_BENCH_WARMUP)")
    ap.add_argument("--unit", choices=["runs", "rounds"], default="rounds",
                    help="Mann-Whitney sample unit: one median per round (default, honest) or "
                         "every run (optimistic p-values: runs within a process are correlated)")
    ap.add_argument("--flock", default=os.environ.get("BENCH_LOCK"),
                    help="wrap every timed run in `flock PATH`")
    ap.add_argument("--max-wait", type=int, default=600,
                    help="after building, wait up to this many seconds for the 1-min load to fall "
                         f"to {LOAD_WARN:g} or less (your own build raises it); 0 = don't wait")
    ap.add_argument("--workdir", help="scratch dir for the worktree (default: a temp dir)")
    ap.add_argument("--json", help="write raw samples and the report to this file")
    args = ap.parse_args()

    filters = [f for raw in args.filter for f in shlex.split(raw)]
    if not filters:
        sys.exit("[ab] give at least one --filter (an empty filter would run the whole suite)")

    work = Path(args.workdir) if args.workdir else Path(tempfile.mkdtemp(prefix="bench_ab_"))
    work.mkdir(parents=True, exist_ok=True)
    wt = work / "wt-a"
    made_worktree = False
    try:
        if wt.exists():
            print(f"[ab] note: stale worktree {wt} found (reused --workdir); removing it first")
            subprocess.run(["git", "worktree", "remove", "--force", str(wt)], cwd=REPO,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if wt.exists():
                shutil.rmtree(wt, ignore_errors=True)
            subprocess.run(["git", "worktree", "prune"], cwd=REPO)
        run(["git", "worktree", "add", "--detach", str(wt), args.a], cwd=REPO,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        made_worktree = True
        sha_a = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=wt,
                               stdout=subprocess.PIPE, text=True).stdout.strip()
        dirty = subprocess.run(["git", "status", "--porcelain"], cwd=REPO,
                               stdout=subprocess.PIPE, text=True).stdout.strip()
        show = subprocess.run(["git", "show", f"{args.a}:cella_lib/tests/long_suite.rs"], cwd=REPO,
                              stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        a_has_warmup = "CELLA_BENCH_WARMUP" in show.stdout
        drop_first = not a_has_warmup
        if not a_has_warmup:
            print(f"[ab] WARNING: ref A ({args.a}) has no CELLA_BENCH_WARMUP, so side A includes its cold "
                  "first run and the result is biased toward 'B faster'.\n"
                  "[ab]          Mitigation: dropping the first timed run of every bench in every process, "
                  "on both sides.")
        if args.unit == "rounds" and args.rounds < 6:
            print(f"[ab] WARNING: only {args.rounds} rounds in rounds mode; with fewer than 6 per side "
                  "p < 0.01 is unreachable, so nothing can be called significant. Use --rounds 8.")
        if args.unit == "runs":
            print("[ab] NOTE: --unit runs gives optimistic p-values (runs within a process are correlated).")
        # Build both sides BEFORE any timing so compilation never overlaps a timed run.
        dir_a, dir_b = wt / "cella_lib", REPO / "cella_lib"
        # A gets its own target dir inside the work dir, so passing --workdir keeps
        # the build cache between invocations (the worktree itself is always removed).
        exe_a = build_and_locate(dir_a, f"A ({args.a} = {sha_a})", work / "target-a")
        exe_b = build_and_locate(dir_b, "B (working tree)")
        waited = 0
        while args.max_wait and (load1() or 0) > LOAD_WARN and waited < args.max_wait:
            if waited == 0:
                print(f"[ab] waiting for load to fall to {LOAD_WARN:g} (the build raised it) ...", flush=True)
            time.sleep(15)
            waited += 15
        start_load = load1()
        print(f"[ab] load average at start: {load_text(start_load)}")
        if start_load is not None and start_load > LOAD_WARN:
            print("[ab] WARNING: machine is busy; results are NOT trustworthy.")
        print(f"[ab] A = {args.a} ({sha_a}), B = working tree"
              f"{' (has uncommitted changes)' if dirty else ' (clean)'}")
        print(f"[ab] filters {filters}, {args.rounds} rounds x {args.runs} runs, warm-up {args.warmup}, unit {args.unit}")
        if not a_has_warmup:
            print("[ab] REPORT CAVEAT: A has no warm-up (cold first run, biased toward 'B faster'); "
                  "first timed run dropped on both sides.")
        if args.unit == "runs":
            print("[ab] REPORT CAVEAT: unit=runs, optimistic p-values (runs within a process are correlated).")

        a_runs, b_runs = {}, {}  # bench -> list (per round) of lists of ms
        for r in range(1, args.rounds + 1):
            for side, exe, cwd, store in (("A", exe_a, dir_a, a_runs), ("B", exe_b, dir_b, b_runs)):
                res = run_once(exe, cwd, filters, args.runs, args.warmup, args.flock, drop_first)
                for name, ts in res.items():
                    store.setdefault(name, []).append(ts)
            print(f"[ab] round {r}/{args.rounds} done (load {load_text(load1())})", flush=True)
        end_load = load1()
        print(f"[ab] load average at end: {load_text(end_load)}")

        names = sorted(set(a_runs) & set(b_runs))
        only = sorted(set(a_runs) ^ set(b_runs))
        if only:
            print(f"[ab] skipped (present on one side only): {', '.join(only)}")
        if not names:
            sys.exit("[ab] no benches in common; check the filter")

        hdr = (f"{'bench':<30} {'A min':>9} {'B min':>9} {'Δmin':>8} {'A med':>9} {'B med':>9} "
               f"{'Δmed':>8} {'outA':>5} {'outB':>5} {'p':>8}  verdict")
        print(f"\n[ab] report: p-values use unit={args.unit}"
              + (" (optimistic p-values, runs within a process are correlated)" if args.unit == "runs" else ""))
        if not a_has_warmup:
            print("[ab] report: A has NO warm-up, biased toward 'B faster' (first timed run dropped on both sides)")
        print("\n" + hdr)
        print("-" * len(hdr))
        report, n_sig = [], 0
        for n in names:
            pa = [v for rd in a_runs[n] for v in rd]
            pb = [v for rd in b_runs[n] for v in rd]
            if args.unit == "rounds":
                sa = [statistics.median(rd) for rd in a_runs[n]]
                sb = [statistics.median(rd) for rd in b_runs[n]]
            else:
                sa, sb = pa, pb
            mn_a, mn_b = min(pa), min(pb)
            md_a, md_b = statistics.median(pa), statistics.median(pb)
            d_min, d_med = pct(mn_b, mn_a), pct(md_b, md_a)
            out_a = sum(len(find_outliers(rd)) for rd in a_runs[n])
            out_b = sum(len(find_outliers(rd)) for rd in b_runs[n])
            p = mann_whitney_p(sa, sb)
            v = verdict(p, d_med, d_min)
            n_sig += v.startswith("significant")
            print(f"{n:<30} {mn_a:>9.3f} {mn_b:>9.3f} {d_min:>+7.2f}% {md_a:>9.3f} {md_b:>9.3f} "
                  f"{d_med:>+7.2f}% {out_a:>5} {out_b:>5} {p:>8.4f}  {v}")
            report.append(dict(bench=n, a_min=mn_a, b_min=mn_b, d_min_pct=d_min, a_med=md_a,
                               b_med=md_b, d_med_pct=d_med, out_a=out_a, out_b=out_b, p=p,
                               verdict=v))
        tot_out = sum(r["out_a"] + r["out_b"] for r in report)
        print(f"\n[ab] {n_sig} of {len(names)} benches significant "
              f"(rule: p < {P_THRESHOLD}, |Δmedian| and |Δmin| > {MEDIAN_THRESHOLD_PCT:g} %, same sign); "
              f"{tot_out} outlier runs flagged in total (kept in the numbers above, not dropped).")
        if (start_load or 0) > LOAD_WARN or (end_load or 0) > LOAD_WARN:
            print("[ab] WARNING: load was above the limit at start or end; re-run on a quiet machine.")
        if args.json:
            Path(args.json).write_text(json.dumps(dict(
                a=args.a, sha_a=sha_a, filters=filters, rounds=args.rounds, runs=args.runs,
                unit=args.unit, load_start=start_load, load_end=end_load, report=report,
                a_runs=a_runs, b_runs=b_runs), indent=1))
    finally:
        if made_worktree:
            subprocess.run(["git", "worktree", "remove", "--force", str(wt)], cwd=REPO,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            subprocess.run(["git", "worktree", "prune"], cwd=REPO)
        if not args.workdir:
            shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
