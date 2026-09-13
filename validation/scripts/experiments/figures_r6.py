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


def e39_gated_reset():
    """Dot strip per fire, one seed at a time: E38's plain reset and E39's
    area-ratio-gated reset, both as (config − E33 twin) mean consensus
    IoU, joined by a thin line so the same seed's movement from one to
    the other reads at a glance, against the E33 ±1 sd band.
    """
    rows39 = load("exp39_gated_reset.json")
    rows38 = load("exp38_immreset.json")
    noise = load("exp33_noise.json")
    if rows39 is None or rows38 is None or noise is None:
        return
    W, H = 1000, 464
    x0, x1, lo, hi = 200, 920, -0.06, 0.12
    sx = lambda v: x0 + (v - lo) / (hi - lo) * (x1 - x0)
    body = []
    for v in (-0.05, 0.0, 0.05, 0.10):
        w = 1.2 if v == 0.0 else 0.8
        body.append(f'<line x1="{sx(v):.0f}" y1="56" x2="{sx(v):.0f}" y2="376" '
                     f'stroke="{INK if v == 0.0 else RULE}" stroke-width="{w}" '
                     f'stroke-opacity="{0.4 if v == 0.0 else 1}"/>')
        body.append(text(sx(v), 392, f"{v:+.2f}", anchor="middle"))
    body.append(text(560, 408, "E38 (MUTED) / E39 (ACCENT) MINUS E33 TWIN, MEAN CONSENSUS IOU, PER SEED",
                      anchor="middle", extra='letter-spacing="0.08em"'))
    for i, f in enumerate(FIRES):
        y = 80 + i * 52
        sd = E33_SD[f]
        row0 = next((x for x in noise if x["fire"] == f and x["seed"] == 0), None)
        if row0 is None:
            continue
        body.append(text(184, y + 4, SHORT[f] + ("*" if row0["holdout"] else ""),
                          size=12, fill=INK, font=SANS, anchor="end", weight=600))
        body.append(f'<rect x="{sx(-sd):.1f}" y="{y - 14}" width="{sx(sd) - sx(-sd):.1f}" height="28" '
                     f'fill="rgba(45,49,66,0.08)"/>')
        for s_ in range(5):
            b = next((x for x in noise if x["fire"] == f and x["seed"] == s_), None)
            a38 = next((x for x in rows38 if x["fire"] == f and x["seed"] == s_), None)
            a39 = next((x for x in rows39 if x["fire"] == f and x["seed"] == s_), None)
            if not (a38 and a39 and b):
                continue
            base = b["mean_consensus_iou"]
            d38, d39 = a38["mean_consensus_iou"] - base, a39["mean_consensus_iou"] - base
            focal = f == "Buck_2017" and s_ == 3
            y38, y39 = y - 8, y + 8
            body.append(f'<line x1="{sx(d38):.1f}" y1="{y38}" x2="{sx(d39):.1f}" y2="{y39}" '
                        f'stroke="{RULE}" stroke-width="1"/>')
            body.append(f'<circle cx="{sx(d38):.1f}" cy="{y38}" r="4.2" fill="{PAPER}"/>'
                        f'<circle cx="{sx(d38):.1f}" cy="{y38}" r="4.2" fill="rgba(79,93,117,0.22)" '
                        f'stroke="{MUTED}" stroke-width="{1.4 if focal else 1}"/>')
            body.append(f'<circle cx="{sx(d39):.1f}" cy="{y39}" r="4.2" fill="{PAPER}"/>'
                        f'<circle cx="{sx(d39):.1f}" cy="{y39}" r="4.2" fill="rgba(235,108,54,0.22)" '
                        f'stroke="{ACCENT}" stroke-width="{1.4 if focal else 1}"/>')
            if focal:
                body.append(text(sx(d38), y38 - 10, f"BUCK 3 E38 {d38:+.3f}", anchor="middle", fill=MUTED, size=7))
                body.append(text(sx(d39), y39 + 14, f"E39 {d39:+.3f}", anchor="middle", fill=ACCENT, size=7))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<circle cx="{x + 6}" cy="{y}" r="4.2" fill="rgba(79,93,117,0.22)" stroke="{MUTED}"/>', "E38 (plain reset) minus E33"),
        (lambda x, y: f'<circle cx="{x + 6}" cy="{y}" r="4.2" fill="rgba(235,108,54,0.22)" stroke="{ACCENT}"/>', "E39 (gated) minus E33"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.08)"/>', "±1 sd (E33)"),
    ]))
    (FIG / "e39-gated-reset.svg").write_text(svg(
        "e39", "E39 gated immigrant reset: change per seed vs E38",
        "Dot strip per fire, one seed at a time: the change in mean forecast IoU from the E33 twin for the plain "
        "E38 reset (muted) and the area-ratio-gated E39 reset (accent), joined by a thin line so the same seed's "
        "movement from one to the other reads at a glance, against the E33 noise band.",
        W, H, "\n".join(body)))
    print("wrote e39-gated-reset.svg")


def _raw(exp_dir, fire, label):
    """One raw wildfire_smc report, or None if it isn't on disk."""
    p = EXP / exp_dir / f"{fire}_{label}.json"
    return json.loads(p.read_text()) if p.exists() else None


def _seed_mean_series(exp_dir, fire, label_of, key, seeds=5):
    """Mean over `seeds` seeds, per observation window, of `scores[*][key]`
    plus the (shared) hours axis -- `None, None` if any seed is missing.
    Used to build the E40-vs-E33 per-day lines directly from the raw
    per-window reports, the same way E39's evidence table read them,
    since the summary rows in the *.json index only keep the mean over
    the whole series.
    """
    per_seed = []
    for s in range(seeds):
        r = _raw(exp_dir, fire, label_of(s))
        if r is None:
            return None, None
        per_seed.append([sc[key] for sc in r["scores"]])
    n = min(len(s) for s in per_seed)
    hours = [sc["hours"] for sc in _raw(exp_dir, fire, label_of(0))["scores"][:n]]
    mean = [sum(s[i] for s in per_seed) / len(per_seed) for i in range(n)]
    return hours, mean


