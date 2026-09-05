#!/usr/bin/env python
"""Figures for the Round 5 experiment files, as standalone SVG (no
dependencies). Reads results/experiments/exp3*.json, writes
experiments/figures/e3*.svg. Re-run after any experiment re-runs.

Style: the diagram-design skill's neutral skin — paper #f5f5f5, ink
#2d3142, muted #4f5d75, one accent #eb6c36 per figure, Geist / Geist Mono,
4px grid, legend as a bottom strip, <title>/<desc> for screen readers.
"""
import json, statistics as st
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


def e33_noise():
    rows = load("exp33_noise.json")
    if rows is None:
        return
    W, H = 1000, 464
    x0, x1, lo, hi = 200, 920, 0.30, 0.70
    sx = lambda v: x0 + (v - lo) / (hi - lo) * (x1 - x0)
    body = []
    for v in (0.3, 0.4, 0.5, 0.6, 0.7):
        body.append(f'<line x1="{sx(v):.0f}" y1="56" x2="{sx(v):.0f}" y2="376" stroke="{RULE}" stroke-width="0.8"/>')
        body.append(text(sx(v), 392, f"{v:.1f}", anchor="middle"))
    body.append(text(560, 408, "MEAN ONE-WINDOW-AHEAD CONSENSUS IOU", anchor="middle", extra='letter-spacing="0.10em"'))
    for i, f in enumerate(FIRES):
        y = 80 + i * 52
        rs = sorted([r for r in rows if r["fire"] == f], key=lambda r: r["seed"])
        vals = [r["mean_consensus_iou"] for r in rs]
        m, sd = st.mean(vals), st.stdev(vals)
        body.append(text(184, y + 4, SHORT[f] + ("*" if rs[0]["holdout"] else ""), size=12, fill=INK, font=SANS, anchor="end", weight=600))
        # ±1 sd band, mean tick, circle null, seeds
        body.append(f'<rect x="{sx(m - sd):.1f}" y="{y - 10}" width="{sx(m + sd) - sx(m - sd):.1f}" height="20" fill="rgba(45,49,66,0.08)"/>')
        body.append(f'<line x1="{sx(m):.1f}" y1="{y - 12}" x2="{sx(m):.1f}" y2="{y + 12}" stroke="{INK}" stroke-width="1.2"/>')
        c = sx(rs[0]["mean_radial_iou"])
        body.append(f'<polygon points="{c:.1f},{y - 7} {c + 7:.1f},{y} {c:.1f},{y + 7} {c - 7:.1f},{y}" fill="{PAPER}" stroke="{INK}" stroke-width="1"/>')
        for r, v in zip(rs, vals):
            focal = f == "Buck_2017" and v == min(vals)
            stroke, fill = (ACCENT, "rgba(235,108,54,0.15)") if focal else (MUTED, "rgba(79,93,117,0.20)")
            body.append(f'<circle cx="{sx(v):.1f}" cy="{y}" r="5" fill="{PAPER}"/>'
                        f'<circle cx="{sx(v):.1f}" cy="{y}" r="5" fill="{fill}" stroke="{stroke}" stroke-width="{1.2 if focal else 1}"/>')
            if focal:
                body.append(text(sx(v), y - 14, f"SEED {r['seed']}: {v:.3f}", anchor="middle", fill=INK))
        body.append(text(x1 + 12, y + 4, f"sd {sd:.3f}", fill=MUTED))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<circle cx="{x + 6}" cy="{y}" r="5" fill="rgba(79,93,117,0.20)" stroke="{MUTED}"/>', "one seed"),
        (lambda x, y: f'<line x1="{x + 6}" y1="{y - 8}" x2="{x + 6}" y2="{y + 8}" stroke="{INK}" stroke-width="1.2"/>', "mean"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.08)"/>', "±1 sd"),
        (lambda x, y: f'<polygon points="{x + 6},{y - 7} {x + 13},{y} {x + 6},{y + 7} {x - 1},{y}" fill="{PAPER}" stroke="{INK}"/>', "Circle null"),
        (lambda x, y: f'<circle cx="{x + 6}" cy="{y}" r="5" fill="rgba(235,108,54,0.15)" stroke="{ACCENT}" stroke-width="1.2"/>', "the one outlier"),
    ]))
    (FIG / "e33-noise-floor.svg").write_text(svg(
        "e33", "E33 noise floor: five seeds per fire",
        "Dot strip of mean forecast IoU for five random seeds of the recommended ensemble on each of six fires, "
        "with the mean, one-standard-deviation band and the Circle null; every spread is under 0.04 and one Buck seed is the only outlier.",
        W, H, "\n".join(body)))
    print("wrote e33-noise-floor.svg")


