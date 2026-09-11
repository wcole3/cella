#!/usr/bin/env python
"""Compute the lagged-persistence and lagged-Circle nulls straight from a
fire's truth.json/scenario.json, with no wildfire_smc run at all -- both
nulls depend only on the observed masks, never on the ensemble. Used to
attach them to E40's already-collected reports (exp40_observed_immigrants)
without a rerun, per the controller's finding: E40's own headline
comparison used the wrong (unlagged) persistence null.

Mirrors cella_lib/examples/wildfire_smc.rs's chamfer_from / mask_at / iou
exactly (same 3/4 chamfer weights, same "closest cell wins ties by index"
radial rule), so the numbers agree with what a rerun would report --
checked directly in the report against E40b's own native lagged-null
fields, which come from the same Rust code.
"""
import json
import sys
from pathlib import Path

VAL = Path(__file__).resolve().parents[2]
SCEN = VAL / "data" / "scenarios"


def iou(a, b):
    inter = sum(1 for x, y in zip(a, b) if x and y)
    union = sum(1 for x, y in zip(a, b) if x or y)
    return inter / union if union else 0.0


def mask_at(arrival, t):
    return [0.0 <= a <= t for a in arrival]


def chamfer_from(seed_mask, w, h):
    FAR = float("inf")
    d = [0.0 if s else FAR for s in seed_mask]

    def idx(x, y):
        return y * w + x

    for y in range(h):
        for x in range(w):
            best = d[idx(x, y)]
            if x > 0:
                best = min(best, d[idx(x - 1, y)] + 3)
            if y > 0:
                best = min(best, d[idx(x, y - 1)] + 3)
                if x > 0:
                    best = min(best, d[idx(x - 1, y - 1)] + 4)
                if x + 1 < w:
                    best = min(best, d[idx(x + 1, y - 1)] + 4)
            d[idx(x, y)] = best
    for y in range(h - 1, -1, -1):
        for x in range(w - 1, -1, -1):
            best = d[idx(x, y)]
            if x + 1 < w:
                best = min(best, d[idx(x + 1, y)] + 3)
            if y + 1 < h:
                best = min(best, d[idx(x, y + 1)] + 3)
                if x + 1 < w:
                    best = min(best, d[idx(x + 1, y + 1)] + 4)
                if x > 0:
                    best = min(best, d[idx(x - 1, y + 1)] + 4)
            d[idx(x, y)] = best
    return d


def radial_mask_from(seed_mask, w, h, target_area):
    dist = chamfer_from(seed_mask, w, h)
    order = sorted(range(w * h), key=lambda i: (dist[i], i))
    out = [False] * (w * h)
    for i in order[:target_area]:
        out[i] = True
    return out


def lagged_nulls(fire):
    sc = json.loads((SCEN / fire / "scenario.json").read_text())
    truth = json.loads((SCEN / fire / "truth.json").read_text())
    w, h = sc["grid"]["width"], sc["grid"]["height"]
    arrival = truth["arrival_hours"]
    observed_at = truth["observed_at"]

    prev = mask_at(arrival, observed_at[0])  # ignition, t=0
    lagged_pers, lagged_circle = [], []
    for k in range(1, len(observed_at)):
        t = observed_at[k]
        obs = mask_at(arrival, t)
        if k >= 2:  # None at the first scored window, same as the Rust code
            lagged_pers.append(iou(prev, obs))
            n = sum(obs)
            circle = radial_mask_from(prev, w, h, n)
            lagged_circle.append(iou(circle, obs))
        prev = obs
    mean = lambda xs: sum(xs) / len(xs) if xs else 0.0
    return mean(lagged_pers), mean(lagged_circle), len(lagged_pers)


if __name__ == "__main__":
    fires = sys.argv[1:] or [
        "Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016", "Ferguson_2018", "Pier_2017"
    ]
    for f in fires:
        p, c, n = lagged_nulls(f)
        print(f"{f:14s} mean_lagged_persistence_iou={p:.4f} mean_lagged_circle_iou={c:.4f} (n={n} windows)")