def e40_observed_immigrants():
    """Top: dot strip per fire, E40 minus its E33 twin, per seed, against
    the E33 ±1 sd band -- the same layout as E39's, one series only.
    Bottom: per-day consensus IoU for Bear and Ferguson, the two fires
    the prediction named as movers -- E40 vs E33 (both the 5-seed mean)
    with the Circle null, read straight from the raw per-window reports.
    """
    rows = load("exp40_observed_immigrants.json")
    noise = load("exp33_noise.json")
    if rows is None or noise is None:
        return
    W, H = 1000, 800
    body = []

    # --- Top: dot strip ---
    x0, x1, lo, hi = 200, 920, -0.06, 0.12
    sx = lambda v: x0 + (v - lo) / (hi - lo) * (x1 - x0)
    for v in (-0.05, 0.0, 0.05, 0.10):
        w = 1.2 if v == 0.0 else 0.8
        body.append(f'<line x1="{sx(v):.0f}" y1="48" x2="{sx(v):.0f}" y2="368" '
                     f'stroke="{INK if v == 0.0 else RULE}" stroke-width="{w}" '
                     f'stroke-opacity="{0.4 if v == 0.0 else 1}"/>')
        body.append(text(sx(v), 384, f"{v:+.2f}", anchor="middle"))
    body.append(text(560, 400, "E40 MINUS E33 TWIN, MEAN CONSENSUS IOU, PER SEED",
                      anchor="middle", extra='letter-spacing="0.08em"'))
    for i, f in enumerate(FIRES):
        y = 72 + i * 50
        sd = E33_SD[f]
        row0 = next((x for x in noise if x["fire"] == f and x["seed"] == 0), None)
        if row0 is None:
            continue
        body.append(text(184, y + 4, SHORT[f] + ("*" if row0["holdout"] else ""),
                          size=12, fill=INK, font=SANS, anchor="end", weight=600))
        body.append(f'<rect x="{sx(-sd):.1f}" y="{y - 12}" width="{sx(sd) - sx(-sd):.1f}" height="24" '
                     f'fill="rgba(45,49,66,0.08)"/>')
        for s_ in range(5):
            b = next((x for x in noise if x["fire"] == f and x["seed"] == s_), None)
            a = next((x for x in rows if x["fire"] == f and x["seed"] == s_), None)
            if not (a and b):
                continue
            d = a["mean_consensus_iou"] - b["mean_consensus_iou"]
            beyond = abs(d) > sd
            fill = ACCENT if beyond else SOFT
            body.append(f'<circle cx="{sx(d):.1f}" cy="{y}" r="4.4" fill="{PAPER}"/>'
                        f'<circle cx="{sx(d):.1f}" cy="{y}" r="4.4" fill="{fill}" fill-opacity="0.4" '
                        f'stroke="{fill}" stroke-width="{1.6 if beyond else 1}"/>')

    # --- Bottom: per-day consensus IoU, Bear and Ferguson, E40 vs E33 vs Circle ---
    PW, PH = 380, 280
    for j, f in enumerate(["Bear_2020", "Ferguson_2018"]):
        px, py = 100 + j * (PW + 60), 460
        hours40, cons40 = _seed_mean_series("exp40_observed_immigrants", f, lambda s: f"observed_seed{s}", "consensus_iou")
        hours33, cons33 = _seed_mean_series("exp33_noise", f, lambda s: f"base_seed{s}", "consensus_iou")
        _, circle = _seed_mean_series("exp33_noise", f, lambda s: f"base_seed{s}", "radial_iou")
        if hours40 is None or hours33 is None:
            continue
        days = [h / 24.0 for h in hours40]
        lo_y = 0.0
        hi_y = max(max(cons40), max(cons33), max(circle)) + 0.05
        sxp = lambda d: px + 24 + d / days[-1] * (PW - 32)
        syp = lambda v: py + PH - 24 - (v - lo_y) / (hi_y - lo_y) * (PH - 40)
        for g in (0.0, hi_y / 2, hi_y):
            body.append(f'<line x1="{px + 20}" y1="{syp(g):.1f}" x2="{px + PW}" y2="{syp(g):.1f}" '
                        f'stroke="{RULE}" stroke-width="0.8"/>')
            body.append(text(px + 16, syp(g) + 3, f"{g:.2f}", anchor="end", size=6.5))
        row0 = next((x for x in noise if x["fire"] == f and x["seed"] == 0), None)
        body.append(text(px, py - 10, SHORT[f] + ("*" if row0 and row0["holdout"] else ""),
                          size=12, fill=INK, font=SANS, weight=600))
        body.append(f'<polyline points="{" ".join(f"{sxp(d):.1f},{syp(v):.1f}" for d, v in zip(days, circle))}" '
                    f'fill="none" stroke="{MUTED}" stroke-width="1.4" stroke-dasharray="3,2" stroke-linejoin="round"/>')
        body.append(f'<polyline points="{" ".join(f"{sxp(d):.1f},{syp(v):.1f}" for d, v in zip(days, cons33))}" '
                    f'fill="none" stroke="{SOFT}" stroke-width="1.8" stroke-linejoin="round"/>')
        body.append(f'<polyline points="{" ".join(f"{sxp(d):.1f},{syp(v):.1f}" for d, v in zip(days, cons40))}" '
                    f'fill="none" stroke="{ACCENT}" stroke-width="2.0" stroke-linejoin="round"/>')
        body.append(text(px, py + PH + 12, "day 0", size=6.5))
        body.append(text(px + PW, py + PH + 12, f"day {days[-1]:.0f}", anchor="end", size=6.5))
    body.append(text(560, 428, "PER-DAY CONSENSUS IOU: E40 (ACCENT) VS E33 (MUTED) VS THE CIRCLE (DASHED), 5-SEED MEAN",
                      anchor="middle", extra='letter-spacing="0.06em"'))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<circle cx="{x + 6}" cy="{y}" r="4.4" fill="{SOFT}" fill-opacity="0.4" stroke="{SOFT}"/>', "E40 minus E33, tie"),
        (lambda x, y: f'<circle cx="{x + 6}" cy="{y}" r="4.4" fill="{ACCENT}" fill-opacity="0.4" stroke="{ACCENT}"/>', "E40 minus E33, beyond sd"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.08)"/>', "±1 sd (E33)"),
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{ACCENT}" stroke-width="2"/>', "E40 per-day"),
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{SOFT}" stroke-width="1.8"/>', "E33 per-day"),
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{MUTED}" stroke-width="1.4" stroke-dasharray="3,2"/>', "Circle"),
    ]))
    (FIG / "e40-observed-immigrants.svg").write_text(svg(
        "e40", "E40 observed-perimeter immigrants: per-seed change and per-day IoU",
        "Top: dot strip per fire, the change in mean forecast IoU from the E33 twin for E40 (immigrants seeded "
        "from the observed perimeter), one dot per seed, against the E33 noise band. Bottom: per-day consensus "
        "IoU for Bear and Ferguson, the two fires the prediction named as movers, comparing E40 and E33 (both the "
        "5-seed mean) with the Circle null.",
        W, H, "\n".join(body)))
    print("wrote e40-observed-immigrants.svg")


