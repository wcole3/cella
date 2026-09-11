#!/usr/bin/env python
"""E41: the Ellipse null. One `wildfire_smc ... nulls` run per fire (no
ensemble members) gives persistence, the Circle and all three Ellipse
variants (`ellipse_era5`, `ellipse_station`, `ellipse_era5x3`) in one
pass. This script fans that out to all six fires and flattens the result
into one row per fire x variant (mean/final IoU, mean Brier), so the
experiment file and figure can read a single JSON.

Usage: python exp_r6_ellipse.py
Writes: results/experiments/exp41_ellipse.json
"""
import json
from concurrent.futures import ThreadPoolExecutor

from r6_common import EXP, FIRES, HOLDOUT, run_nulls

VARIANTS = [
    ("persistence", "persistence_iou", "brier_persistence"),
    ("circle", "radial_iou", "brier_radial"),
    ("ellipse_era5", "ellipse_era5_iou", "brier_ellipse_era5"),
    ("ellipse_station", "ellipse_station_iou", "brier_ellipse_station"),
    ("ellipse_era5x3", "ellipse_era5x3_iou", "brier_ellipse_era5x3"),
]


def rows_for(fire, report):
    scores = report["scores"]
    n = len(scores)
    out = []
    for variant, iou_key, brier_key in VARIANTS:
        ious = [s[iou_key] for s in scores if s[iou_key] is not None]
        briers = [s[brier_key] for s in scores if s[brier_key] is not None]
        if not ious:
            # ellipse_station with no station file for this fire.
            out.append({
                "fire": fire, "variant": variant, "holdout": fire in HOLDOUT,
                "mean_iou": None, "final_iou": None, "mean_brier": None,
                "station_available": report["station_available"],
            })
            continue
        out.append({
            "fire": fire, "variant": variant, "holdout": fire in HOLDOUT,
            "mean_iou": sum(ious) / n, "final_iou": ious[-1],
            "mean_brier": sum(briers) / n,
            "station_available": report["station_available"],
        })
    return out


def main():
    out_dir = EXP / "exp41_ellipse"
    with ThreadPoolExecutor(max_workers=3) as ex:
        reports = list(ex.map(lambda f: (f, run_nulls(out_dir, f)), FIRES))
    rows = []
    for fire, report in reports:
        rows.extend(rows_for(fire, report))
    (EXP / "exp41_ellipse.json").write_text(json.dumps(rows, indent=1))
    total_wall = sum(r["wall_time_secs"] for _, r in reports)
    print(f"wrote exp41_ellipse.json, {len(rows)} rows, total wall time {total_wall:.1f}s")


if __name__ == "__main__":
    main()
