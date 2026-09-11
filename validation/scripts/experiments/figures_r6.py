#!/usr/bin/env python
"""Figures for the Round 6 experiment files, as standalone SVG (no
dependencies). Reads results/experiments/exp4*.json, writes
experiments/figures/e4*.svg. Re-run after any experiment re-runs.

Style: the same as figures_r5.py — paper #f5f5f5, ink #2d3142, muted
#4f5d75, one accent #eb6c36 per figure, Geist / Geist Mono, 4px grid,
legend as a bottom strip, <title>/<desc> for screen readers.
"""
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
EXP = HERE.parents[1] / "results" / "experiments"
FIG = HERE.parents[1] / "experiments" / "figures"
PAPER, INK, MUTED, SOFT, ACCENT = "#f5f5f5", "#2d3142", "#4f5d75", "#7a8399", "#eb6c36"
RULE = "rgba(45,49,66,0.10)"
FONTS = ("@import url('https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600"
         "&amp;family=Geist+Mono:wght@400;500;600&amp;display=swap');")
SANS, MONO = "'Geist', sans-serif", "'Geist Mono', monospace"
FIRES = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016", "Ferguson_2018", "Pier_2017"]
SHORT = {f: f.split("_")[0] for f in FIRES}
# The E33 five-seed noise floor (sd of mean forecast IoU), the tie bar for
# every delta reported since Round 5.
E33_SD = {"Bear_2020": 0.015, "Brattain_2020": 0.004, "Buck_2017": 0.039,
          "Chimney_2016": 0.012, "Ferguson_2018": 0.007, "Pier_2017": 0.003}


def svg(slug, title, desc, w, h, body):
    return (f'<?xml version="1.0" encoding="UTF-8"?>\n'
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" role="img" '
            f'aria-labelledby="{slug}-title {slug}-desc">\n'
            f'<title id="{slug}-title">{title}</title>\n<desc id="{slug}-desc">{desc}</desc>\n'
            f'<defs><style>{FONTS}</style></defs>\n'
            f'<rect width="100%" height="100%" fill="{PAPER}"/>\n{body}\n</svg>\n')


def text(x, y, s, size=8, fill=MUTED, font=MONO, anchor="start", weight=400, extra=""):
    return (f'<text x="{x}" y="{y}" fill="{fill}" font-size="{size}" font-family="{font}" '
            f'font-weight="{weight}" text-anchor="{anchor}" {extra}>{s}</text>')


def legend(y, w, items):
    """items: list of (svg snippet drawn at x, label). Horizontal strip."""
    out = [f'<line x1="32" y1="{y - 8}" x2="{w - 32}" y2="{y - 8}" stroke="{RULE}" stroke-width="0.8"/>',
           text(32, y + 8, "LEGEND", extra='letter-spacing="0.14em"')]
    x = 112
    for mark, label in items:
        out.append(mark(x, y + 4))
        out.append(text(x + 20, y + 8, label))
        x += 48 + 7 * len(label)
    return "\n".join(out)


def load(name):
    p = EXP / name
    return json.loads(p.read_text()) if p.exists() else None


