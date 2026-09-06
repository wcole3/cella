#!/usr/bin/env python
"""Three overview figures for experiments/README.md, as standalone SVG.

Numbers are hard-coded from the experiment log (they are the recorded
rows, not re-computed), so this script needs no result files. Style: the
diagram-design skill's neutral skin (paper #f5f5f5, ink #2d3142, muted
#4f5d75, one accent #eb6c36 per figure, Geist / Geist Mono, 4px grid,
legend as a bottom strip, <title>/<desc> for screen readers).

  overview-scores.svg          small multiples: mean IoU per fire by round vs the Circle
  overview-configurations.svg  timeline of rounds with the working configuration
  overview-score-families.svg  how the two score families are computed
"""
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIG = HERE.parents[1] / "experiments" / "figures"
PAPER, INK, MUTED, SOFT, ACCENT = "#f5f5f5", "#2d3142", "#4f5d75", "#7a8399", "#eb6c36"
RULE = "rgba(45,49,66,0.10)"
FONTS = ("@import url('https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600"
         "&amp;family=Geist+Mono:wght@400;500;600&amp;display=swap');")
SANS, MONO = "'Geist', sans-serif", "'Geist Mono', monospace"


def svg(slug, title, desc, w, h, body, defs=""):
    return (f'<?xml version="1.0" encoding="UTF-8"?>\n'
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" role="img" '
            f'aria-labelledby="{slug}-title {slug}-desc">\n'
            f'<title id="{slug}-title">{title}</title>\n<desc id="{slug}-desc">{desc}</desc>\n'
            f'<defs><style>{FONTS}</style>{defs}</defs>\n'
            f'<rect width="100%" height="100%" fill="{PAPER}"/>\n{body}\n</svg>\n')


def text(x, y, s, size=8, fill=MUTED, font=MONO, anchor="start", weight=400, extra=""):
    return (f'<text x="{x}" y="{y}" fill="{fill}" font-size="{size}" font-family="{font}" '
            f'font-weight="{weight}" text-anchor="{anchor}" {extra}>{s}</text>')


def legend(y, w, items):
    out = [f'<line x1="32" y1="{y - 8}" x2="{w - 32}" y2="{y - 8}" stroke="{RULE}" stroke-width="0.8"/>',
           text(32, y + 8, "LEGEND", extra='letter-spacing="0.14em"')]
    x = 112
    for mark, label in items:
        out.append(mark(x, y + 4))
        out.append(text(x + 20, y + 8, label))
        x += 48 + 6 * len(label)
    return "\n".join(out)


# ---------------------------------------------------------------- figure 1
FIRES = ["Bear", "Brattain", "Buck", "Chimney", "Ferguson*", "Pier*"]
CIRCLE = [0.541, 0.450, 0.670, 0.372, 0.373, 0.559]
# Single-run mean IoU (Rounds 1-3) and ensemble forecast consensus IoU (Rounds 4-5).
ROUNDS = ["R1", "R3", "R4", "R5"]
SINGLE = {  # E16c global control (Round 1 global recipe), E20 transfer recipe
    "R1": [0.220, 0.308, 0.369, 0.421, 0.145, 0.321],
    "R3": [0.452, 0.369, 0.466, 0.391, 0.158, 0.512],
}
ENSEMBLE = {  # E25 recommended row, E33 five-seed mean
    "R4": [0.473, 0.400, 0.616, 0.426, 0.345, 0.533],
    "R5": [0.479, 0.416, 0.590, 0.434, 0.344, 0.535],
}


