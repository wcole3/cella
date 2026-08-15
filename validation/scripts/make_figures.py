#!/usr/bin/env python
"""Draw the figures embedded in validation/ANALYSIS.md.

Everything here is drawn from real harness output — no illustrative fakes.
Inputs (all produced by `wildfire_validate`, see validation/README.md):

- validation/data/scenarios/<fire>/truth.json   observed arrival grid
- validation/results/<fire>.json                score report (ensemble means)
- validation/results/fields/<fire>.json         per-cell arrival grids
                                                (seed-0 sim + radial null)
- validation/results/p0_sweep.json              optional; from p0_sweep.py

Outputs PNGs into validation/figures/ (committed, unlike results/).

Run:
    validation/.venv/bin/python validation/scripts/make_figures.py
"""

import json
from pathlib import Path

import numpy as np
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.colors import ListedColormap
from matplotlib.patches import Patch

ROOT = Path(__file__).resolve().parents[1]
SCENARIOS = ROOT / "data" / "scenarios"
RESULTS = ROOT / "results"
FIGURES = ROOT / "figures"

FIRES = ["Bear_2020", "Brattain_2020", "Buck_2017",
         "Chimney_2016", "Ferguson_2018", "Pier_2017"]
HOLDOUT = {"Ferguson_2018", "Pier_2017"}

# One cell is 30 m x 30 m = 900 m^2; this converts a cell count to km^2.
KM2_PER_CELL = 900.0 / 1e6

# Palette (matches ANALYSIS.md / the scorecard page).
INK = "#1E2226"
MUTED = "#5A646D"
PAPER = "#EDEFF1"
HIT = "#3B4046"      # burned in both maps — the model got this land right
FALSE = "#D2571F"    # model burned it, reality did not (false alarm)
MISS = "#3A6EA5"     # reality burned it, model missed it
CIRCLE = "#9AA4AC"   # the area-matched radial null


def pretty(fire):
    name = fire.replace("_", " ")
    return name + " (holdout)" if fire in HOLDOUT else name


def load_fire(fire):
    truth = json.loads((SCENARIOS / fire / "truth.json").read_text())
    report = json.loads((RESULTS / f"{fire}.json").read_text())
    fields = json.loads((RESULTS / "fields" / f"{fire}.json").read_text())
    h, w = fields["height"], fields["width"]
    grids = {
        "obs": np.array(truth["arrival_hours"]).reshape(h, w),
        "sim": np.array(fields["sim_arrival_seed0"]).reshape(h, w),
        "radial": np.array(fields["radial_arrival"]).reshape(h, w),
    }
    return truth, report, grids


def burned(arrival, t):
    """Burned mask at time t from an arrival grid (-1 = never burned)."""
    return (arrival >= 0.0) & (arrival <= t)


def crop_box(masks, pad=12):
    """Bounding box around everything any mask burned, plus a margin."""
    union = np.zeros_like(masks[0])
    for m in masks:
        union |= m
    ys, xs = np.nonzero(union)
    h, w = union.shape
    return (max(ys.min() - pad, 0), min(ys.max() + pad, h),
            max(xs.min() - pad, 0), min(xs.max() + pad, w))


def style_axes(ax):
    ax.set_xticks([])
    ax.set_yticks([])
    for s in ax.spines.values():
        s.set_color(MUTED)
        s.set_linewidth(0.6)


