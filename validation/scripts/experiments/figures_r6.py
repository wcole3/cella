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
    """Per fire: mean IoU bars for persistence, Circle, the three
    pre-registered Ellipse variants, and the post-hoc centred-ellipse
    control (dashed outline — not pre-registered), with the E33 ±1 sd
    band drawn around the Circle bar — the bar a variant has to clear (or
    fall under) to be more than noise, on that fire.
    """
    rows = load("exp41_ellipse.json")
    if rows is None:
        return
    W, H = 1080, 560
    PW, PH = 304, 200
    variants = [("persistence", "PERSISTENCE", MUTED), ("circle", "CIRCLE", INK),
                ("ellipse_era5", "ELLIPSE ERA5", ACCENT), ("ellipse_station", "ELLIPSE STATION", "#4f8a6d"),
                ("ellipse_era5x3", "ELLIPSE ERA5×3", "#a0522d"),
                ("ellipse_era5_centred", "CENTRED (POST-HOC)", "#6b5b95")]
    body = []
    for i, f in enumerate(FIRES):
        px, py = panel_grid(6, 3, PW, PH, 40, 72, 48, 56)[i]
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
            post_hoc = key == "ellipse_era5_centred"
            beyond = key not in ("persistence", "circle") and abs(v - cm) > sd
            fill = f'{col}' if key in ("persistence", "circle") else (col if beyond else SOFT)
            opacity = "0.85" if key in ("persistence", "circle") or beyond else "0.35"
            dash = ' stroke-dasharray="3,2"' if post_hoc else ""
            body.append(f'<rect x="{x:.1f}" y="{sy(v):.1f}" width="{bw:.1f}" height="{sy(0) - sy(v):.1f}" '
                        f'fill="{fill}" fill-opacity="{opacity}" stroke="{INK}" stroke-width="0.6"{dash}/>')
            body.append(text(x + bw / 2, sy(v) - 4, f"{v:.2f}", anchor="middle", size=7))
        for k, (key, v, label, col) in enumerate(vals):
            body.append(text(sx(k) + bw / 2, py + PH - 12, label.split()[0][:4], anchor="middle", size=6.5))
    body.append(text(500, H - 96, "MEAN IOU OVER THE OBSERVATION SERIES, PER FIRE", anchor="middle",
                      extra='letter-spacing="0.10em"'))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.08)"/>', "Circle ±1 sd (E33)"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="{SOFT}" fill-opacity="0.35" stroke="{INK}" stroke-width="0.6"/>', "Ellipse variant, tie"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="{ACCENT}" fill-opacity="0.85" stroke="{INK}" stroke-width="0.6"/>', "Ellipse variant, beyond sd"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="none" stroke="{INK}" stroke-width="0.6" stroke-dasharray="3,2"/>', "post-hoc control (not pre-registered)"),
    ]))
    (FIG / "e41-ellipse-null.svg").write_text(svg(
        "e41", "E41 Ellipse null: mean IoU per fire",
        "Small multiples, one bar chart per fire, of mean IoU for persistence, the Circle, the three pre-registered "
        "Ellipse variants (ERA5, station, ERA5 wind x3) and a post-hoc centred-ellipse control (dashed outline), "
        "with the Circle's E33 ±1 sd band so a real gain or loss over the Circle can be told from noise.",
        W, H, "\n".join(body)))
    print("wrote e41-ellipse-null.svg")


