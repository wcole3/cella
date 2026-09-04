#!/usr/bin/env python
"""Print a fire x variant table of mean IoU (and area ratio) for one
experiment JSON, with the Circle for reference. Usage: summarize.py FILE"""
import json, sys
rows = json.load(open(sys.argv[1]))
fires = []
for r in rows:
    if r["fire"] not in fires:
        fires.append(r["fire"])
variants = []
for r in rows:
    if r["variant"] not in variants:
        variants.append(r["variant"])
print("| variant | " + " | ".join(f.split("_")[0] for f in fires) + " |")
print("|---|" + "---|" * len(fires))
for v in variants:
    cells = []
    for f in fires:
        m = [r for r in rows if r["fire"] == f and r["variant"] == v]
        cells.append(f"{m[0]['mean_iou']:.3f} (×{m[0]['area_ratio']:.1f})" if m else "")
    print(f"| {v} | " + " | ".join(cells) + " |")
circle = {f: next(r["mean_iou_radial"] for r in rows if r["fire"] == f) for f in fires}
print("| Circle | " + " | ".join(f"{circle[f]:.3f}" for f in fires) + " |")