def overview_scores():
    W, H = 1000, 560
    PW, PH = 272, 176
    lo, hi = 0.0, 0.7
    body = []
    for i, fire in enumerate(FIRES):
        px = 56 + (i % 3) * (PW + 48)
        py = 48 + (i // 3) * (PH + 56)
        sx = lambda k: px + 40 + k * (PW - 64) / 3
        sy = lambda v: py + PH - 24 - (v - lo) / (hi - lo) * (PH - 40)
        body.append(text(px, py - 8, fire, size=12, fill=INK, font=SANS, weight=600))
        for g in (0.0, 0.2, 0.4, 0.6):
            body.append(f'<line x1="{px + 24}" y1="{sy(g):.1f}" x2="{px + PW}" y2="{sy(g):.1f}" stroke="{RULE}" stroke-width="0.8"/>')
            body.append(text(px + 20, sy(g) + 3, f"{g:.1f}", anchor="end"))
        # Circle null
        body.append(f'<line x1="{px + 24}" y1="{sy(CIRCLE[i]):.1f}" x2="{px + PW}" y2="{sy(CIRCLE[i]):.1f}" stroke="{MUTED}" stroke-width="1" stroke-dasharray="5,4"/>')
        # score-family divider between R3 and R4
        xd = (sx(1) + sx(2)) / 2
        body.append(f'<line x1="{xd:.1f}" y1="{py + 8}" x2="{xd:.1f}" y2="{py + PH - 20}" stroke="{ACCENT}" stroke-width="1" stroke-dasharray="3,3"/>')
        # single-run segment
        s = [SINGLE["R1"][i], SINGLE["R3"][i]]
        body.append(f'<polyline points="{sx(0):.1f},{sy(s[0]):.1f} {sx(1):.1f},{sy(s[1]):.1f}" fill="none" stroke="{INK}" stroke-width="1.8"/>')
        for k, v in enumerate(s):
            body.append(f'<circle cx="{sx(k):.1f}" cy="{sy(v):.1f}" r="4" fill="{PAPER}" stroke="{INK}" stroke-width="1.5"/>')
        # ensemble segment
        e = [ENSEMBLE["R4"][i], ENSEMBLE["R5"][i]]
        body.append(f'<polyline points="{sx(2):.1f},{sy(e[0]):.1f} {sx(3):.1f},{sy(e[1]):.1f}" fill="none" stroke="{INK}" stroke-width="1.8"/>')
        for k, v in enumerate(e):
            body.append(f'<circle cx="{sx(k + 2):.1f}" cy="{sy(v):.1f}" r="4" fill="{INK}"/>')
        # endpoint values, placed on the side away from the Circle line (the
        # point itself is never moved)
        c = CIRCLE[i]
        y_r1 = sy(s[0]) - 10 if 0 <= s[0] - c <= 0.10 else sy(s[0]) + 16
        y_r5 = sy(e[1]) + 16 if 0 <= c - e[1] <= 0.10 else sy(e[1]) - 10
        body.append(text(sx(0), y_r1, f"{s[0]:.2f}", anchor="middle"))
        body.append(text(sx(3), y_r5, f"{e[1]:.2f}", anchor="middle", fill=INK))
        for k, r in enumerate(ROUNDS):
            body.append(text(sx(k), py + PH - 8, r, anchor="middle"))
    body.append(text(500, H - 64, "ROUND (ROUND 2 CHANGED NO CONFIGURATION; * = HOLDOUT FIRE)", anchor="middle", extra='letter-spacing="0.10em"'))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<circle cx="{x + 8}" cy="{y}" r="4" fill="{PAPER}" stroke="{INK}" stroke-width="1.5"/>', "single run, mean IoU (R1, R3)"),
        (lambda x, y: f'<circle cx="{x + 8}" cy="{y}" r="4" fill="{INK}"/>', "ensemble forecast, consensus IoU (R4, R5)"),
        (lambda x, y: f'<line x1="{x}" y1="{y}" x2="{x + 16}" y2="{y}" stroke="{MUTED}" stroke-dasharray="5,4"/>', "Circle null"),
        (lambda x, y: f'<line x1="{x + 8}" y1="{y - 7}" x2="{x + 8}" y2="{y + 7}" stroke="{ACCENT}" stroke-dasharray="3,3"/>', "score family changes"),
    ]))
    (FIG / "overview-scores.svg").write_text(svg(
        "ovs", "Score per fire across the rounds",
        "Six small line charts, one per fire, of the working configuration's score after Rounds 1, 3, 4 and 5 "
        "against the Circle null. Rounds 1 and 3 are single-run mean IoU; Rounds 4 and 5 are ensemble forecast "
        "consensus IoU; a dashed accent line marks where the score family changes.", W, H, "\n".join(body)))
    print("wrote overview-scores.svg")