def panel_grid(n, cols, pw, ph, x_gap, y_gap, top, left):
    """Origins of n small-multiple panels, row-major."""
    return [(left + (i % cols) * (pw + x_gap), top + (i // cols) * (ph + y_gap)) for i in range(n)]


def e33_sd():
    rows = load("exp33_noise.json")
    if rows is None:
        return {}
    out = {}
    for f in FIRES:
        v = [r["mean_consensus_iou"] for r in rows if r["fire"] == f]
        out[f] = (st.mean(v), st.stdev(v), [r for r in rows if r["fire"] == f and r["seed"] == 0][0])
    return out


def e32_members():
    rows = load("exp32_members.json")
    base = e33_sd()
    if rows is None or not base:
        return
    W, H = 1000, 520
    PW, PH = 272, 160
    ms = [8, 16, 32, 64, 128]
    body = []
    for i, f in enumerate(FIRES):
        px, py = panel_grid(6, 3, PW, PH, 48, 56, 48, 56)[i]
        pts = {32: base[f][0]}
        for r in rows:
            if r["fire"] == f:
                pts[r["members"]] = r["mean_consensus_iou"]
        vals = [pts[m] for m in ms if m in pts]
        circle = base[f][2]["mean_radial_iou"]
        lo = min(min(vals), circle) - 0.03
        hi = max(max(vals), circle) + 0.03
        lo, hi = round(lo * 20) / 20, round(hi * 20 + 0.5) / 20
        sx = lambda m: px + 24 + ms.index(m) * (PW - 48) / 4
        sy = lambda v: py + PH - 24 - (v - lo) / (hi - lo) * (PH - 40)
        body.append(text(px, py - 8, SHORT[f] + ("*" if base[f][2]["holdout"] else ""), size=12, fill=INK, font=SANS, weight=600))
        for g in (lo, (lo + hi) / 2, hi):
            body.append(f'<line x1="{px + 16}" y1="{sy(g):.1f}" x2="{px + PW}" y2="{sy(g):.1f}" stroke="{RULE}" stroke-width="0.8"/>')
            body.append(text(px + 12, sy(g) + 3, f"{g:.2f}", anchor="end"))
        # ±1 sd band at M = 32 from E33, then the Circle, then the series.
        m, sd = base[f][0], base[f][1]
        body.append(f'<rect x="{px + 16}" y="{sy(m + sd):.1f}" width="{PW - 16}" height="{sy(m - sd) - sy(m + sd):.1f}" fill="rgba(45,49,66,0.08)"/>')
        body.append(f'<line x1="{px + 16}" y1="{sy(circle):.1f}" x2="{px + PW}" y2="{sy(circle):.1f}" stroke="{MUTED}" stroke-width="1" stroke-dasharray="5,4"/>')
        poly = " ".join(f"{sx(mm):.1f},{sy(pts[mm]):.1f}" for mm in ms if mm in pts)
        body.append(f'<polyline points="{poly}" fill="none" stroke="{INK}" stroke-width="1.8" stroke-linejoin="round"/>')
        for mm in ms:
            if mm in pts:
                body.append(f'<circle cx="{sx(mm):.1f}" cy="{sy(pts[mm]):.1f}" r="4" fill="{INK}"/>')
            body.append(text(sx(mm), py + PH - 8, str(mm), anchor="middle"))
    body.append(text(500, H - 60, "MEMBERS (LOG SPACED)", anchor="middle", extra='letter-spacing="0.10em"'))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{INK}" stroke-width="1.8"/><circle cx="{x + 8}" cy="{y}" r="3" fill="{INK}"/>', "mean consensus IoU"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.08)"/>', "E33 ±1 sd at M=32"),
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{MUTED}" stroke-dasharray="5,4"/>', "Circle null"),
    ]))
    (FIG / "e32-members.svg").write_text(svg(
        "e32", "E32 ensemble size: forecast skill against members",
        "Six small line charts, one per fire, of mean forecast IoU for ensembles of 8 to 128 members, "
        "with the Circle null and the E33 noise band at 32 members.", W, H, "\n".join(body)))
    print("wrote e32-members.svg")