# ---------------------------------------------------------------- figure 1
def fig_triptych(fire="Bear_2020"):
    """Observed vs model vs Circle, side by side, one fire. The clearest
    single picture of the over-burning problem."""
    truth, report, g = load_fire(fire)
    t = truth["observed_at"][-1]
    obs = burned(g["obs"], t)
    sim = burned(g["sim"], t)
    rad = burned(g["radial"], t)
    y0, y1, x0, x1 = crop_box([obs, sim, rad])

    panels = [
        (obs, HIT, f"What really burned\n{obs.sum() * KM2_PER_CELL:.0f} km²"),
        (sim, FALSE, f"What the model predicted\n{sim.sum() * KM2_PER_CELL:.0f} km²"),
        (rad, CIRCLE, "The Circle (dumb baseline,\narea forced to match)"),
    ]
    fig, axes = plt.subplots(1, 3, figsize=(10.5, 4.2))
    for ax, (mask, color, title) in zip(axes, panels):
        ax.imshow(mask[y0:y1, x0:x1], cmap=ListedColormap([PAPER, color]),
                  interpolation="nearest")
        ax.set_title(title, fontsize=10.5, color=INK)
        style_axes(ax)
    fig.suptitle(f"{pretty(fire)} — final day, textbook settings, before any tuning",
                 fontsize=12, color=INK)
    fig.tight_layout(rect=[0, 0, 1, 0.94])
    fig.savefig(FIGURES / "bear_triptych.png", dpi=150)
    plt.close(fig)


# ---------------------------------------------------------------- figure 2
def fig_agreement():
    """All six fires: where the model was right, where it false-alarmed,
    where it missed — at the final observation."""
    fig, axes = plt.subplots(2, 3, figsize=(11, 7.6))
    cmap = ListedColormap([PAPER, HIT, FALSE, MISS])
    for ax, fire in zip(axes.flat, FIRES):
        truth, report, g = load_fire(fire)
        t = truth["observed_at"][-1]
        obs = burned(g["obs"], t)
        sim = burned(g["sim"], t)
        cat = np.zeros(obs.shape, dtype=np.uint8)
        cat[sim & obs] = 1   # hit
        cat[sim & ~obs] = 2  # false alarm
        cat[~sim & obs] = 3  # miss
        y0, y1, x0, x1 = crop_box([obs, sim])
        ax.imshow(cat[y0:y1, x0:x1], cmap=cmap, vmin=0, vmax=3,
                  interpolation="nearest")
        ax.set_title(f"{pretty(fire)}\nmodel {report['final_iou_model']:.2f}  ·  "
                     f"Circle {report['final_iou_radial']:.2f}",
                     fontsize=10, color=INK)
        style_axes(ax)
    fig.legend(handles=[
        Patch(color=HIT, label="correct (burned in both)"),
        Patch(color=FALSE, label="false alarm (model only)"),
        Patch(color=MISS, label="miss (reality only)"),
    ], loc="lower center", ncol=3, frameon=False, fontsize=10)
    fig.suptitle("Where the predictions go wrong — final day, uncalibrated model",
                 fontsize=13, color=INK)
    fig.tight_layout(rect=[0, 0.05, 1, 0.95])
    fig.savefig(FIGURES / "agreement_maps.png", dpi=150)
    plt.close(fig)


# ---------------------------------------------------------------- figure 3
def fig_area_curves():
    """Burned area over time, model vs reality. Shows the over-burning
    growing day by day instead of as one final number."""
    fig, axes = plt.subplots(2, 3, figsize=(11, 6.4), sharex=False)
    for ax, fire in zip(axes.flat, FIRES):
        _, report, _ = load_fire(fire)
        hours = [s["hours"] for s in report["model"]]
        sim = [s["sim_burned"] * KM2_PER_CELL for s in report["model"]]
        obs = [s["obs_burned"] * KM2_PER_CELL for s in report["model"]]
        ax.plot(hours, sim, color=FALSE, lw=2, label="model")
        ax.plot(hours, obs, color=INK, lw=2, ls="--", marker="o", ms=3,
                label="reality")
        ax.set_title(pretty(fire), fontsize=10.5, color=INK)
        ax.grid(color=PAPER, lw=0.8)
        ax.tick_params(labelsize=8.5, colors=MUTED)
        for s in ax.spines.values():
            s.set_color(MUTED)
            s.set_linewidth(0.6)
    for ax in axes[1]:
        ax.set_xlabel("hours since ignition", fontsize=9, color=MUTED)
    for ax in axes[:, 0]:
        ax.set_ylabel("burned area (km²)", fontsize=9, color=MUTED)
    handles, labels = axes.flat[0].get_legend_handles_labels()
    fig.legend(handles, labels, loc="lower center", ncol=2, frameon=False,
               fontsize=10)
    fig.suptitle("Burned area over time — the model burns too much, everywhere",
                 fontsize=13, color=INK)
    fig.tight_layout(rect=[0, 0.05, 1, 0.95])
    fig.savefig(FIGURES / "area_curves.png", dpi=150)
    plt.close(fig)


