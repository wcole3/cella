#!/usr/bin/env python
"""Fire x config tables for the ensemble experiments (E24/E25)."""
import json, sys
from pathlib import Path
EXP = Path(__file__).resolve().parents[2] / "results" / "experiments"
rows = []
for f in ["exp24_smc.json", "exp25b_smc_imm.json", "exp25c_smc_horizon.json"]:
    p = EXP / f
    if p.exists():
        rows += json.load(open(p))
fires = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016", "Ferguson_2018", "Pier_2017"]
configs = []
for r in rows:
    if r["config"] not in configs:
        configs.append(r["config"])
def cell(f, c, key, fmt="{:.3f}"):
    m = [r for r in rows if r["fire"] == f and r["config"] == c]
    return fmt.format(m[0][key]) if m else ""
for key, title in [("mean_consensus_iou", "mean consensus IoU (p >= 0.5)"), ("mean_member_iou", "mean member IoU"),
                   ("final_consensus_iou", "final-day consensus IoU"), ("mean_brier_ensemble", "mean Brier (ensemble)")]:
    print(f"\n### {title}\n")
    print("| config | " + " | ".join(f.split("_")[0] for f in fires) + " |")
    print("|---|" + "---|" * len(fires))
    for c in configs:
        print(f"| {c} | " + " | ".join(cell(f, c, key) for f in fires) + " |")
    if key == "mean_consensus_iou":
        print("| Circle (mean IoU) | " + " | ".join(cell(f, configs[0], "mean_radial_iou") for f in fires) + " |")
    if key == "mean_brier_ensemble":
        print("| Circle (Brier) | " + " | ".join(cell(f, configs[0], "mean_brier_radial") for f in fires) + " |")
print("\n### final ensemble parameter means (assim configs)\n")
print("| config | fire | p0 | tau (d) | dur | wind x |")
print("|---|---|---|---|---|---|")
for c in configs:
    if not c.startswith("assim"):
        continue
    for f in fires:
        m = [r for r in rows if r["fire"] == f and r["config"] == c]
        if m:
            r = m[0]
            print(f"| {c} | {f.split('_')[0]} | {r['final_p0_mean']:.2f} | {r['final_tau_mean']:.0f} | {r['final_dur_mean']:.0f} | {r['final_wind_scale_mean']:.2f} |")