def e35_prior():
    rows = load("exp35_prior.json")
    base = e33_sd()
    if rows is None or not base:
        return
    W, H = 1000, 464
    x0, x1, lo, hi = 200, 920, 0.30, 0.70
    sx = lambda v: x0 + (v - lo) / (hi - lo) * (x1 - x0)
    body = []
    for v in (0.3, 0.4, 0.5, 0.6, 0.7):
        body.append(f'<line x1="{sx(v):.0f}" y1="56" x2="{sx(v):.0f}" y2="376" stroke="{RULE}" stroke-width="0.8"/>')
        body.append(text(sx(v), 392, f"{v:.1f}", anchor="middle"))
    body.append(text(560, 408, "MEAN ONE-WINDOW-AHEAD CONSENSUS IOU", anchor="middle", extra='letter-spacing="0.10em"'))
    for i, f in enumerate(FIRES):
        y = 80 + i * 52
        m, sd, row0 = base[f]
        body.append(text(184, y + 4, SHORT[f] + ("*" if row0["holdout"] else ""), size=12, fill=INK, font=SANS, anchor="end", weight=600))
        body.append(f'<rect x="{sx(m - sd):.1f}" y="{y - 10}" width="{sx(m + sd) - sx(m - sd):.1f}" height="20" fill="rgba(45,49,66,0.08)"/>')
        body.append(f'<line x1="{sx(m):.1f}" y1="{y - 12}" x2="{sx(m):.1f}" y2="{y + 12}" stroke="{INK}" stroke-width="1.2"/>')
        c = sx(row0["mean_radial_iou"])
        body.append(f'<polygon points="{c:.1f},{y - 7} {c + 7:.1f},{y} {c:.1f},{y + 7} {c - 7:.1f},{y}" fill="{PAPER}" stroke="{INK}" stroke-width="1"/>')
        for r in rows:
            if r["fire"] != f:
                continue
            v = sx(r["mean_consensus_iou"])
            if r["config"] == "prior_narrow":
                body.append(f'<circle cx="{v:.1f}" cy="{y}" r="5" fill="{PAPER}" stroke="{MUTED}" stroke-width="1.2"/>')
            else:
                body.append(f'<rect x="{v - 5:.1f}" y="{y - 5}" width="10" height="10" fill="rgba(79,93,117,0.20)" stroke="{MUTED}" stroke-width="1"/>')
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<line x1="{x + 6}" y1="{y - 8}" x2="{x + 6}" y2="{y + 8}" stroke="{INK}" stroke-width="1.2"/><rect x="{x - 2}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.08)"/>', "broad prior: E33 mean ±1 sd"),
        (lambda x, y: f'<circle cx="{x + 6}" cy="{y}" r="5" fill="{PAPER}" stroke="{MUTED}" stroke-width="1.2"/>', "narrow prior"),
        (lambda x, y: f'<rect x="{x + 1}" y="{y - 5}" width="10" height="10" fill="rgba(79,93,117,0.20)" stroke="{MUTED}"/>', "very broad prior"),
        (lambda x, y: f'<polygon points="{x + 6},{y - 7} {x + 13},{y} {x + 6},{y + 7} {x - 1},{y}" fill="{PAPER}" stroke="{INK}"/>', "Circle null"),
    ]))
    (FIG / "e35-prior.svg").write_text(svg(
        "e35", "E35 prior width: narrow, broad and very broad",
        "Dot strip per fire of mean forecast IoU under a narrow, the standard broad, and a very broad prior, "
        "against the E33 noise band and the Circle null.", W, H, "\n".join(body)))
    print("wrote e35-prior.svg")