def _mean_from_k1(exp_dir, fire, label_of, key, seeds=5):
    """Mean of `scores[1:][*][key]` (k >= 1, so it is comparable with the
    lagged nulls, which are undefined at the very first scored window),
    averaged first within each seed's own series, then across seeds.
    """
    per_seed = []
    for s in range(seeds):
        r = _raw(exp_dir, fire, label_of(s))
        tail = r["scores"][1:]
        per_seed.append(sum(sc[key] for sc in tail) / len(tail))
    return sum(per_seed) / len(per_seed)


def _mean_lagged(exp_dir, fire, label, key):
    """Mean of a lagged-null field over the windows where it exists (k >=
    2); seed-independent (the lagged nulls depend only on the truth), so
    any one report for the fire carries the number.
    """
    r = _raw(exp_dir, fire, label)
    vals = [sc[key] for sc in r["scores"] if sc.get(key) is not None]
    return sum(vals) / len(vals)


def e40b_lagged_nulls():
    """Controller finding (post-hoc, after E40): E40's headline comparison
    used the wrong dummy competitor. Per fire, five bars, all means over
    k >= 1 so they are directly comparable: E33 (no correction), E40 (20%
    of the population state-corrected), E40b (everyone state-corrected),
    lagged persistence, lagged Circle -- the two nulls that see exactly
    what state correction sees (the mask one window back) and no more.
    """
    e33 = load("exp33_noise.json")
    e40 = load("exp40_observed_immigrants.json")
    e40b = load("exp40b_all_state_correction.json")
    if e33 is None or e40 is None or e40b is None:
        return
    W, H = 1080, 560
    PW, PH = 304, 200
    variants = [
        ("e33", "E33 (NONE)", MUTED),
        ("e40", "E40 (IMMIGRANTS)", SOFT),
        ("e40b", "E40b (ALL)", ACCENT),
        ("lp", "LAGGED PERSISTENCE", INK),
        ("lc", "LAGGED CIRCLE", "#4f8a6d"),
    ]
    body = []
    for i, f in enumerate(FIRES):
        px, py = panel_grid(6, 3, PW, PH, 40, 72, 48, 56)[i]
        e33m = _mean_from_k1("exp33_noise", f, lambda s: f"base_seed{s}", "consensus_iou")
        e40m = _mean_from_k1("exp40_observed_immigrants", f, lambda s: f"observed_seed{s}", "consensus_iou")
        e40bm = _mean_from_k1("exp40b_all_state_correction", f, lambda s: f"all_seed{s}", "consensus_iou")
        lp = _mean_lagged("exp40b_all_state_correction", f, "all_seed0", "lagged_persistence_iou")
        lc = _mean_lagged("exp40b_all_state_correction", f, "all_seed0", "lagged_circle_iou")
        vals = {"e33": e33m, "e40": e40m, "e40b": e40bm, "lp": lp, "lc": lc}
        hi = max(vals.values()) + 0.06
        sy = lambda v: py + PH - 28 - (v / hi) * (PH - 56)
        sx = lambda k: px + 20 + k * (PW - 32) / len(variants)
        bw = (PW - 32) / len(variants) - 10
        holdout = f in ("Ferguson_2018", "Pier_2017")
        for g in (0.0, hi / 2, hi):
            body.append(f'<line x1="{px + 16}" y1="{sy(g):.1f}" x2="{px + PW}" y2="{sy(g):.1f}" '
                        f'stroke="{RULE}" stroke-width="0.8"/>')
            body.append(text(px + 12, sy(g) + 3, f"{g:.2f}", anchor="end"))
        body.append(text(px, py - 8, SHORT[f] + ("*" if holdout else ""),
                          size=12, fill=INK, font=SANS, weight=600))
        for k, (key, label, col) in enumerate(variants):
            x = sx(k)
            v = vals[key]
            body.append(f'<rect x="{x:.1f}" y="{sy(v):.1f}" width="{bw:.1f}" height="{sy(0) - sy(v):.1f}" '
                        f'fill="{col}" fill-opacity="0.85" stroke="{INK}" stroke-width="0.6"/>')
            body.append(text(x + bw / 2, sy(v) - 4, f"{v:.2f}", anchor="middle", size=7))
            body.append(text(x + bw / 2, py + PH - 12, label.split()[0][:4], anchor="middle", size=6.5))
    body.append(text(500, H - 96, "MEAN CONSENSUS IOU OVER k >= 1 (COMPARABLE TO THE LAGGED NULLS), PER FIRE",
                      anchor="middle", extra='letter-spacing="0.10em"'))
    body.append(legend(H - 28, W, [(lambda x, y, c=col: f'<rect x="{x}" y="{y - 6}" width="16" height="12" '
                                     f'fill="{c}" fill-opacity="0.85" stroke="{INK}" stroke-width="0.6"/>', label)
                                    for _, label, col in variants]))
    (FIG / "e40b-lagged-nulls.svg").write_text(svg(
        "e40b", "E40b vs the lagged nulls, per fire",
        "Small multiples, one bar chart per fire: mean consensus IoU over the windows with a lagged null (k >= 1), "
        "for E33 (no state correction), E40 (20% of the population state-corrected), E40b (everyone "
        "state-corrected), lagged persistence (yesterday's mask, unchanged) and the lagged Circle (yesterday's "
        "mask, grown to today's true area) -- the two dummy competitors that see exactly what state correction "
        "sees and no more. Every fire's ensemble configuration loses to both lagged nulls.",
        W, H, "\n".join(body)))
    print("wrote e40b-lagged-nulls.svg")


