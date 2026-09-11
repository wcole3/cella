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
    """E30a: elongation (E12's measure) vs. burned-area size, one panel per
    wind speed, one line per spread rule (Bernoulli vs. arrival). The
    prediction this figure checks at a glance: the Bernoulli line should
    fall from > 1.5 (2 % burned) to < 1.3 (20 % burned) at 8 m/s; the
    arrival line should stay flat (within +-0.15) at every wind. Each point
    is the mean over 3 seeds; the shaded band is the seed min-max.
    """
    data = load("exp30a_arrival_flat.json")
    if data is None:
        return
    rows = data["illuminate"]
    winds = sorted({r["wind_ms"] for r in rows})
    sizes = sorted({r["size_frac"] for r in rows})
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
                    by_size.setdefault(r["size_frac"], []).append(r["elongation"])
            pts_mean, pts_lo, pts_hi = [], [], []
            for s in sizes:
                vs = by_size.get(s, [])
                if not vs:
                    continue
                pts_mean.append((s, sum(vs) / len(vs)))
                pts_lo.append((s, min(vs)))
                pts_hi.append((s, max(vs)))
            if not pts_mean:
                continue
            band = (" ".join(f"{sx(s):.1f},{sy(v):.1f}" for s, v in pts_hi) + " " +
                    " ".join(f"{sx(s):.1f},{sy(v):.1f}" for s, v in reversed(pts_lo)))
            body.append(f'<polygon points="{band}" fill="{col}" fill-opacity="0.12"/>')
            body.append(f'<polyline points="{" ".join(f"{sx(s):.1f},{sy(v):.1f}" for s, v in pts_mean)}" '
                        f'fill="none" stroke="{col}" stroke-width="1.8" stroke-linejoin="round"/>')
            for s, v in pts_mean:
                body.append(f'<circle cx="{sx(s):.1f}" cy="{sy(v):.1f}" r="2.6" fill="{PAPER}" '
                            f'stroke="{col}" stroke-width="1.4"/>')
        for s in sizes:
            body.append(text(sx(s), py + PH - 8, f"{s * 100:.0f}%", size=6.5, anchor="middle"))
    body.append(text(500, 24, "E12 ELONGATION VS. BURNED-AREA SIZE, ONE PANEL PER WIND SPEED",
                      anchor="middle", extra='letter-spacing="0.10em"'))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{MUTED}" stroke-width="1.8"/>',
         "bernoulli (probability rule)"),
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{ACCENT}" stroke-width="1.8"/>',
         "arrival (heat-accumulator rule)"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="{MUTED}" fill-opacity="0.12"/>',
         "3-seed min-max band"),
    ]))
    (FIG / "e30a-arrival-flat.svg").write_text(svg(
        "e30a", "E30a arrival-time kernel: elongation vs. size",
        "Four panels, one per wind speed (0, 2, 5, 8 m/s), each plotting E12's elongation measure against the "
        "burned-area size (2, 5, 10, 20 percent) for a point ignition on a 400x400 uniform grid, one line for "
        "the Bernoulli spread rule and one for the arrival-time rule, mean over 3 seeds with a min-max band.",
        W, H, "\n".join(body)))
    print("wrote e30a-arrival-flat.svg")


if __name__ == "__main__":
    FIG.mkdir(parents=True, exist_ok=True)
    e41_ellipse()
    e42_posterior()
    e39_gated_reset()
    e40_observed_immigrants()
    e40b_lagged_nulls()
    e43_spot_illuminate()
    e30a()