def e36_offline():
    rows = load("exp36_offline.json")
    base = e33_sd()
    if rows is None or not base:
        return
    W, H = 1000, 520
    PW, PH = 272, 160
    body = []
    for i, f in enumerate(FIRES):
        px, py = panel_grid(6, 3, PW, PH, 48, 56, 48, 56)[i]
        off = [r for r in rows if r["fire"] == f][0]
        filt = base[f][2]
        a, b = off["per_day_consensus"], filt["per_day_consensus"]
        n = min(len(a), len(b))
        fit_days = off["fit"]["fit_days"]
        lo, hi = 0.0, 0.8
        sx = lambda d: px + 24 + d * (PW - 48) / max(n - 1, 1)
        sy = lambda v: py + PH - 24 - (v - lo) / (hi - lo) * (PH - 40)
        body.append(text(px, py - 8, SHORT[f] + ("*" if filt["holdout"] else ""), size=12, fill=INK, font=SANS, weight=600))
        for g in (0.0, 0.4, 0.8):
            body.append(f'<line x1="{px + 16}" y1="{sy(g):.1f}" x2="{px + PW}" y2="{sy(g):.1f}" stroke="{RULE}" stroke-width="0.8"/>')
            body.append(text(px + 12, sy(g) + 3, f"{g:.1f}", anchor="end"))
        # fit window shading, then the two curves
        body.append(f'<rect x="{sx(0):.1f}" y="{py + 8}" width="{sx(fit_days - 1) - sx(0):.1f}" height="{PH - 32}" fill="rgba(45,49,66,0.06)"/>')
        body.append(f'<polyline points="{" ".join(f"{sx(d):.1f},{sy(b[d]):.1f}" for d in range(n))}" fill="none" stroke="{INK}" stroke-width="1.2" stroke-linejoin="round"/>')
        body.append(f'<polyline points="{" ".join(f"{sx(d):.1f},{sy(a[d]):.1f}" for d in range(n))}" fill="none" stroke="{ACCENT}" stroke-width="1.8" stroke-linejoin="round"/>')
        for d in (0, n - 1):
            body.append(text(sx(d), py + PH - 8, f"day {d + 1}", anchor="middle"))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{ACCENT}" stroke-width="1.8"/>', "GA fitted on the shaded days, then run forward"),
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{INK}" stroke-width="1.2"/>', "filter learning every day (E33 seed 0)"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.06)"/>', "fit window"),
    ]))
    (FIG / "e36-offline.svg").write_text(svg(
        "e36", "E36 offline fit versus learning as it burns",
        "Six small line charts of per-day forecast IoU: a genetic algorithm fitted to the first days then run forward, "
        "against the particle filter that learns from each day's perimeter.", W, H, "\n".join(body)))
    print("wrote e36-offline.svg")


def e37_illuminate():
    rows = load("exp37_illuminate.json")
    if rows is None:
        return
    W, H = 1000, 560
    PW, PH = 272, 200
    body = []
    for i, f in enumerate(FIRES):
        px, py = panel_grid(6, 3, PW, PH, 48, 40, 48, 56)[i]
        r = [x for x in rows if x["fire"] == f][0]
        rep = json.loads((EXP / "exp37_illuminate" / f"{f}.json").read_text())
        a = rep["archive"]
        (gx, gy), (rx, ry) = a["dims"], a["ranges"]
        cw, ch = (PW - 40) / gx, (PH - 40) / gy
        body.append(text(px, py - 8, f"{SHORT[f]} · {a['stats']['elites']} elites, coverage {a['stats']['coverage']:.2f}", size=12, fill=INK, font=SANS, weight=600))
        body.append(f'<rect x="{px + 32}" y="{py}" width="{PW - 40}" height="{PH - 40}" fill="none" stroke="{RULE}"/>')
        for e in a["elites"]:
            cx, cy = e["coords"]
            body.append(f'<rect x="{px + 32 + cx * cw:.1f}" y="{py + (gy - 1 - cy) * ch:.1f}" width="{cw:.1f}" height="{ch:.1f}" fill="rgba(45,49,66,0.28)"/>')
        for (hh, g, el) in rep["observed"]:
            ox = px + 32 + (g - rx[0]) / (rx[1] - rx[0]) * (PW - 40)
            oy = py + (PH - 40) - (el - ry[0]) / (ry[1] - ry[0]) * (PH - 40)
            ox, oy = min(max(ox, px + 32), px + PW - 8), min(max(oy, py), py + PH - 40)
            body.append(f'<circle cx="{ox:.1f}" cy="{oy:.1f}" r="4" fill="{PAPER}"/><circle cx="{ox:.1f}" cy="{oy:.1f}" r="4" fill="rgba(235,108,54,0.15)" stroke="{ACCENT}" stroke-width="1.2"/>')
        body.append(text(px + 32, py + PH - 24, f"{rx[0]:.2f}", anchor="start"))
        body.append(text(px + PW - 8, py + PH - 24, f"growth {rx[1]:.2f}", anchor="end"))
        body.append(text(px + 28, py + PH - 40, f"{ry[0]:.0f}", anchor="end"))
        body.append(text(px + 28, py + 8, f"{ry[1]:.0f}", anchor="end"))
        body.append(text(px + 28, py + PH / 2 - 20, "elong.", anchor="end"))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.28)"/>', "a knob setting the model can produce (archive cell)"),
        (lambda x, y: f'<circle cx="{x + 8}" cy="{y}" r="4" fill="rgba(235,108,54,0.15)" stroke="{ACCENT}" stroke-width="1.2"/>', "the observed fire, one dot per day"),
    ]))
    (FIG / "e37-illuminate.svg").write_text(svg(
        "e37", "E37 what shapes can the fire model make",
        "Six MAP-Elites archives, one per fire, over growth and elongation of the burned area after five days, "
        "with the observed fire's own growth and elongation marked day by day.", W, H, "\n".join(body)))
    print("wrote e37-illuminate.svg")