# ---------------------------------------------------------------- figure 4
def fig_scores():
    """Final overlap score, model vs both dummies, per fire."""
    rows = []
    for fire in FIRES:
        _, report, _ = load_fire(fire)
        rows.append((pretty(fire), report["final_iou_model"],
                     report["final_iou_persistence"], report["final_iou_radial"]))
    rows.reverse()  # so the chart reads top-down alphabetically
    names = [r[0] for r in rows]
    y = np.arange(len(rows))
    bh = 0.26
    fig, ax = plt.subplots(figsize=(9, 4.8))
    ax.barh(y + bh, [r[1] for r in rows], bh, color=FALSE, label="model")
    ax.barh(y, [r[3] for r in rows], bh, color=CIRCLE, label="the Circle")
    ax.barh(y - bh, [r[2] for r in rows], bh, color=PAPER,
            edgecolor=MUTED, lw=0.7, label="persistence")
    ax.set_yticks(y, names, fontsize=10)
    ax.set_xlabel("final overlap score (IoU) — higher is better", fontsize=10,
                  color=MUTED)
    ax.set_xlim(0, 0.7)
    ax.tick_params(colors=MUTED)
    ax.grid(axis="x", color=PAPER, lw=0.8)
    ax.set_axisbelow(True)
    for s in ax.spines.values():
        s.set_visible(False)
    ax.legend(frameon=False, fontsize=10, loc="lower right")
    ax.set_title("The model beats the Circle on one fire out of six",
                 fontsize=13, color=INK, pad=12)
    fig.tight_layout()
    fig.savefig(FIGURES / "final_scores.png", dpi=150)
    plt.close(fig)


# ---------------------------------------------------------------- figure 5
def fig_knife_edge():
    """The knife edge: sweep the ignition-probability knob on one fire and
    watch total burned area jump from 'fizzles' to 'explodes'. Needs
    p0_sweep.json from p0_sweep.py; skipped when absent."""
    sweep_path = RESULTS / "p0_sweep.json"
    if not sweep_path.exists():
        print("p0_sweep.json missing — skipping knife-edge figure "
              "(run p0_sweep.py first)")
        return
    sweep = json.loads(sweep_path.read_text())
    p0 = [r["p0"] for r in sweep["runs"]]
    area = [r["final_sim_burned_cells"] * KM2_PER_CELL for r in sweep["runs"]]
    observed = sweep["observed_final_cells"] * KM2_PER_CELL

    fig, ax = plt.subplots(figsize=(8.5, 4.6))
    ax.plot(p0, area, color=FALSE, lw=2, marker="o", ms=5, label="model")
    ax.axhline(observed, color=INK, ls="--", lw=1.5,
               label=f"reality ({observed:.0f} km²)")
    ax.set_yscale("log")
    ax.set_xlabel("ignition-probability knob (p0)", fontsize=10, color=MUTED)
    ax.set_ylabel("final burned area (km², log scale)", fontsize=10, color=MUTED)
    ax.tick_params(colors=MUTED)
    ax.grid(color=PAPER, lw=0.8)
    ax.set_axisbelow(True)
    for s in ax.spines.values():
        s.set_color(MUTED)
        s.set_linewidth(0.6)
    ax.legend(frameon=False, fontsize=10)
    ax.set_title(f"The knife edge — {pretty(sweep['fire'])}, single run per point",
                 fontsize=13, color=INK, pad=12)
    fig.tight_layout()
    fig.savefig(FIGURES / "knife_edge.png", dpi=150)
    plt.close(fig)


def main():
    FIGURES.mkdir(exist_ok=True)
    fig_triptych()
    fig_agreement()
    fig_area_curves()
    fig_scores()
    fig_knife_edge()
    print(f"figures written to {FIGURES}")


if __name__ == "__main__":
    main()