# ---------------------------------------------------------------- figure 2
def overview_configurations():
    W, H = 1000, 436
    Y = 200                       # baseline
    PX_PER_DAY = 140
    # honest scale on both sides of a visible axis break (Aug 15 -> Sep 1 is 17 days)
    x_aug14, x_aug15 = 64, 64 + PX_PER_DAY
    x_break0, x_break1 = 244, 284
    x_sep = lambda d: 324 + (d - 1) * PX_PER_DAY   # Sep 1..5 -> 324 .. 884
    body = []
    body.append(f'<line x1="{x_aug14 - 24}" y1="{Y}" x2="{x_break0}" y2="{Y}" stroke="{INK}" stroke-width="1"/>')
    body.append(f'<line x1="{x_break1}" y1="{Y}" x2="{x_sep(5) + 40}" y2="{Y}" stroke="{INK}" stroke-width="1"/>')
    # axis break: two short slashes
    for dx in (0, 12):
        body.append(f'<line x1="{x_break0 + 8 + dx}" y1="{Y + 8}" x2="{x_break0 + 20 + dx}" y2="{Y - 8}" stroke="{INK}" stroke-width="1"/>')
    body.append(text((x_break0 + x_break1) / 2 + 6, Y + 28, "17 DAYS", anchor="middle", extra='letter-spacing="0.10em"'))
    # ticks and date labels
    ticks = [(x_aug14, "AUG 14"), (x_aug15, "AUG 15"), (x_sep(1), "SEP 1"), (x_sep(2), "SEP 2"),
             (x_sep(3), "SEP 3"), (x_sep(4), "SEP 4"), (x_sep(5), "SEP 5")]
    for x, lab in ticks:
        body.append(f'<line x1="{x}" y1="{Y - 4}" x2="{x}" y2="{Y + 4}" stroke="{INK}" stroke-width="1"/>')
        body.append(text(x, Y + 20, lab, anchor="middle", extra='letter-spacing="0.10em"'))
    body.append(text(500, Y + 44, "2026", anchor="middle", extra='letter-spacing="0.14em"'))

    events = [
        # x, above?, major?, title, lines (mono)
        (x_aug14, False, False, "Baseline", ["textbook p0 0.58, dur 5", "loses to the Circle on 5 of 6"]),
        (x_aug15, True, False, "Round 1 · E1–E8", ["per fire: E1 (p0, dur) + temperature", "global: p0 0.22, dur 5 + temperature"]),
        (x_sep(1), False, False, "Round 2 · E9–E13", ["wind audit: no bug, input inert", "wind_from_deg; no recipe change"]),
        (x_sep(2), True, False, "Round 3 · E14–E23", ["per fire: p0 ×4, station wind,", "moisture, decay τ 5 d (E17)", "global: E20 transfer recipe"]),
        (x_sep(4), False, True, "Round 4 · E24–E28", ["32-member filter, β 10, σ 0.2,", "immigrants 0.2; containment", "operator on, decay off"]),
        (x_sep(5), True, False, "E31 + Round 5 · E32–E38", ["generic explore engine (E31)", "same configuration;", "immigrant_reset option"]),
    ]
    for x, above, major, title, lines in events:
        r = 6 if major else 4
        fill = ACCENT if major else INK
        body.append(f'<circle cx="{x}" cy="{Y}" r="{r}" fill="{fill}"/>')
        anchor = "middle"
        tx = x
        if x == x_aug14:
            anchor, tx = "start", x - 24
        if x == x_sep(5):
            anchor, tx = "end", x + 40
        if above:
            body.append(f'<line x1="{x}" y1="{Y - r - 2}" x2="{x}" y2="{Y - 44}" stroke="{SOFT}" stroke-width="1"/>')
            y0 = Y - 56 - 14 * len(lines)
            body.append(text(tx, y0, title, size=11, fill=INK, font=SANS, anchor=anchor, weight=600))
            for j, ln in enumerate(lines):
                body.append(text(tx, y0 + 16 + 14 * j, ln, anchor=anchor))
        else:
            body.append(f'<line x1="{x}" y1="{Y + r + 2}" x2="{x}" y2="{Y + 56}" stroke="{SOFT}" stroke-width="1"/>')
            y0 = Y + 76
            body.append(text(tx, y0, title, size=11, fill=INK, font=SANS, anchor=anchor, weight=600))
            for j, ln in enumerate(lines):
                body.append(text(tx, y0 + 16 + 14 * j, ln, anchor=anchor))
    # score-family brackets under the below-labels
    yb = Y + 152
    for x0, x1, lab in ((x_aug14, x_sep(2), "SINGLE RUN · MEAN IoU"), (x_sep(4), x_sep(5), "ENSEMBLE FORECAST · CONSENSUS IoU + BRIER")):
        body.append(f'<line x1="{x0}" y1="{yb}" x2="{x1}" y2="{yb}" stroke="{MUTED}" stroke-width="1"/>')
        for x in (x0, x1):
            body.append(f'<line x1="{x}" y1="{yb - 6}" x2="{x}" y2="{yb}" stroke="{MUTED}" stroke-width="1"/>')
        body.append(text((x0 + x1) / 2, yb + 16, lab, anchor="middle", extra='letter-spacing="0.10em"'))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<circle cx="{x + 8}" cy="{y}" r="4" fill="{INK}"/>', "round"),
        (lambda x, y: f'<circle cx="{x + 8}" cy="{y}" r="6" fill="{ACCENT}"/>', "the ensemble becomes the product"),
        (lambda x, y: f'<line x1="{x + 4}" y1="{y + 6}" x2="{x + 12}" y2="{y - 6}" stroke="{INK}"/><line x1="{x + 10}" y1="{y + 6}" x2="{x + 18}" y2="{y - 6}" stroke="{INK}"/>', "axis break (same scale both sides)"),
    ]))
    (FIG / "overview-configurations.svg").write_text(svg(
        "ovc", "Rounds and working configurations, 14 August to 5 September 2026",
        "Timeline of the baseline and five experiment rounds with the working configuration after each, "
        "an axis break over the 17-day gap, and brackets showing which rounds use single-run mean IoU and "
        "which use ensemble forecast consensus IoU.", W, H, "\n".join(body)))
    print("wrote overview-configurations.svg")


