#!/usr/bin/env python
"""Figures for the Round 7 experiment files, as standalone SVG (no
dependencies). Reads results/experiments/exp4*.json, writes
experiments/figures/e4*.svg. Re-run after any experiment re-runs.

Style: the same as figures_r6.py (which followed figures_r5.py) --
paper #f5f5f5, ink #2d3142, muted #4f5d75, one accent #eb6c36 per figure,
Geist / Geist Mono, 4px grid, legend as a bottom strip, <title>/<desc>
for screen readers. Rendered locally with `resvg` (falls back to a local
serif/mono font, typically DejaVu, when Geist isn't installed -- the SVG
itself is what's committed, not a rendered PNG).

Three figures, one per Round 7 promotion-test file:
  - e44-arm-b-vs-e33: five-seed Arm B vs the E33 baseline, per fire, with
    the E33 +-1 sd band (the promotion test itself).
  - e45-ablations: Arm B (5-seed) vs its two mechanism ablations
    (sigma0 -- diversity kept, learning removed; +-20 degrees -- learning
    kept, range narrowed), per fire, with Arm B's own +-1 sd band.
  - e46-three-arms: E33, Arm B, and the three station-wind arms (a/b/c),
    per fire, each compared against its own governing baseline's sd band.
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
# The E33 five-seed noise floor (sd of mean forecast IoU) -- the tie bar
# for every non-arrival-kernel arm since Round 5.
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


def fire_mean_sd(rows, fire):
    """Mean and sample sd of mean_consensus_iou across every seed row for
    one fire -- the same convention r7_common.fire_stats uses (sample sd,
    n-1; None if fewer than 2 rows).
    """
    vals = [r["mean_consensus_iou"] for r in rows if r["fire"] == fire]
    if not vals:
        return None, None, 0
    mean = sum(vals) / len(vals)
    if len(vals) < 2:
        return mean, None, len(vals)
    var = sum((v - mean) ** 2 for v in vals) / (len(vals) - 1)
    return mean, var ** 0.5, len(vals)


def bar_panel(body, px, py, pw, ph, fire, bars, band):
    """One small-multiple panel: a vertical bar per (label, mean, sd, color,
    beyond) tuple in `bars`, an optional shaded +-1 sd `band` (lo, hi) drawn
    behind everything, gridlines, and a fire-name header. Shared by all
    three figures below.
    """
    vals = [b[1] for b in bars if b[1] is not None]
    if not vals:
        return
    lo_val = min(vals + ([band[0]] if band else []))
    hi_val = max(vals + ([band[1]] if band else [])) + 0.05
    lo_val = min(lo_val, 0.0)
    sy = lambda v: py + ph - 28 - (v - lo_val) / (hi_val - lo_val) * (ph - 56)
    sx = lambda k: px + 20 + k * (pw - 32) / len(bars)
    bw = (pw - 32) / len(bars) - 10
    if band:
        body.append(f'<rect x="{px + 16}" y="{sy(band[1]):.1f}" width="{pw - 16}" '
                     f'height="{max(sy(band[0]) - sy(band[1]), 1):.1f}" fill="rgba(45,49,66,0.08)"/>')
    for g in (lo_val, (lo_val + hi_val) / 2, hi_val):
        body.append(f'<line x1="{px + 16}" y1="{sy(g):.1f}" x2="{px + pw}" y2="{sy(g):.1f}" '
                     f'stroke="{RULE}" stroke-width="0.8"/>')
        body.append(text(px + 12, sy(g) + 3, f"{g:.2f}", anchor="end"))
    body.append(text(px, py - 8, fire, size=12, fill=INK, font=SANS, weight=600))
    for k, (label, v, sd, color, beyond) in enumerate(bars):
        if v is None:
            continue
        x = sx(k)
        fill = color if beyond else SOFT
        opacity = "0.85" if beyond else "0.35"
        body.append(f'<rect x="{x:.1f}" y="{sy(v):.1f}" width="{bw:.1f}" height="{sy(lo_val) - sy(v):.1f}" '
                     f'fill="{fill}" fill-opacity="{opacity}" stroke="{INK}" stroke-width="0.6"/>')
        if sd:
            body.append(f'<line x1="{x + bw / 2:.1f}" y1="{sy(v - sd):.1f}" x2="{x + bw / 2:.1f}" '
                         f'y2="{sy(v + sd):.1f}" stroke="{INK}" stroke-width="1"/>')
        body.append(text(x + bw / 2, sy(v) - 4, f"{v:.3f}", anchor="middle", size=6.5))
        body.append(text(x + bw / 2, py + ph - 12, label, anchor="middle", size=6.5))


def e44_arm_b_vs_e33():
    """The promotion test itself: per fire, E33 (5-seed) vs Arm B (5-seed),
    with the E33 +-1 sd band -- the exact bar E44's stop rule was judged
    against. Pier's loss and Buck's non-tie gain should read at a glance.
    """
    e33 = load("exp33_noise.json")
    armb = load("exp44_arm_b_5seed.json")
    if e33 is None or armb is None:
        print("skip e44: results not on disk")
        return
    W, H = 1080, 660
    PW, PH = 304, 220
    body = []
    for i, f in enumerate(FIRES):
        px, py = panel_grid(6, 3, PW, PH, 40, 60, 40, 56)[i]
        m33, sd33, _ = fire_mean_sd(e33, f)
        mb, sdb, _ = fire_mean_sd(armb, f)
        sd = E33_SD[f]
        beyond_b = mb is not None and m33 is not None and abs(mb - m33) > sd
        bars = [("E33", m33, sd33, INK, True),
                ("ARM B", mb, sdb, ACCENT, beyond_b)]
        band = (m33 - sd, m33 + sd) if m33 is not None else None
        holdout = any(r["fire"] == f and r.get("holdout") for r in e33)
        bar_panel(body, px, py, PW, PH, SHORT[f] + ("*" if holdout else ""), bars, band)
        body.append(text(px + PW, py - 8, f"sd(E33) {sd:.3f}", anchor="end", size=6.5))
    body.append(text(540, H - 96, "MEAN CONSENSUS IOU, 5 SEEDS: E33 VS ARM B, +-1 SD (E33) BAND",
                      anchor="middle", extra='letter-spacing="0.08em"'))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.08)"/>', "E33 +-1 sd"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="{INK}" fill-opacity="0.85" stroke="{INK}" stroke-width="0.6"/>', "E33 (5-seed mean)"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="{ACCENT}" fill-opacity="0.85" stroke="{INK}" stroke-width="0.6"/>', "Arm B, beyond E33 sd"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="{SOFT}" fill-opacity="0.35" stroke="{INK}" stroke-width="0.6"/>', "Arm B, tie"),
        (lambda x, y: f'<line x1="{x + 8}" y1="{y - 6}" x2="{x + 8}" y2="{y + 6}" stroke="{INK}" stroke-width="1"/>', "+-1 sd (that arm's own)"),
    ]))
    (FIG / "e44-arm-b-vs-e33.svg").write_text(svg(
        "e44", "E44 promotion test: Arm B vs E33, five seeds",
        "Six small multiples, one bar pair per fire: E33's five-seed mean consensus IoU against Arm B's own "
        "five-seed mean, each with its own sd whisker, against a shaded +-1 sd band around E33 (the stop rule's "
        "own tie bar). Pier's bar sits outside the band on the loss side; Brattain, Chimney and Ferguson sit "
        "outside it on the gain side.",
        W, H, "\n".join(body)))
    print("wrote e44-arm-b-vs-e33.svg")


def e45_ablations():
    """E45's two mechanism ablations against Arm B's own five-seed mean and
    sd (the noise floor E45 was judged against, not E33's).
    """
    armb_summary = load("exp44_arm_b_5seed_summary.json")
    sigma0 = load("exp45_wind_rot_mechanism_sigma0.json")
    r20 = load("exp45_wind_rot_mechanism_20.json")
    if armb_summary is None or sigma0 is None or r20 is None:
        print("skip e45: results not on disk")
        return
    arm_b_sd = armb_summary["arm_b_sd"]
    W, H = 1080, 660
    PW, PH = 304, 220
    body = []
    for i, f in enumerate(FIRES):
        px, py = panel_grid(6, 3, PW, PH, 40, 60, 40, 56)[i]
        mb, sdb = arm_b_sd[f]["mean"], arm_b_sd[f]["sd"]
        m0, sd0, _ = fire_mean_sd(sigma0, f)
        m20, sd20, _ = fire_mean_sd(r20, f)
        beyond0 = m0 is not None and abs(m0 - mb) > sdb
        beyond20 = m20 is not None and abs(m20 - mb) > sdb
        bars = [("ARM B", mb, None, INK, True),
                ("s0", m0, sd0, "#4f8a6d", beyond0),
                ("+-20", m20, sd20, ACCENT, beyond20)]
        band = (mb - sdb, mb + sdb)
        holdout = any(r["fire"] == f and r.get("holdout") for r in sigma0)
        bar_panel(body, px, py, PW, PH, SHORT[f] + ("*" if holdout else ""), bars, band)
        body.append(text(px + PW, py - 8, f"sd(ArmB) {sdb:.3f}", anchor="end", size=6.5))
    body.append(text(540, H - 96, "MEAN CONSENSUS IOU: ARM B (5-SEED) VS s0 / +-20 DEGREES (3-SEED EACH), +-1 SD (ARM B) BAND",
                      anchor="middle", extra='letter-spacing="0.06em"'))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="rgba(45,49,66,0.08)"/>', "Arm B +-1 sd"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="#4f8a6d" fill-opacity="0.85" stroke="{INK}" stroke-width="0.6"/>', "sigma0 (diversity, no learning), beyond sd"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="{ACCENT}" fill-opacity="0.85" stroke="{INK}" stroke-width="0.6"/>', "+-20 deg (learning, narrow range), beyond sd"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="{SOFT}" fill-opacity="0.35" stroke="{INK}" stroke-width="0.6"/>', "tie"),
    ]))
    (FIG / "e45-ablations.svg").write_text(svg(
        "e45", "E45 mechanism ablations: sigma0 and +-20 degrees vs Arm B",
        "Six small multiples, one bar triple per fire: Arm B's own five-seed mean consensus IoU, against the "
        "sigma0 ablation (mutation frozen, diversity kept) and the +-20 degree ablation (range narrowed, "
        "learning kept), each three-seed, against a shaded +-1 sd band around Arm B's own five-seed sd. "
        "Chimney and Bear show the two ablations pulling in opposite directions.",
        W, H, "\n".join(body)))
    print("wrote e45-ablations.svg")


def e46_three_arms():
    """E46's three station-wind arms next to their governing baselines:
    E33 for arm (a), Arm B for arms (b)/(c).
    """
    e33 = load("exp33_noise.json")
    armb_summary = load("exp44_arm_b_5seed_summary.json")
    sa = load("exp46_station_wind_input_a.json")
    sb = load("exp46_station_wind_input_b.json")
    sc = load("exp46_station_wind_input_c.json")
    if e33 is None or armb_summary is None or sa is None or sb is None or sc is None:
        print("skip e46: results not on disk")
        return
    arm_b_sd = armb_summary["arm_b_sd"]
    W, H = 1080, 700
    PW, PH = 304, 240
    body = []
    for i, f in enumerate(FIRES):
        px, py = panel_grid(6, 3, PW, PH, 40, 60, 40, 56)[i]
        m33, sd33, _ = fire_mean_sd(e33, f)
        mb, sdb = arm_b_sd[f]["mean"], arm_b_sd[f]["sd"]
        ma, sda, _ = fire_mean_sd(sa, f)
        mb2, sdb2, _ = fire_mean_sd(sb, f)
        mc, sdc, _ = fire_mean_sd(sc, f)
        sd_e33 = E33_SD[f]
        beyond_a = ma is not None and abs(ma - m33) > sd_e33
        beyond_b2 = mb2 is not None and abs(mb2 - mb) > sdb
        beyond_c = mc is not None and abs(mc - mb) > sdb
        bars = [("E33", m33, sd33, INK, True),
                ("ArmB", mb, sdb, INK, True),
                ("(a)", ma, sda, "#4f8a6d", beyond_a),
                ("(b)", mb2, sdb2, "#a0522d", beyond_b2),
                ("(c)", mc, sdc, ACCENT, beyond_c)]
        holdout = any(r["fire"] == f and r.get("holdout") for r in e33)
        bar_panel(body, px, py, PW, PH, SHORT[f] + ("*" if holdout else ""), bars, None)
        body.append(text(px + PW, py - 8, "vs. own baseline", anchor="end", size=6.5))
    body.append(text(540, H - 116, "MEAN CONSENSUS IOU, 3 SEEDS: STATION-WIND ARMS (a) BERNOULLI, (b)/(c) ARRIVAL KERNEL GENE OFF/ON",
                      anchor="middle", extra='letter-spacing="0.05em"'))
    body.append(legend(H - 28, W, [
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="{INK}" fill-opacity="0.85" stroke="{INK}" stroke-width="0.6"/>', "E33 / Arm B (their own baselines)"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="#4f8a6d" fill-opacity="0.85" stroke="{INK}" stroke-width="0.6"/>', "(a) station wind, Bernoulli, no gene"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="#a0522d" fill-opacity="0.85" stroke="{INK}" stroke-width="0.6"/>', "(b) station wind, arrival kernel, gene off"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="{ACCENT}" fill-opacity="0.85" stroke="{INK}" stroke-width="0.6"/>', "(c) station wind, arrival kernel, gene on"),
        (lambda x, y: f'<rect x="{x}" y="{y - 6}" width="16" height="12" fill="{SOFT}" fill-opacity="0.35" stroke="{INK}" stroke-width="0.6"/>', "tie vs. own baseline"),
    ]))
    (FIG / "e46-three-arms.svg").write_text(svg(
        "e46", "E46 station wind as the driver input: three arms",
        "Six small multiples, one bar group per fire: E33 and Arm B (each the governing baseline for the arms "
        "next to it), and the three station-wind arms (a: Bernoulli config, b: arrival kernel with the gene off, "
        "c: arrival kernel with the gene on), each coloured by whether it moves beyond its own baseline's sd. "
        "Ferguson's (a)/(b) gains and Chimney's (b) loss / (c) recovery are the two headline moves.",
        W, H, "\n".join(body)))
    print("wrote e46-three-arms.svg")


if __name__ == "__main__":
    FIG.mkdir(parents=True, exist_ok=True)
    e44_arm_b_vs_e33()
    e45_ablations()
    e46_three_arms()
