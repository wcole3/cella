#!/usr/bin/env python
"""Shared runner for the Round 6 experiments. Reuses the Round 5 constants
(`BIN`, `FIRES`, `HOLDOUT`, `EXP`) from `r5_common` rather than duplicating
them, and adds `run_nulls` for `wildfire_smc`'s `nulls` mode (E41): a single
run per fire that reports persistence, the Circle and all three Ellipse
variants at once, so there is no "config" axis to loop over the way
`r5_common.run` loops over ensemble settings.
"""
import json
import os
import subprocess

from r5_common import BIN, EXP, FIRES, HOLDOUT, VAL

__all__ = ["BIN", "EXP", "FIRES", "HOLDOUT", "VAL", "run_nulls"]


def run_nulls(out_dir, fire, env=None):
    """One `wildfire_smc <fire> 0 nulls <out.json>` run. Returns the parsed
    report (all of persistence/Circle/Ellipse-×3, every observation) plus
    the wall time the binary itself measured.
    """
    out_dir.mkdir(parents=True, exist_ok=True)
    rep = out_dir / f"{fire}_nulls.json"
    full_env = {**os.environ, **(env or {})}
    subprocess.run(
        [str(BIN), str(VAL / "data" / "scenarios" / fire), "0", "nulls", str(rep)],
        check=True,
        capture_output=True,
        env=full_env,
    )
    r = json.loads(rep.read_text())
    print(
        f"{fire:14s} nulls  {r['wall_time_secs']:5.1f}s | mean IoU "
        f"persistence {r['mean_persistence_iou']:.3f} circle {r['mean_radial_iou']:.3f} "
        f"ellipse_era5 {r['mean_ellipse_era5_iou']:.3f} era5x3 {r['mean_ellipse_era5x3_iou']:.3f} "
        f"era5_centred {r['mean_ellipse_era5_centred_iou']:.3f} (post-hoc) "
        f"station {r['mean_ellipse_station_iou']}",
        flush=True,
    )
    return r