def e43_spot_illuminate():
    """Six MAP-Elites archives, one per fire, same layout as E37's own
    figure (figures_r5.py's e37_illuminate), but with the spotting genes
    added (SMC_SPOT=1): the filled cells are E43's own reachable region.
    A dashed stepped line traces the *top* of E37's reachable region at
    each growth bin -- its wedge boundary, spotting off -- so a reader can
    see at a glance whether spotting pushes elongation past what the
    model could already reach at that size. The observed daily dots are
    E43's own (same five days of weather and the same truth as E37, so
    they land in the same place).
    """
    rows = load("exp43_spot_illuminate.json")
    if rows is None:
        return
    W, H = 1000, 560
    PW, PH = 272, 200
    body = []
    for i, f in enumerate(FIRES):
        px, py = panel_grid(6, 3, PW, PH, 48, 40, 48, 56)[i]
        r = [x for x in rows if x["fire"] == f][0]
        rep = json.loads((EXP / "exp43_spot_illuminate" / f"{f}.json").read_text())
        a = rep["archive"]
        (gx, gy), (rx, ry) = a["dims"], a["ranges"]
        cw, ch = (PW - 40) / gx, (PH - 40) / gy
        body.append(text(px, py - 8, f"{SHORT[f]} · {a['stats']['elites']} elites, coverage {a['stats']['coverage']:.2f}",
                          size=12, fill=INK, font=SANS, weight=600))
        body.append(f'<rect x="{px + 32}" y="{py}" width="{PW - 40}" height="{PH - 40}" fill="none" stroke="{RULE}"/>')
        # E43's own filled cells (spotting on).
        for e in a["elites"]:
            cx, cy = e["coords"]
            body.append(f'<rect x="{px + 32 + cx * cw:.1f}" y="{py + (gy - 1 - cy) * ch:.1f}" '
                        f'width="{cw:.1f}" height="{ch:.1f}" fill="rgba(45,49,66,0.28)"/>')
        # E37's own wedge boundary (spotting off): for each growth column
        # E37 reached, the top of its highest filled cell.
        e37_path = EXP / "exp37_illuminate" / f"{f}.json"
        if e37_path.exists():
            e37_archive = json.loads(e37_path.read_text())["archive"]
            col_max = {}
            for e in e37_archive["elites"]:
                cx37, cy37 = e["coords"]
                col_max[cx37] = max(col_max.get(cx37, -1), cy37)
            pts = []
            for cx37 in sorted(col_max):
                top_y = py + (gy - 1 - col_max[cx37]) * ch
                x_left = px + 32 + cx37 * cw
                x_right = px + 32 + (cx37 + 1) * cw
                pts.append(f"{x_left:.1f},{top_y:.1f}")
                pts.append(f"{x_right:.1f},{top_y:.1f}")
            if pts:
                body.append(f'<polyline points="{" ".join(pts)}" fill="none" stroke="{ACCENT}" '
                            f'stroke-width="1.6" stroke-dasharray="4,2" stroke-linejoin="miter"/>')
        for (hh, g, el) in rep["observed"]:
            ox = px + 32 + (g - rx[0]) / (rx[1] - rx[0]) * (PW - 40)
            oy = py + (PH - 40) - (el - ry[0]) / (ry[1] - ry[0]) * (PH - 40)
            ox, oy = min(max(ox, px + 32), px + PW - 8), min(max(oy, py), py + PH - 40)
            body.append(f'<circle cx="{ox:.1f}" cy="{oy:.1f}" r="4" fill="{PAPER}"/>'
                        f'<circle cx="{ox:.1f}" cy="{oy:.1f}" r="4" fill="rgba(45,49,66,0.15)" stroke="{INK}" stroke-width="1.2"/>')
        body.append(text(px + 32, py + PH - 24, f"{rx[0]:.2f}", anchor="start"))
        body.append(text(px + PW - 8, py + PH - 24, f"growth {rx[1]:.2f}", anchor="end"))
        body.append(text(px + 28, py + PH - 40, f"{ry[0]:.0f}", anchor="end"))
        body.append(text(px + 28, py + 8, f"{ry[1]:.0f}", anchor="end"))
        body.append(text(px + 28, py + PH / 2 - 20, "elong.", anchor="end"))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.28)"/>',
         "E43 (spotting on): a knob setting the model can produce"),
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{ACCENT}" stroke-width="1.6" stroke-dasharray="4,2"/>',
         "E37 wedge boundary (spotting off)"),
        (lambda x, y: f'<circle cx="{x + 8}" cy="{y}" r="4" fill="rgba(45,49,66,0.15)" stroke="{INK}" stroke-width="1.2"/>',
         "the observed fire, one dot per day"),
    ]))
    (FIG / "e43-spot-illuminate.svg").write_text(svg(
        "e43", "E43 does spotting extend the reachable shape region",
        "Six MAP-Elites archives, one per fire, over growth and elongation of the burned area after five days with "
        "spotting genes added (SMC_SPOT=1), with E37's own reachable-region boundary (spotting off) overlaid as a "
        "dashed line and the observed fire's growth and elongation marked day by day.",
        W, H, "\n".join(body)))
    print("wrote e43-spot-illuminate.svg")