def panel_grid(n, cols, pw, ph, x_gap, y_gap, top, left):
    """Origins of n small-multiple panels, row-major."""
    return [(left + (i % cols) * (pw + x_gap), top + (i // cols) * (ph + y_gap)) for i in range(n)]


def e41_ellipse():
    """Per fire: mean IoU bars for persistence, Circle, and the three
    Ellipse variants, with the E33 ±1 sd band drawn around the Circle bar
    — the bar a variant has to clear (or fall under) to be more than
    noise, on that fire.
    """
    rows = load("exp41_ellipse.json")
    if rows is None:
        return
    W, H = 1000, 560
    PW, PH = 272, 200
    variants = [("persistence", "PERSISTENCE", MUTED), ("circle", "CIRCLE", INK),
                ("ellipse_era5", "ELLIPSE ERA5", ACCENT), ("ellipse_station", "ELLIPSE STATION", "#4f8a6d"),
                ("ellipse_era5x3", "ELLIPSE ERA5×3", "#a0522d")]
    body = []
    for i, f in enumerate(FIRES):
        px, py = panel_grid(6, 3, PW, PH, 48, 72, 48, 56)[i]
        by_variant = {r["variant"]: r for r in rows if r["fire"] == f}
        circle_row = by_variant["circle"]
        vals = [(key, by_variant[key]["mean_iou"], label, col) for key, label, col in variants
                if by_variant.get(key) and by_variant[key]["mean_iou"] is not None]
        hi = max(v for _, v, _, _ in vals) + 0.06
        sy = lambda v: py + PH - 28 - (v / hi) * (PH - 56)
        sx = lambda k: px + 20 + k * (PW - 32) / len(vals)
        bw = (PW - 32) / len(vals) - 10
        sd = E33_SD[f]
        cm = circle_row["mean_iou"]
        # ±1 sd band around the Circle's own mean, spanning the panel, so
        # every bar's height is read against the noise floor at a glance.
        body.append(f'<rect x="{px + 16}" y="{sy(cm + sd):.1f}" width="{PW - 16}" '
                     f'height="{max(sy(cm - sd) - sy(cm + sd), 1):.1f}" fill="rgba(45,49,66,0.08)"/>')
        for g in (0.0, hi / 2, hi):
            body.append(f'<line x1="{px + 16}" y1="{sy(g):.1f}" x2="{px + PW}" y2="{sy(g):.1f}" '
                        f'stroke="{RULE}" stroke-width="0.8"/>')
            body.append(text(px + 12, sy(g) + 3, f"{g:.2f}", anchor="end"))
        body.append(text(px, py - 8, SHORT[f] + ("*" if circle_row["holdout"] else ""),
                          size=12, fill=INK, font=SANS, weight=600))
        body.append(text(px + PW, py - 8, f"sd {sd:.3f}", anchor="end"))
        for k, (key, v, label, col) in enumerate(vals):
            x = sx(k)
            beyond = key not in ("persistence", "circle") and abs(v - cm) > sd
            fill = f'{col}' if key in ("persistence", "circle") else (col if beyond else SOFT)
            opacity = "0.85" if key in ("persistence", "circle") or beyond else "0.35"
            body.append(f'<rect x="{x:.1f}" y="{sy(v):.1f}" width="{bw:.1f}" height="{sy(0) - sy(v):.1f}" '
                        f'fill="{fill}" fill-opacity="{opacity}" stroke="{INK}" stroke-width="0.6"/>')
            body.append(text(x + bw / 2, sy(v) - 4, f"{v:.2f}", anchor="middle", size=7))
        for k, (key, v, label, col) in enumerate(vals):
            body.append(text(sx(k) + bw / 2, py + PH - 12, label.split()[0][:4], anchor="middle", size=6.5))
    body.append(text(500, H - 96, "MEAN IOU OVER THE OBSERVATION SERIES, PER FIRE", anchor="middle",
                      extra='letter-spacing="0.10em"'))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.08)"/>', "Circle ±1 sd (E33)"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="{SOFT}" fill-opacity="0.35" stroke="{INK}" stroke-width="0.6"/>', "Ellipse variant, tie"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="{ACCENT}" fill-opacity="0.85" stroke="{INK}" stroke-width="0.6"/>', "Ellipse variant, beyond sd"),
    ]))
    (FIG / "e41-ellipse-null.svg").write_text(svg(
        "e41", "E41 Ellipse null: mean IoU per fire",
        "Small multiples, one bar chart per fire, of mean IoU for persistence, the Circle, and the three Ellipse "
        "variants (ERA5, station, ERA5 wind x3), with the Circle's E33 ±1 sd band so a real gain or loss over the "
        "Circle can be told from noise.", W, H, "\n".join(body)))
    print("wrote e41-ellipse-null.svg")


if __name__ == "__main__":
    FIG.mkdir(parents=True, exist_ok=True)
    e41_ellipse()