def e34_operators():
    rows = load("exp34_operators.json")
    base = e33_sd()
    if rows is None or not base:
        return
    configs = [("imm0", "immigrants 0"), ("imm0.1", "immigrants 0.1"), ("imm0.4", "immigrants 0.4"),
               ("sigma0.1", "sigma 0.1"), ("sigma0.4", "sigma 0.4"), ("beta5", "beta 5"), ("beta20", "beta 20"),
               ("cross0.5", "crossover 0.5"), ("cross1.0", "crossover 1.0")]
    W, H = 1000, 488
    left, top, cw, rh = 240, 72, 112, 40
    body = []
    for j, f in enumerate(FIRES):
        body.append(text(left + j * cw + cw / 2, top - 16, SHORT[f] + ("*" if base[f][2]["holdout"] else ""), size=12, fill=INK, font=SANS, anchor="middle", weight=600))
        body.append(text(left + j * cw + cw / 2, top - 4, f"sd {base[f][1]:.3f}", anchor="middle"))
    for i, (key, label) in enumerate(configs):
        y = top + i * rh
        if i in (3, 5, 7):
            body.append(f'<line x1="{left - 200}" y1="{y - 4}" x2="{left + 6 * cw}" y2="{y - 4}" stroke="{RULE}" stroke-width="0.8"/>')
        body.append(text(left - 16, y + rh / 2 + 4, label, size=12, fill=INK, font=SANS, anchor="end", weight=500))
        for j, f in enumerate(FIRES):
            r = [x for x in rows if x["fire"] == f and x["config"] == key]
            if not r:
                continue
            d = r[0]["mean_consensus_iou"] - base[f][2]["mean_consensus_iou"]
            beyond = abs(d) >= base[f][1]
            # Tone: ties are faint; real deltas carry ink, a real gain the accent.
            if not beyond:
                fill, stroke, col = "rgba(45,49,66,0.04)", RULE, SOFT
            elif d > 0:
                fill, stroke, col = "rgba(235,108,54,0.15)", ACCENT, INK
            else:
                fill, stroke, col = "rgba(45,49,66,0.16)", MUTED, INK
            body.append(f'<rect x="{left + j * cw + 4}" y="{y}" width="{cw - 8}" height="{rh - 8}" rx="4" fill="{fill}" stroke="{stroke}" stroke-width="0.8"/>')
            body.append(text(left + j * cw + cw / 2, y + rh / 2 + 2, f"{d:+.3f}", fill=col, anchor="middle", weight=500 if beyond else 400))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" rx="2" fill="rgba(45,49,66,0.04)" stroke="{RULE}"/>', "tie (inside the fire's E33 sd)"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" rx="2" fill="rgba(45,49,66,0.16)" stroke="{MUTED}"/>', "loss beyond sd"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" rx="2" fill="rgba(235,108,54,0.15)" stroke="{ACCENT}"/>', "gain beyond sd"),
    ]))
    (FIG / "e34-operators.svg").write_text(svg(
        "e34", "E34 operator ablation: change in forecast IoU",
        "Matrix of nine operator settings against six fires showing the change in mean forecast IoU from the recommended "
        "configuration, with ties inside each fire's noise left faint.", W, H, "\n".join(body)))
    print("wrote e34-operators.svg")