def e30a():
    """E30a fix round 2: elongation (E12's measure) vs. burned CELL COUNT
    (not a grid fraction — see the experiment file for why), one panel per
    wind speed, one line per spread rule (Bernoulli vs. arrival), on the
    upwind-ignition 900x300 domain. Exponential wind law, p0 = 0.12,
    jitter 0.2. The prediction this figure checks at a glance: both lines
    should stay flat (self-similar) at every wind, now that the domain no
    longer lets the fire's head reach a boundary within these checkpoints.
    Each point is the mean over 3 seeds; the shaded band is the seed
    min-max; a hollow point marks a checkpoint with `boundary_contact` —
    read those as unreliable, not as data (mild/calm winds still touch the
    *upwind* edge at the larger checkpoints; that does not affect the
    downwind-driven shape at higher wind, per the experiment file).
    """
    data = load("exp30a_arrival_flat.json")
    if data is None:
        return
    rows = [r for r in data["illuminate"] if r["wind_law"] == "exponential" and r["p0"] == 0.12 and r["jitter"] == 0.2]
    winds = sorted({r["wind_ms"] for r in rows})
    sizes = sorted({r["cells"] for r in rows})
    rules = [("bernoulli", "BERNOULLI", MUTED), ("arrival", "ARRIVAL", ACCENT)]

    W, H = 1000, 320
    PW, PH = 200, 200
    body = []
    grid = panel_grid(len(winds), len(winds), PW, PH, 32, 0, 56, 48)
    for i, wind in enumerate(winds):
        px, py = grid[i]
        # y-axis fixed 1.0-2.0 across all four panels so the eye can compare
        # panels directly; the model never approaches MAX_ELONGATION (10) here.
        lo, hi = 1.0, 2.0
        sx = lambda s: px + 24 + (s - sizes[0]) / (sizes[-1] - sizes[0]) * (PW - 32)
        sy = lambda v: py + PH - 24 - (min(max(v, lo), hi) - lo) / (hi - lo) * (PH - 40)
        for g in (1.0, 1.3, 1.5, 2.0):
            body.append(f'<line x1="{px + 16}" y1="{sy(g):.1f}" x2="{px + PW}" y2="{sy(g):.1f}" '
                        f'stroke="{RULE}" stroke-width="0.8"/>')
            body.append(text(px + 12, sy(g) + 3, f"{g:.1f}", size=6.5, anchor="end"))
        body.append(text(px, py - 8, f"{wind:.0f} m/s", size=12, fill=INK, font=SANS, weight=600))
        for key, label, col in rules:
            by_size = {}
            for r in rows:
                if r["wind_ms"] == wind and r["spread"] == key:
                    by_size.setdefault(r["cells"], []).append((r["elongation"], r["boundary_contact"]))
            pts_mean, pts_lo, pts_hi, pts_contact = [], [], [], []
            for s in sizes:
                vs = by_size.get(s, [])
                if not vs:
                    continue
                es = [e for e, _ in vs]
                pts_mean.append((s, sum(es) / len(es)))
                pts_lo.append((s, min(es)))
                pts_hi.append((s, max(es)))
                pts_contact.append((s, any(c for _, c in vs)))
            if not pts_mean:
                continue
            band = (" ".join(f"{sx(s):.1f},{sy(v):.1f}" for s, v in pts_hi) + " " +
                    " ".join(f"{sx(s):.1f},{sy(v):.1f}" for s, v in reversed(pts_lo)))
            body.append(f'<polygon points="{band}" fill="{col}" fill-opacity="0.12"/>')
            body.append(f'<polyline points="{" ".join(f"{sx(s):.1f},{sy(v):.1f}" for s, v in pts_mean)}" '
                        f'fill="none" stroke="{col}" stroke-width="1.8" stroke-linejoin="round"/>')
            for (s, v), (_, contact) in zip(pts_mean, pts_contact):
                fill = PAPER if not contact else "none"
                dash = '' if not contact else ' stroke-dasharray="1.5,1.2"'
                body.append(f'<circle cx="{sx(s):.1f}" cy="{sy(v):.1f}" r="2.6" fill="{fill}" '
                            f'stroke="{col}" stroke-width="1.4"{dash}/>')
        for s in sizes:
            body.append(text(sx(s), py + PH - 8, f"{s // 1000}k", size=6.5, anchor="middle"))
    body.append(text(500, 24, "E12 ELONGATION VS. BURNED CELL COUNT, ONE PANEL PER WIND SPEED (EXPONENTIAL LAW)",
                      anchor="middle", extra='letter-spacing="0.10em"'))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{MUTED}" stroke-width="1.8"/>',
         "bernoulli (probability rule)"),
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{ACCENT}" stroke-width="1.8"/>',
         "arrival (minimum travel time)"),
        (lambda x, y: f'<circle cx="{x + 8}" cy="{y}" r="2.6" fill="none" stroke="{INK}" stroke-width="1.4" stroke-dasharray="1.5,1.2"/>',
         "hollow/dashed = boundary contact (unreliable)"),
    ]))
    (FIG / "e30a-arrival-flat.svg").write_text(svg(
        "e30a", "E30a arrival-time kernel: elongation vs. size",
        "Four panels, one per wind speed (0, 2, 5, 8 m/s), each plotting E12's elongation measure against the "
        "burned cell count (2000, 5000, 10000, 20000) for a point ignition upwind on a 900x300 uniform grid, "
        "one line for the Bernoulli spread rule and one for the arrival-time rule (exponential wind law), mean "
        "over 3 seeds with a min-max band; hollow dashed points mark boundary contact.",
        W, H, "\n".join(body)))
    print("wrote e30a-arrival-flat.svg")