# ---------------------------------------------------------------- figure 3
def node(cx, cy, w, h, title, sub, kind):
    fill, stroke, dash = {
        "input": ("rgba(79,93,117,0.10)", SOFT, ""),
        "step": ("#ffffff", INK, ""),
        "store": ("rgba(45,49,66,0.05)", MUTED, ""),
        "focal": ("rgba(235,108,54,0.08)", ACCENT, ""),
    }[kind]
    x, y = cx - w / 2, cy - h / 2
    out = [f'<rect x="{x:.0f}" y="{y:.0f}" width="{w}" height="{h}" rx="6" fill="{fill}" stroke="{stroke}" stroke-width="1.2" {dash}/>']
    ty = cy - 4 if sub else cy + 4
    out.append(text(cx, ty, title, size=11, fill=INK, font=SANS, anchor="middle", weight=600))
    if isinstance(sub, str):
        sub = [sub]
    for j, s in enumerate(sub or []):
        out.append(text(cx, cy + 12 + 12 * j, s, anchor="middle"))
    return "\n".join(out)


def overview_score_families():
    W, H = 1000, 604
    NW, NH = 128, 72
    slot = lambda i: 96 + i * 162
    yA, yB, yL = 152, 316, 460
    defs = (f'<marker id="ovf-arrow" markerWidth="8" markerHeight="6" refX="7" refY="3" orient="auto">'
            f'<polygon points="0 0, 8 3, 0 6" fill="{MUTED}"/></marker>')
    body = []
    # zones (drawn first)
    body.append(f'<rect x="24" y="80" width="952" height="136" rx="8" fill="none" stroke="{RULE}" stroke-width="1"/>')
    body.append(text(40, 100, "ROUNDS 1–3 · ONE RUN · MEAN IoU", extra='letter-spacing="0.12em"', fill=INK))
    body.append(f'<rect x="24" y="240" width="952" height="280" rx="8" fill="none" stroke="{RULE}" stroke-width="1"/>')
    body.append(text(40, 260, "ROUNDS 4–5 · 32-MEMBER ENSEMBLE, LEARNING EACH DAY · FORECAST CONSENSUS IoU", extra='letter-spacing="0.12em"', fill=INK))
    arrow = lambda x1, x2, y: f'<line x1="{x1}" y1="{y}" x2="{x2}" y2="{y}" stroke="{MUTED}" stroke-width="1.2" marker-end="url(#ovf-arrow)"/>'
    # lane A connectors then nodes
    for i in range(4):
        body.append(arrow(slot(i) + NW / 2, slot(i + 1) - NW / 2, yA))
    A = [("One knob set", ["p0, duration, wind ×"], "input"),
         ("One run", ["day 0 to the end"], "step"),
         ("Burned mask", ["one per observed day"], "store"),
         ("IoU vs truth", ["each day, day 0 excluded"], "step"),
         ("Mean IoU", ["average over the days"], "store")]
    for i, (t, s, k) in enumerate(A):
        body.append(node(slot(i), yA, NW, NH, t, s, k))
    # lane B connectors
    for i in range(5):
        body.append(arrow(slot(i) + NW / 2, slot(i + 1) - NW / 2, yB))
    # loop: score (slot 4) down to learn node, learn node back up into runs (slot 1)
    lx, lw = (slot(2) + slot(3)) / 2, 232
    x5, x1 = slot(4), slot(1)
    body.append(f'<path d="M{x5},{yB + NH / 2} V{yL - 8} Q{x5},{yL} {x5 - 8},{yL} H{lx + lw / 2}" '
                f'fill="none" stroke="{MUTED}" stroke-width="1.2" marker-end="url(#ovf-arrow)"/>')
    body.append(f'<path d="M{lx - lw / 2},{yL} H{x1 + 8} Q{x1},{yL} {x1},{yL - 8} V{yB + NH / 2}" '
                f'fill="none" stroke="{MUTED}" stroke-width="1.2" marker-end="url(#ovf-arrow)"/>')
    body.append(text(x1 - 10, yL - 44, "day k → k+1", anchor="end"))
    body.append(text(x5 + 10, yL - 44, "after scoring", anchor="start"))
    B = [("32 knob sets", ["drawn from the prior"], "input"),
         ("32 runs", ["advance to day k"], "step"),
         ("Probability map", ["share of members", "that burned each cell"], "store"),
         ("Consensus mask", ["cells at ≥ 50 %"], "step"),
         ("IoU vs truth, day k", ["before day k's mask", "is used"], "focal"),
         ("Consensus IoU", ["mean over days;", "Brier from the map"], "store")]
    for i, (t, s, k) in enumerate(B):
        body.append(node(slot(i), yB, NW, NH, t, s, k))
    body.append(node(lx, yL, lw, NH, "Learn from day k's mask", ["resample · mutate · immigrants", "each child keeps its parent's grid"], "step"))
    # caption
    body.append(text(500, 548, "The two families are not comparable: a single run saw nothing; the ensemble's forecast for day k has learned from days 1 … k−1.", anchor="middle", fill=INK, size=9, font=SANS))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" rx="2" fill="rgba(79,93,117,0.10)" stroke="{SOFT}"/>', "input"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" rx="2" fill="#ffffff" stroke="{INK}"/>', "step"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" rx="2" fill="rgba(45,49,66,0.05)" stroke="{MUTED}"/>', "data / score"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" rx="2" fill="rgba(235,108,54,0.08)" stroke="{ACCENT}"/>', "the honesty rule: score first, learn second"),
    ]))
    (FIG / "overview-score-families.svg").write_text(svg(
        "ovf", "How the two score families are computed",
        "Two lanes of boxes and arrows. Top: one knob set, one run, a burned mask per day, IoU against the truth, "
        "averaged into mean IoU. Bottom: 32 knob sets from a prior, 32 runs to day k, a probability map, a 50 percent "
        "consensus mask, IoU against day k's truth before that mask is used, then a learning step that feeds the next day.",
        W, H, "\n".join(body), defs))
    print("wrote overview-score-families.svg")


if __name__ == "__main__":
    FIG.mkdir(parents=True, exist_ok=True)
    overview_scores()
    overview_configurations()
    overview_score_families()