def e38_immreset():
    rows = load("exp38_immreset.json")
    noise = load("exp33_noise.json")
    base = e33_sd()
    if rows is None or noise is None or not base:
        return
    W, H = 1000, 464
    x0, x1, lo, hi = 200, 920, -0.06, 0.12
    sx = lambda v: x0 + (v - lo) / (hi - lo) * (x1 - x0)
    body = []
    for v in (-0.05, 0.0, 0.05, 0.10):
        w = 1.2 if v == 0.0 else 0.8
        body.append(f'<line x1="{sx(v):.0f}" y1="56" x2="{sx(v):.0f}" y2="376" stroke="{INK if v == 0.0 else RULE}" stroke-width="{w}" stroke-opacity="{0.4 if v == 0.0 else 1}"/>')
        body.append(text(sx(v), 392, f"{v:+.2f}", anchor="middle"))
    body.append(text(560, 408, "RESET MINUS E33 TWIN, MEAN CONSENSUS IOU, PER SEED", anchor="middle", extra='letter-spacing="0.10em"'))
    for i, f in enumerate(FIRES):
        y = 80 + i * 52
        m, sd, row0 = base[f]
        body.append(text(184, y + 4, SHORT[f] + ("*" if row0["holdout"] else ""), size=12, fill=INK, font=SANS, anchor="end", weight=600))
        body.append(f'<rect x="{sx(-sd):.1f}" y="{y - 10}" width="{sx(sd) - sx(-sd):.1f}" height="20" fill="rgba(45,49,66,0.08)"/>')
        for s_ in range(5):
            a = [x for x in rows if x["fire"] == f and x["seed"] == s_]
            b = [x for x in noise if x["fire"] == f and x["seed"] == s_]
            if not a or not b:
                continue
            d = a[0]["mean_consensus_iou"] - b[0]["mean_consensus_iou"]
            focal = f == "Buck_2017" and s_ == 3
            stroke, fill = (ACCENT, "rgba(235,108,54,0.15)") if focal else (MUTED, "rgba(79,93,117,0.20)")
            body.append(f'<circle cx="{sx(d):.1f}" cy="{y}" r="5" fill="{PAPER}"/><circle cx="{sx(d):.1f}" cy="{y}" r="5" fill="{fill}" stroke="{stroke}" stroke-width="{1.2 if focal else 1}"/>')
            if focal:
                body.append(text(sx(d), y - 14, f"BUCK SEED 3: {d:+.3f}", anchor="middle", fill=INK))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<circle cx="{x + 6}" cy="{y}" r="5" fill="rgba(79,93,117,0.20)" stroke="{MUTED}"/>', "one seed, reset minus its E33 twin"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.08)"/>', "±1 sd (E33)"),
        (lambda x, y: f'<circle cx="{x + 6}" cy="{y}" r="5" fill="rgba(235,108,54,0.15)" stroke="{ACCENT}" stroke-width="1.2"/>', "the seed that locked in (E33)"),
    ]))
    (FIG / "e38-immreset.svg").write_text(svg(
        "e38", "E38 immigrant reset: change per seed",
        "Dot strip per fire of the change in mean forecast IoU when immigrants start with a fresh driver state, "
        "one dot per matched seed, against the E33 noise band.", W, H, "\n".join(body)))
    print("wrote e38-immreset.svg")


if __name__ == "__main__":
    FIG.mkdir(parents=True, exist_ok=True)
    e33_noise()
    e32_members()
    e35_prior()
    e34_operators()
    e36_offline()
    e37_illuminate()
    e38_immreset()