def e30():
    """E30/Task 8: two panels stacked. Top: six MAP-Elites archives, same
    layout as E43's own figure (`e43_spot_illuminate`), but E30b's kernel
    is `SMC_SPREAD=arrival SMC_WIND_LAW=rear_focus` instead of spotting --
    the filled cells are what that kernel can reach in 5 days; the dashed
    stepped line is E37's own reachable-region boundary (bernoulli,
    exponential law) for comparison; the observed daily dots are the same
    as E37's (same truth, same weather). Bottom: a per-fire, per-seed dot
    strip of (E30 forecast) minus (E33 twin) mean consensus IoU, against
    the E33 +-1 sd band -- the forecast half of the acceptance test, the
    same style as `e39_gated_reset` but with only one series to plot.
    """
    illum = load("exp30_arrival_illuminate.json")
    fires_rows = load("exp30_arrival_fires.json")
    noise = load("exp33_noise.json")
    if illum is None or fires_rows is None or noise is None:
        return
    W, H = 1000, 1080
    PW, PH = 272, 200
    body = []

    # --- Top: six archive panels, E30's kernel filled, E37's boundary dashed.
    for i, f in enumerate(FIRES):
        px, py = panel_grid(6, 3, PW, PH, 48, 40, 48, 56)[i]
        r = [x for x in illum if x["fire"] == f][0]
        rep = json.loads((EXP / "exp30_arrival_illuminate" / f"{f}.json").read_text())
        a = rep["archive"]
        (gx, gy), (rx, ry) = a["dims"], a["ranges"]
        cw, ch = (PW - 40) / gx, (PH - 40) / gy
        body.append(text(px, py - 8, f"{SHORT[f]} · {a['stats']['elites']} elites, coverage {a['stats']['coverage']:.2f}",
                          size=12, fill=INK, font=SANS, weight=600))
        body.append(f'<rect x="{px + 32}" y="{py}" width="{PW - 40}" height="{PH - 40}" fill="none" stroke="{RULE}"/>')
        for e in a["elites"]:
            cx, cy = e["coords"]
            body.append(f'<rect x="{px + 32 + cx * cw:.1f}" y="{py + (gy - 1 - cy) * ch:.1f}" '
                        f'width="{cw:.1f}" height="{ch:.1f}" fill="rgba(235,108,54,0.28)"/>')
        e37_path = EXP / "exp37_illuminate" / f"{f}.json"
        if e37_path.exists():
            e37_archive = json.loads(e37_path.read_text())["archive"]
            col_max = {}
            for e in e37_archive["elites"]:
                cx37, cy37 = e["coords"]
                col_max[cx37] = max(col_max.get(cx37, -1), cy37)
            pts = []
            for cx37 in sorted(col_max):
                top_y = py + (gy - 1 - col_max[cx37]) * ch
                x_left = px + 32 + cx37 * cw
                x_right = px + 32 + (cx37 + 1) * cw
                pts.append(f"{x_left:.1f},{top_y:.1f}")
                pts.append(f"{x_right:.1f},{top_y:.1f}")
            if pts:
                body.append(f'<polyline points="{" ".join(pts)}" fill="none" stroke="{MUTED}" '
                            f'stroke-width="1.6" stroke-dasharray="4,2" stroke-linejoin="miter"/>')
        for (hh, g, el) in rep["observed"]:
            ox = px + 32 + (g - rx[0]) / (rx[1] - rx[0]) * (PW - 40)
            oy = py + (PH - 40) - (el - ry[0]) / (ry[1] - ry[0]) * (PH - 40)
            ox, oy = min(max(ox, px + 32), px + PW - 8), min(max(oy, py), py + PH - 40)
            body.append(f'<circle cx="{ox:.1f}" cy="{oy:.1f}" r="4" fill="{PAPER}"/>'
                        f'<circle cx="{ox:.1f}" cy="{oy:.1f}" r="4" fill="rgba(45,49,66,0.15)" stroke="{INK}" stroke-width="1.2"/>')
        body.append(text(px + 32, py + PH - 24, f"{rx[0]:.2f}", anchor="start"))
        body.append(text(px + PW - 8, py + PH - 24, f"growth {rx[1]:.2f}", anchor="end"))
        body.append(text(px + 28, py + PH - 40, f"{ry[0]:.0f}", anchor="end"))
        body.append(text(px + 28, py + 8, f"{ry[1]:.0f}", anchor="end"))
        body.append(text(px + 28, py + PH / 2 - 20, "elong.", anchor="end"))
    body.append(legend(524, W, [
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(235,108,54,0.28)"/>',
         "E30/E37b (arrival, rear_focus): a knob setting the model can produce"),
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{MUTED}" stroke-width="1.6" stroke-dasharray="4,2"/>',
         "E37 wedge boundary (bernoulli, exponential)"),
        (lambda x, y: f'<circle cx="{x + 8}" cy="{y}" r="4" fill="rgba(45,49,66,0.15)" stroke="{INK}" stroke-width="1.2"/>',
         "the observed fire, one dot per day"),
    ]))
    body.append(text(500, 24, "TOP: E30/E37B ARCHIVES (ARRIVAL, REAR_FOCUS) VS. E37'S OWN WEDGE BOUNDARY",
                      anchor="middle", extra='letter-spacing="0.08em"'))

    # --- Bottom: per-fire, per-seed delta strip, E30 forecast minus E33 twin.
    yoff = 600
    x0, x1, lo, hi = 200, 920, -0.10, 0.06
    sx = lambda v: x0 + (v - lo) / (hi - lo) * (x1 - x0)
    for v in (-0.08, -0.04, 0.0, 0.04):
        w = 1.2 if v == 0.0 else 0.8
        body.append(f'<line x1="{sx(v):.0f}" y1="{yoff + 24}" x2="{sx(v):.0f}" y2="{yoff + 344}" '
                     f'stroke="{INK if v == 0.0 else RULE}" stroke-width="{w}" '
                     f'stroke-opacity="{0.4 if v == 0.0 else 1}"/>')
        body.append(text(sx(v), yoff + 360, f"{v:+.2f}", anchor="middle"))
    body.append(text(560, yoff + 376, "E30 (ARRIVAL, REAR_FOCUS) MINUS E33 TWIN, MEAN CONSENSUS IOU, PER SEED",
                      anchor="middle", extra='letter-spacing="0.08em"'))
    for i, f in enumerate(FIRES):
        y = yoff + 48 + i * 52
        sd = E33_SD[f]
        row0 = next((x for x in noise if x["fire"] == f and x["seed"] == 0), None)
        if row0 is None:
            continue
        body.append(text(184, y + 4, SHORT[f] + ("*" if row0["holdout"] else ""),
                          size=12, fill=INK, font=SANS, anchor="end", weight=600))
        body.append(f'<rect x="{sx(-sd):.1f}" y="{y - 12}" width="{sx(sd) - sx(-sd):.1f}" height="24" '
                     f'fill="rgba(45,49,66,0.08)"/>')
        deltas = []
        for s_ in range(5):
            b = next((x for x in noise if x["fire"] == f and x["seed"] == s_), None)
            a30 = next((x for x in fires_rows if x["fire"] == f and x["seed"] == s_), None)
            if not (a30 and b):
                continue
            d = a30["mean_consensus_iou"] - b["mean_consensus_iou"]
            deltas.append(d)
            body.append(f'<circle cx="{sx(d):.1f}" cy="{y}" r="4.2" fill="{PAPER}"/>'
                        f'<circle cx="{sx(d):.1f}" cy="{y}" r="4.2" fill="rgba(235,108,54,0.22)" '
                        f'stroke="{ACCENT}" stroke-width="1"/>')
        if deltas:
            m = sum(deltas) / len(deltas)
            body.append(f'<line x1="{sx(m):.1f}" y1="{y - 16}" x2="{sx(m):.1f}" y2="{y + 16}" '
                        f'stroke="{INK}" stroke-width="1.6"/>')
    body.append(legend(yoff + 420, W, [
        (lambda x, y: f'<circle cx="{x + 6}" cy="{y}" r="4.2" fill="rgba(235,108,54,0.22)" stroke="{ACCENT}"/>', "E30 minus E33, one dot per seed"),
        (lambda x, y: f'<line x1="{x}" y1="{y - 6}" x2="{x}" y2="{y + 6}" stroke="{INK}" stroke-width="1.6"/>', "mean of the 5 seeds"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.08)"/>', "±1 sd (E33)"),
    ]))
    (FIG / "e30-arrival-fires.svg").write_text(svg(
        "e30", "E30 arrival-time kernel on the six fires",
        "Top: six MAP-Elites archives under the arrival/rear_focus kernel (E37b), E37's own reachable-region "
        "boundary overlaid as a dashed line, the observed fire's growth and elongation marked day by day. Bottom: "
        "a dot strip per fire, one seed at a time, of the E30 forecast's mean consensus IoU minus its E33 twin, "
        "against the E33 noise band.",
        W, H, "\n".join(body)))
    print("wrote e30-arrival-fires.svg")


