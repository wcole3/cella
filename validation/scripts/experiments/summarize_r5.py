#!/usr/bin/env python
"""Markdown tables for the Round 5 experiments (E32-E38) from whatever
result files exist in results/experiments/. Deltas are judged against the
E33 per-fire sd: a |delta| below it is marked '=' (tie)."""
import json, statistics as st
from pathlib import Path

EXP = Path(__file__).resolve().parents[2] / "results" / "experiments"
FIRES = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016", "Ferguson_2018", "Pier_2017"]
SHORT = [f.split("_")[0] + ("*" if f in ("Ferguson_2018", "Pier_2017") else "") for f in FIRES]


def load(name):
    p = EXP / name
    return json.loads(p.read_text()) if p.exists() else None


def pick(rows, fire, **kw):
    for r in rows:
        if r["fire"] == fire and all(r.get(k) == v for k, v in kw.items()):
            return r
    return None


noise = load("exp33_noise.json")
if noise is None:
    raise SystemExit("run E33 first")
base = {f: pick(noise, f, seed=0) for f in FIRES}
sd = {f: st.stdev([r["mean_consensus_iou"] for r in noise if r["fire"] == f]) for f in FIRES}


def verdict(f, v):
    d = v - base[f]["mean_consensus_iou"]
    return f"{d:+.3f}" + ("" if abs(d) >= sd[f] else " =")


def table(title, configs, rows, key="mean_consensus_iou", fmt="{:.3f}", show_delta=True):
    print(f"\n### {title}\n")
    print("| config | " + " | ".join(SHORT) + " |")
    print("|---|" + "---|" * len(FIRES))
    print("| base (E33 seed 0) | " + " | ".join(fmt.format(base[f][key]) for f in FIRES) + " |")
    for label, sel in configs:
        cells = []
        for f in FIRES:
            r = pick(rows, f, **sel)
            if r is None:
                cells.append("")
            elif show_delta and key == "mean_consensus_iou":
                cells.append(f"{fmt.format(r[key])} ({verdict(f, r[key])})")
            else:
                cells.append(fmt.format(r[key]))
        print(f"| {label} | " + " | ".join(cells) + " |")
    print("| E33 sd | " + " | ".join(f"{sd[f]:.3f}" for f in FIRES) + " |")


print("## Round 5 summary\n")
print("Base = assim, beta 10, sigma 0.2, immigrants 0.2, containment only, M 32, seed 0. '=' marks a delta inside the fire's E33 sd.")

if (rows := load("exp32_members.json")) is not None:
    table("E32 ensemble size: mean consensus IoU", [(f"M = {m}", {"members": m}) for m in (8, 16, 64, 128)], rows)
    table("E32 ensemble size: mean Brier", [(f"M = {m}", {"members": m}) for m in (8, 16, 64, 128)], rows, key="mean_brier_ensemble", fmt="{:.4f}")

if (rows := load("exp34_operators.json")) is not None:
    cfgs = [(c, {"config": c}) for c in ["imm0", "imm0.1", "imm0.4", "sigma0.1", "sigma0.4", "beta5", "beta20", "cross0.5", "cross1.0"]]
    table("E34 operators: mean consensus IoU", cfgs, rows)
    table("E34 operators: mean Brier", cfgs, rows, key="mean_brier_ensemble", fmt="{:.4f}")
    table("E34 operators: mean ESS", cfgs, rows, key="mean_ess", fmt="{:.1f}")

if (rows := load("exp35_prior.json")) is not None:
    cfgs = [("narrow", {"config": "prior_narrow"}), ("very broad", {"config": "prior_very_broad"})]
    table("E35 prior width: mean consensus IoU", cfgs, rows)
    table("E35 prior width: mean Brier", cfgs, rows, key="mean_brier_ensemble", fmt="{:.4f}")

if (rows := load("exp36_offline.json")) is not None:
    print("\n### E36 offline fit vs filter: forecast IoU on days after the fit window\n")
    print("| fire | fit days | fit IoU | GA forward, days 4+ | filter, days 4+ | delta | GA forward, all days | filter, all days |")
    print("|---|---|---|---|---|---|---|---|")
    for f in FIRES:
        r = pick(rows, f)
        if r is None:
            continue
        k = r["fit"]["fit_days"]
        ga = r["per_day_consensus"]; fi = base[f]["per_day_consensus"]
        n = min(len(ga), len(fi))
        ga_f = st.mean(ga[k:n]); fi_f = st.mean(fi[k:n])
        print(f"| {f.split('_')[0]} | {k} | {r['fit']['best_fit_iou']:.3f} | {ga_f:.3f} | {fi_f:.3f} | {ga_f - fi_f:+.3f} | {st.mean(ga[:n]):.3f} | {st.mean(fi[:n]):.3f} |")
    print("\nFitted genomes:")
    for f in FIRES:
        r = pick(rows, f)
        if r:
            g = {k: (list(v.values())[0] if isinstance(v, dict) else v) for k, v in r["fit"]["best_genome"].items()}
            print(f"- {f.split('_')[0]}: " + ", ".join(f"{k} {v:.3g}" if isinstance(v, float) else f"{k} {v}" for k, v in g.items()))

if (rows := load("exp37_illuminate.json")) is not None:
    print("\n### E37 illumination: model elongation range vs observed\n")
    print("| fire | elites | coverage | model elongation max | observed elongation (by day) | observed growth (by day) |")
    print("|---|---|---|---|---|---|")
    for f in FIRES:
        r = pick(rows, f)
        if r is None:
            continue
        obs_el = " ".join(f"{o[2]:.2f}" for o in r["observed"]); obs_g = " ".join(f"{o[1]:.3f}" for o in r["observed"])
        print(f"| {f.split('_')[0]} | {r['elites']} | {r['coverage']:.2f} | {r['max_model_elongation']:.2f} | {obs_el} | {obs_g} |")

if (rows := load("exp38_immreset.json")) is not None:
    print("\n### E38 immigrant reset: per seed, reset minus E33 twin (mean consensus IoU)\n")
    print("| fire | " + " | ".join(f"seed {s}" for s in range(5)) + " | mean delta |")
    print("|---|" + "---|" * 6)
    for f in FIRES:
        ds = []
        cells = []
        for s in range(5):
            a = pick(rows, f, seed=s); b = pick(noise, f, seed=s)
            if a and b:
                d = a["mean_consensus_iou"] - b["mean_consensus_iou"]; ds.append(d)
                cells.append(f"{a['mean_consensus_iou']:.3f} ({d:+.3f})")
            else:
                cells.append("")
        print(f"| {f.split('_')[0]} | " + " | ".join(cells) + f" | {st.mean(ds):+.3f} |" if ds else "")
