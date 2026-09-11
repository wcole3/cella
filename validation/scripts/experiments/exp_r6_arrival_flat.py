#!/usr/bin/env python
"""E30a: does making wind set ignition *time* instead of ignition *chance*
give a spread kernel whose elongation survives at any fire size?

Runs `cella_lib/examples/wildfire_ros` (a synthetic-grid example, no
scenario needed) in its three E30a modes and combines their JSON reports
into one file:

- `arrival_flat`: the E19 flat-grid front-speed table, both spread rules
  (`bernoulli`/`arrival`), wind 0/2/5/8 m/s, p0 0.12/0.22/0.44, burn
  duration 5/10, 3 seeds.
- `illuminate`: point-ignition elongation (E12's second-moment measure) at
  2/5/10/20 % burned on a 400x400 uniform grid, 8 m/s toward +x, both
  rules, 3 seeds.
- `lb`: length-to-breadth table on the arrival rule at 10 % size for
  2/5/8 m/s, `c2` in {0.131, 0.2, 0.3, 0.45} under the exponential wind
  law plus the rear-focus law, against Anderson (1983)'s `LB(U)`, and the
  closed-form head:back ratio at 0.6 m/s for both laws.

Each mode's own `binary_git`/`binary_built_utc` (stamped by
`cella_lib/build.rs`) must agree; the script fails loudly if they don't
(a rebuild happened mid-run) rather than silently mixing binaries.

Output: `validation/results/experiments/exp30a_arrival_flat.json`, one
object with `binary_git`, `binary_built_utc`, `speed_table`,
`illuminate`, `lb`.

Usage (from repo root): `python3 validation/scripts/experiments/exp_r6_arrival_flat.py`
"""
import json
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VAL = REPO / "validation"
OUT = VAL / "results" / "experiments" / "exp30a_arrival_flat.json"
BIN = REPO / "cella_lib" / "target" / "release" / "examples" / "wildfire_ros"


def run_mode(mode: str) -> dict:
    proc = subprocess.run([str(BIN), mode], check=True, capture_output=True, text=True)
    sys.stderr.write(proc.stderr)
    return json.loads(proc.stdout)


def main() -> None:
    if not BIN.exists():
        raise SystemExit(
            f"{BIN} not built — run: cd cella_lib && cargo build --release --examples"
        )
    modes = ["arrival_flat", "illuminate", "lb"]
    reports = {m: run_mode(m) for m in modes}

    gits = {r["binary_git"] for r in reports.values()}
    builts = {r["binary_built_utc"] for r in reports.values()}
    if len(gits) != 1 or len(builts) != 1:
        raise SystemExit(f"binary_git/binary_built_utc disagree across modes: {reports}")
    binary_git = gits.pop()
    if binary_git.endswith("-dirty"):
        print(f"WARNING: binary_git is dirty ({binary_git}) — not a reproducible run", file=sys.stderr)

    combined = {
        "binary_git": binary_git,
        "binary_built_utc": builts.pop(),
        "speed_table": reports["arrival_flat"]["results"],
        "illuminate": reports["illuminate"]["results"],
        "lb": reports["lb"]["results"],
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(combined, indent=1))
    print(
        f"wrote {OUT} (binary_git {combined['binary_git']}): "
        f"{len(combined['speed_table'])} speed rows, "
        f"{len(combined['illuminate'])} illuminate rows, "
        f"{len(combined['lb'])} lb rows",
        flush=True,
    )


if __name__ == "__main__":
    main()