def _ellipse_mean(raw_path):
    """Mean ellipse_iou over a raw report's score series, or None if the
    field is absent (older reports, e.g. E33/E30, predate it)."""
    if not raw_path.exists():
        return None
    rep = json.loads(raw_path.read_text())
    vals = [s["ellipse_iou"] for s in rep["scores"] if "ellipse_iou" in s]
    return sum(vals) / len(vals) if vals else None


def _wind_rot_median(raw_path):
    """Median final wind_rot_deg across a raw report's final_genomes, or
    None if the gene wasn't in this run (Arm A)."""
    if not raw_path.exists():
        return None
    rep = json.loads(raw_path.read_text())
    vals = [g["wind_rot_deg"]["Float"] for g in rep["final_genomes"] if "wind_rot_deg" in g]
    if not vals:
        return None
    vals.sort()
    n = len(vals)
    mid = n // 2
    return vals[mid] if n % 2 else (vals[mid - 1] + vals[mid]) / 2.0


def e30b():
    """E30b/Task 10 pilot: six per-fire bar panels (E33 seed 0 / E30
    seed 0 / Arm A / Arm B mean consensus IoU), a dashed Circle line and
    an E33 +-1 sd band in each, plus a bottom strip of Arm B's learned
    wind_rot_deg median per fire against a +-90 degree axis.
    """
    noise = load("exp33_noise.json")
    e30_rows = load("exp30_arrival_fires.json")
    pilot = load("exp30b_arrival_x4_pilot.json")
    if noise is None or e30_rows is None or pilot is None:
        return
    raw = EXP / "exp30b_arrival_x4_pilot"
    by_fire_seed0 = lambda rows: {r["fire"]: r for r in rows if r["seed"] == 0}
    e33, e30 = by_fire_seed0(noise), by_fire_seed0(e30_rows)
    arm_a = {r["fire"]: r for r in pilot if r["config"] == "armA_seed0"}
    arm_b = {r["fire"]: r for r in pilot if r["config"] == "armB_seed0"}

    W, H = 1000, 840
    PW, PH = 280, 220
    body = []
    series = [("E33", e33, MUTED, 0.55), ("E30", e30, MUTED, 0.85),
              ("A", arm_a, ACCENT, 0.35), ("B", arm_b, ACCENT, 0.85)]
    ymax = 0.75
    for i, f in enumerate(FIRES):
        px, py = panel_grid(6, 3, PW, PH, 40, 40, 56, 44)[i]
        sy = lambda v: py + PH - 32 - min(v, ymax) / ymax * (PH - 56)
        body.append(f'<line x1="{px + 32}" y1="{py + 8}" x2="{px + 32}" y2="{py + PH - 32}" stroke="{RULE}"/>')
        body.append(f'<line x1="{px + 32}" y1="{py + PH - 32}" x2="{px + PW}" y2="{py + PH - 32}" stroke="{RULE}"/>')
        r33 = e33.get(f)
        if r33 is None:
            continue
        sd = E33_SD[f]
        base = r33["mean_consensus_iou"]
        body.append(f'<rect x="{px + 32}" y="{sy(base + sd):.1f}" width="{PW - 32}" '
                     f'height="{max(sy(base - sd) - sy(base + sd), 0.5):.1f}" fill="rgba(45,49,66,0.08)"/>')
        circle = r33["mean_radial_iou"]
        body.append(f'<line x1="{px + 32}" y1="{sy(circle):.1f}" x2="{px + PW}" y2="{sy(circle):.1f}" '
                     f'stroke="{INK}" stroke-width="1.2" stroke-dasharray="4,2" stroke-opacity="0.6"/>')
        body.append(text(px, py - 4, SHORT[f] + ("*" if r33["holdout"] else ""),
                          size=13, fill=INK, font=SANS, weight=600))
        bw = (PW - 48) / 4
        for k, (label, rows, col, op) in enumerate(series):
            row = rows.get(f)
            if row is None:
                continue
            v = row["mean_consensus_iou"]
            x = px + 40 + k * bw
            body.append(f'<rect x="{x:.1f}" y="{sy(v):.1f}" width="{bw - 6:.1f}" '
                        f'height="{py + PH - 32 - sy(v):.1f}" fill="{col}" fill-opacity="{op}" '
                        f'stroke="{col}" stroke-width="1"/>')
            body.append(text(x + (bw - 6) / 2, py + PH - 20, label, size=7, anchor="middle"))
        body.append(text(px + 28, py + 4, f"{ymax:.2f}", size=6.5, anchor="end"))
        body.append(text(px + 28, py + PH - 34, "0", size=6.5, anchor="end"))
    body.append(text(500, 24, "MEAN CONSENSUS IOU, SEED 0: E33 / E30 / ARM A / ARM B, VS. CIRCLE (DASHED) AND E33 +-1 SD",
                      anchor="middle", extra='letter-spacing="0.07em"'))
    body.append(legend(572, W, [
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="14" height="12" fill="{MUTED}" fill-opacity="0.55"/>', "E33 seed 0"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="14" height="12" fill="{MUTED}" fill-opacity="0.85"/>', "E30 seed 0"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="14" height="12" fill="{ACCENT}" fill-opacity="0.35"/>', "Arm A"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="14" height="12" fill="{ACCENT}" fill-opacity="0.85"/>', "Arm B"),
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{INK}" stroke-width="1.2" stroke-dasharray="4,2"/>', "Circle"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.08)"/>', "E33 +-1 sd"),
    ]))

    # --- Bottom strip: Arm B's learned wind_rot_deg median per fire, one
    # row per fire (same layout as e30()/e39_gated_reset()'s dot strips).
    yoff = 630
    x0, x1, lo, hi = 200, 920, -90.0, 90.0
    sx = lambda v: x0 + (v - lo) / (hi - lo) * (x1 - x0)
    for v in (-90, -45, 0, 45, 90):
        w = 1.2 if v == 0 else 0.8
        body.append(f'<line x1="{sx(v):.0f}" y1="{yoff + 8}" x2="{sx(v):.0f}" y2="{yoff + 8 + 6 * 26}" '
                     f'stroke="{INK if v == 0 else RULE}" stroke-width="{w}" '
                     f'stroke-opacity="{0.4 if v == 0 else 1}"/>')
        body.append(text(sx(v), yoff + 8 + 6 * 26 + 16, f"{v:+.0f}°", anchor="middle"))
    body.append(text(560, yoff - 8, "ARM B: LEARNED wind_rot_deg MEDIAN PER FIRE (FINAL GENOMES)",
                      anchor="middle", extra='letter-spacing="0.07em"'))
    for i, f in enumerate(FIRES):
        y = yoff + 8 + i * 26 + 13
        r33 = e33.get(f)
        body.append(text(184, y + 4, SHORT[f] + ("*" if r33 and r33["holdout"] else ""),
                          size=11, fill=INK, font=SANS, anchor="end", weight=600))
        rot = _wind_rot_median(raw / f"{f}_armB_seed0.json")
        if rot is None:
            continue
        body.append(f'<circle cx="{sx(rot):.1f}" cy="{y}" r="5" fill="{PAPER}"/>'
                    f'<circle cx="{sx(rot):.1f}" cy="{y}" r="5" fill="rgba(235,108,54,0.28)" '
                    f'stroke="{ACCENT}" stroke-width="1.4"/>')
        body.append(text(sx(rot), y - 10, f"{rot:+.1f}°", anchor="middle", size=7, fill=MUTED))
    (FIG / "e30b-pilot.svg").write_text(svg(
        "e30b", "E30b pilot: uncapped clock and a wind-direction gene",
        "Top: six per-fire bar panels comparing mean consensus IoU (seed 0) across E33, E30, and this pilot's "
        "Arm A / Arm B, with the Circle null as a dashed line and the E33 noise band shaded. Bottom: Arm B's "
        "learned wind_rot_deg median per fire, against a +-90 degree axis.",
        W, H, "\n".join(body)))
    print("wrote e30b-pilot.svg")


if __name__ == "__main__":
    FIG.mkdir(parents=True, exist_ok=True)
    e41_ellipse()
    e42_posterior()
    e39_gated_reset()
    e40_observed_immigrants()
    e40b_lagged_nulls()
    e43_spot_illuminate()
    e30a()
    e30()
    e30b()