def e42_posterior():
    """Row 1: per fire, the learned p0 posterior (cross-seed median ± sd)
    over days since the first mask, with day 1 and day 5 marked — the
    numbers behind the "does p0 drift the same way on every fire"
    question. Row 2: the model containment curve (each member's FINAL
    genome against the real fire's observed growth) against ICS-209's reported
    percent contained, both on the same day axis, each with its 50 %
    crossing marked — the ICS-209 check. X-axis is clipped per panel to
    keep both crossings legible; the full curves and exact crossing days
    are in the E42 table, not just this figure.
    """
    rows = load("exp42_posterior.json")
    if rows is None:
        return
    W, H = 1080, 900
    PW, PH = 152, 340
    body = []
    grid = panel_grid(12, 6, PW, PH, 24, 96, 48, 40)
    for i, f in enumerate(FIRES):
        r = rows[f]
        obs = r["obs"]
        days = [o["day"] for o in obs]
        med = [o["p0_mean_median"] for o in obs]
        sd = [o["p0_mean_sd"] for o in obs]
        final_day = days[-1]

        # --- Row 1: p0 posterior trajectory ---
        px, py = grid[i]
        lo = min(m - s for m, s in zip(med, sd)) - 0.02
        hi = max(m + s for m, s in zip(med, sd)) + 0.02
        sx = lambda d: px + 20 + d / final_day * (PW - 28)
        sy = lambda v: py + PH - 24 - (v - lo) / (hi - lo) * (PH - 44)
        for g in (lo, (lo + hi) / 2, hi):
            body.append(f'<line x1="{px + 16}" y1="{sy(g):.1f}" x2="{px + PW}" y2="{sy(g):.1f}" '
                        f'stroke="{RULE}" stroke-width="0.8"/>')
            body.append(text(px + 12, sy(g) + 3, f"{g:.2f}", size=6.5, anchor="end"))
        band = (" ".join(f"{sx(d):.1f},{sy(m + s):.1f}" for d, m, s in zip(days, med, sd)) + " " +
                " ".join(f"{sx(d):.1f},{sy(m - s):.1f}" for d, m, s in reversed(list(zip(days, med, sd)))))
        body.append(f'<polygon points="{band}" fill="{ACCENT}" fill-opacity="0.15"/>')
        body.append(f'<polyline points="{" ".join(f"{sx(d):.1f},{sy(m):.1f}" for d, m in zip(days, med))}" '
                    f'fill="none" stroke="{ACCENT}" stroke-width="1.8" stroke-linejoin="round"/>')
        for mark_day in (1, 5):
            if mark_day <= final_day:
                j = min(range(len(days)), key=lambda k: abs(days[k] - mark_day))
                body.append(f'<circle cx="{sx(days[j]):.1f}" cy="{sy(med[j]):.1f}" r="2.6" '
                            f'fill="{PAPER}" stroke="{ACCENT}" stroke-width="1.4"/>')
        body.append(text(px, py - 8, SHORT[f] + ("*" if r["holdout"] else ""),
                          size=12, fill=INK, font=SANS, weight=600))
        body.append(text(px + PW, py - 8, "p0 posterior", anchor="end", size=6.5))
        body.append(text(px, py + PH + 10, "day 0", size=6.5))
        body.append(text(px + PW, py + PH + 10, f"day {final_day:.0f}", anchor="end", size=6.5))

        # --- Row 2: model containment curve vs ICS-209 ---
        px2, py2 = grid[i + 6]
        model_curve = r["model_containment_curve"]
        mdays = [c["day"] for c in model_curve]
        mvals = [c["contained_fraction_model"] for c in model_curve]
        idays = r["ics209"]["days"]
        ivals = [p / 100.0 for p in r["ics209"]["pct_contained"]]
        xmax = max(mdays[-1], r["model_day50"] or 0, r["ics_day50"] or mdays[-1])
        xmax = max(xmax, (r["ics_day50"] or 0) * 1.25, mdays[-1])
        xmax = max(xmax, 1.0)
        sx2 = lambda d: px2 + 20 + min(max(d, 0), xmax) / xmax * (PW - 28)
        sy2 = lambda v: py2 + PH - 24 - v * (PH - 44)
        for g in (0.0, 0.5, 1.0):
            dash = ' stroke-dasharray="2,2"' if g == 0.5 else ""
            body.append(f'<line x1="{px2 + 16}" y1="{sy2(g):.1f}" x2="{px2 + PW}" y2="{sy2(g):.1f}" '
                        f'stroke="{RULE}" stroke-width="0.8"{dash}/>')
            body.append(text(px2 + 12, sy2(g) + 3, f"{g:.1f}", size=6.5, anchor="end"))
        clipped_i = [(d, v) for d, v in zip(idays, ivals) if 0 <= d <= xmax]
        if clipped_i:
            body.append(f'<polyline points="{" ".join(f"{sx2(d):.1f},{sy2(v):.1f}" for d, v in clipped_i)}" '
                        f'fill="none" stroke="{MUTED}" stroke-width="1.6" stroke-linejoin="round"/>')
        clipped_m = [(d, v) for d, v in zip(mdays, mvals) if d <= xmax]
        body.append(f'<polyline points="{" ".join(f"{sx2(d):.1f},{sy2(v):.1f}" for d, v in clipped_m)}" '
                    f'fill="none" stroke="{INK}" stroke-width="1.8" stroke-linejoin="round"/>')
        if r["model_day50"] is not None and r["model_day50"] <= xmax:
            body.append(f'<circle cx="{sx2(r["model_day50"]):.1f}" cy="{sy2(0.5):.1f}" r="3" '
                        f'fill="{INK}"/>')
        if r["ics_day50"] is not None and r["ics_day50"] <= xmax:
            body.append(f'<circle cx="{sx2(r["ics_day50"]):.1f}" cy="{sy2(0.5):.1f}" r="3" '
                        f'fill="{PAPER}" stroke="{MUTED}" stroke-width="1.6"/>')
        body.append(text(px2 + PW, py2 - 8, "containment", anchor="end", size=6.5))
        body.append(text(px2, py2 + PH + 10, "day 0", size=6.5))
        body.append(text(px2 + PW, py2 + PH + 10, f"day {xmax:.0f}", anchor="end", size=6.5))
    body.append(text(540, 30, "ROW 1: LEARNED p0 POSTERIOR (MEDIAN ± SD, 5 SEEDS) · "
                      "ROW 2: MODEL CONTAINMENT CURVE VS ICS-209, DOTS AT 50 % CROSSING",
                      anchor="middle", extra='letter-spacing="0.06em"'))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{ACCENT}" stroke-width="1.8"/>', "p0 posterior median ± sd"),
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{INK}" stroke-width="1.8"/>', "model containment curve"),
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{MUTED}" stroke-width="1.6"/>', "ICS-209 percent contained"),
        (lambda x, y: f'<circle cx="{x + 8}" cy="{y}" r="3" fill="{INK}"/>', "model 50 % day"),
        (lambda x, y: f'<circle cx="{x + 8}" cy="{y}" r="3" fill="{PAPER}" stroke="{MUTED}" stroke-width="1.6"/>', "ICS-209 50 % day"),
    ]))
    (FIG / "e42-posterior.svg").write_text(svg(
        "e42", "E42 posterior trajectories and the ICS-209 containment check",
        "Twelve small multiples, two rows of six fires. Row 1: each fire's learned p0 posterior (median and "
        "cross-seed sd band) over days since the first mask, with day 1 and day 5 marked. Row 2: the model "
        "containment curve, built from every member's final genome run against the fire's observed growth, "
        "against ICS-209's reported percent contained, with a dot at each curve's 50 % crossing day.",
        W, H, "\n".join(body)))
    print("wrote e42-posterior.svg")


if __name__ == "__main__":
    FIG.mkdir(parents=True, exist_ok=True)
    e41_ellipse()
    e42_posterior()
