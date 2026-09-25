# E48 — why Brattain fails under arrival without the gene

_Round 7 (2026-09-23) · read-mostly diagnostic: seed 0, Brattain only, two
arms · new opt-in knob `SMC_DIAG=1` (per-window diagnostics, no existing
field changed) · runner `exp_r7_e48.py` → `exp48_brattain_arrival_diagnosis.json`
(+ raw `exp48_brattain_arrival_diagnosis/`) · compared against the E41
Ellipse null's own per-window series on Brattain
(`exp41_ellipse/Brattain_2020_nulls.json`, not re-run) · no score family,
this is a finding · pre-registered TEST_PLAN v1.9 · `binary_git 815d1cc`
(clean HEAD both arms) · load(1 min) 2.30 at batch launch, 3.89 at batch
finish · batch wall time 1972 s for both arms together at 2 workers
(**shared box, 2 concurrent jobs — do not compare this wall time against
any other batch's**); Arm A's own run finished about 7.6 min after Arm
B's (see "Arm A's extra wall time" below — read from the two report
files' own timestamps, not independently instrumented, so approximate)
· terms:
[GLOSSARY.md](GLOSSARY.md)_

**In short.** Half the prediction held cleanly, half only partly. ERA5's
direction really is close to the station log's on most days, but on the
three windows that carry just over half the season's burned area (days 4,
5 and 6 — hours 96, 120, 144) it swings 83°–139° away from the station
reading, far more than on any other high-growth day. Arm A (no gene)
misses catastrophically downwind of the ignition centroid at exactly the
two biggest of those three (days 5 and 6: 52,147 and 86,673 missed
cells), then spends the rest of the run over-growing in every direction
instead of catching up on the front (mean member area balloons to
1.57–1.58× the observed burned area by days 14–16) — a structural
failure, not just a
bad day's forecast. Arm B's `wind_rot_deg` gene cuts that downwind miss by
42–45% on exactly those two windows, and also avoids the late-run
over-growth (its own area ratio settles at a contained 0.61× instead of
climbing past 1.5×) — but it helps by a similar or larger margin on day 7
(41%), where ERA5 and the station agree fairly well (23.5°), so "covers
*exactly* those windows" is too strong a claim for what the data shows.
And on day 4 (hour 96, the third of the three big-disagreement windows)
the gene barely helps at all (5% miss reduction) despite that window also
having the second-worst ERA5/station disagreement of the three.

**Question.** E30b's pilot (`48-e30b-uncapped-clock-direction-gene-pilot.md`)
found that Arm A (the arrival kernel, rear-focus wind law, 4x clock and
widened prior, with no learned wind-rotation gene) posted its single worst
result — 13.2 sd below E33 — on Brattain, the one fire E41 said had the
*right* ERA5 direction and was predicted to need the least help. Arm B
(the same plus the learned `wind_rot_deg` gene) reversed this completely,
+8.1 sd above E33. That pilot had no per-window visibility into why: it
only reported mean/final consensus IoU per fire. **Why does Brattain fail
under Arm A, and what is Arm B's gene actually correcting, window by
window?**

**What we changed.** A new opt-in knob, `SMC_DIAG=1`
(`cella_lib/examples/wildfire_smc/knobs.rs`), gates a new optional field,
`diag`, on `assim` mode's per-observation report row (`ObsScore`,
`cella_lib/examples/wildfire_smc/score.rs`) — `None`/omitted from the JSON
when the knob is unset, so every report from before this knob existed
(and every report from a run that doesn't set it) is byte-identical to
before. When set, each scored window's `diag` carries:

- the ERA5 wind vector for that window (speed, compass "from" bearing,
  and the grid "toward" bearing `wind_toward_grid_deg` converts it to —
  the same vector the Ellipse null and the driver's own `rear_focus` wind
  law use);
- the station vector mean for the same window, from `station_hourly.json`
  (`station_vector_mean`, already used by the Ellipse null's station
  variant), `None` where the field is absent (`station_speed_ms`/
  `station_toward_deg`, both present here — Brattain has a station log);
- the ensemble's per-window learned-gene medians: `model.p0`,
  `wind_scale`, and (Arm B only — absent, not zero, under Arm A, which has
  no such gene) `wind_rot_deg`. `Ensemble::genome_stats` already reports
  mean/sd/min/max per gene, but not a median, so a small new function
  (`crate::diag::median_gene`) reads it off `Ensemble::genomes()` instead;
- a head-vs-flank decomposition of the miss: every cell where the
  consensus mask (probability ≥ 0.5, the same threshold `consensus_iou`
  already scores) disagrees with the truth mask is classified by whether
  its displacement from the ignition centroid has a positive dot product
  with the window's ERA5 "toward" vector — "downwind" if so, "cross-wind"
  otherwise (this second bucket also catches ties and anything upwind;
  the pre-registered question only asks for two buckets). Four counts per
  window: `downwind_miss`, `crosswind_miss` (truth burned, consensus
  didn't call it), `downwind_false_positive`, `crosswind_false_positive`
  (consensus called it, truth didn't).

New module `cella_lib/examples/wildfire_smc/diag.rs` holds
`median_gene`, `centroid` and `head_flank_decompose`, each unit-tested;
`score.rs` carries a test that `diag: None` never serialises a `"diag"`
key. Wired into the scoring loop in
`cella_lib/examples/wildfire_smc/modes/open.rs` with about a dozen lines
(the station log is loaded once per run via a small refactor —
`crate::nulls::load_station`, factored out of `nulls::run_nulls` so both
call sites share the exact same `SMC_STATION_WIND`-or-`station_hourly.json`
lookup instead of duplicating it).

Two runs, seed 0, Brattain only: **Arm A** (arrival kernel, rear-focus
wind law, 4x clock, `arrival_x4.json` prior — Arm B's own preset minus
`SMC_WIND_ROT_GENE`) and **Arm B** (the same plus `SMC_WIND_ROT_GENE=90`),
both with `SMC_DIAG=1`, `assim` mode, 32 members, 2 workers, niced,
one batch, shared-machine rules per the Round 7 plan.

**Why we expected it to matter.** If Arm A's Brattain miss is concentrated
in a few windows rather than spread evenly across the whole run, that
would explain a large mean-IoU loss without contradicting "the daily-mean
ERA5 direction is right" (E41 scores the *whole* Ellipse series, which can
still average out a few bad windows). And if the windows Arm B's gene
diversity helps most on are the same windows the head/flank decomposition
flags as downwind-miss-heavy, that would be direct, window-level evidence
that the gene is doing more than adding noise — it is correcting the
input on exactly the days it is wrong, which the E30b pilot could only
infer indirectly from the small (8.6°–43.8°) learned median rotations at
the *end* of the run.

**How we scored it.** No score family — this experiment reads and reports
per-window diagnostics, it does not compute or compare an IoU/Brier
metric family against a noise floor. For each arm and each scored window:
`consensus_iou` (already reported, read here for context), the ERA5 and
station wind vectors, the learned p0/wind_scale (Arm B: + wind_rot_deg)
medians, and the four head/flank counts. The E41 Ellipse null's own
per-window `ellipse_era5_iou` series on Brattain (`ellipse_era5` variant —
ERA5 wind, unchanged from this campaign's default input) is read
alongside, not re-run, to see whether the null's own per-window skill
(rather than its 21-window mean) tracks the same windows.

**Prediction, written before the run.** ERA5 direction is right on the
daily mean but wrong on the two or three windows that carry most of the
burned area, and Arm B's gene diversity covers exactly those windows.

**Result — reproduction check.** Both runs' `mean_consensus_iou` (0.3625
Arm A, 0.4477 Arm B) match the E30b pilot's own Brattain numbers (0.363,
0.448) to three decimal places at the same seed and preset — the
`SMC_DIAG=1` wiring changes nothing about the simulation itself, only adds
the new field, exactly as intended.

**Result — the windows that carry the burned area.** Of Brattain's
226,193 total burned cells, three single windows carry just over half:

| day | hours | Δburn (cells) | % of total | ERA5 vs station disagreement |
|---|---|---|---|---|
| 5 | 120 | 47,227 | 20.9% | **139.3°** |
| 6 | 144 | 39,470 | 17.4% | **82.7°** |
| 4 | 96 | 27,862 | 12.3% | **96.6°** |
| 7 | 168 | 24,396 | 10.8% | 23.5° |
| 3 | 72 | 18,708 | 8.3% | 54.6° |

Days 4, 5 and 6 (hours 96/120/144) are the three windows that carry the
most burned area (50.6% of the season's total between them) — and they
are also the three windows, among every window with meaningful growth,
with by far the largest disagreement between the ERA5 "toward" bearing
and the station vector mean (83°–139°, vs. 14.1°–54.6° on every other
window with ≥ 3,000 cells of growth). Two windows later in the run (hours
552, 576) show even larger disagreement (144°, 125°) but essentially no
growth left to get wrong (34 and 24 cells). **This is the strongest, most
literal reading of the prediction's first clause, and it held.**

**Result — Arm A (no gene), every scored window.**

| day | hours | obs burned | Δburn | consensus IoU | Ellipse(E41) IoU | ERA5 toward° | station toward° | disagree° | p0 median | wind× median | downwind miss | crosswind miss | downwind FP | crosswind FP | area ratio |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 24 | 14,397 | 14,397 | 0.177 | 0.185 | 32.8 | 74.5 | 41.8 | 0.083 | 0.687 | 1,118 | 10,632 | 585 | 0 | 0.31 |
| 2 | 48 | 27,859 | 13,462 | 0.160 | 0.261 | 139.9 | 100.8 | 39.1 | 0.162 | 0.703 | 20,532 | 2,567 | 1,836 | 1 | 0.38 |
| 3 | 72 | 46,567 | 18,708 | 0.307 | 0.570 | 182.5 | 127.9 | 54.6 | 0.276 | 0.421 | 27,794 | 3,603 | 0 | 2,853 | 0.83 |
| 4 | 96 | 74,429 | 27,862 | 0.507 | 0.612 | 215.5 | 118.8 | 96.6 | 0.417 | 0.360 | 7,296 | 1,786 | 27,025 | 27,551 | 1.50 |
| 5 | 120 | 121,656 | 47,227 | 0.468 | 0.507 | 318.0 | 178.7 | 139.3 | 0.288 | 0.431 | 52,147 | 7,701 | 0 | 10,421 | 1.08 |
| 6 | 144 | 161,126 | 39,470 | 0.358 | 0.548 | 331.1 | 53.7 | 82.7 | 0.238 | 0.496 | 86,673 | 14,578 | 0 | 5,961 | 0.70 |
| 7 | 168 | 185,522 | 24,396 | 0.364 | 0.523 | 295.3 | -41.2 | 23.5 | 0.239 | 0.628 | 92,932 | 19,871 | 0 | 14,432 | 0.76 |
| 8 | 192 | 200,001 | 14,479 | 0.349 | 0.505 | 291.4 | -82.7 | 14.1 | 0.293 | 0.759 | 98,858 | 19,353 | 1,378 | 32,903 | 0.80 |
| 9 | 216 | 213,521 | 13,520 | 0.329 | 0.486 | 288.9 | -93.6 | 22.5 | 0.294 | 0.785 | 108,758 | 19,530 | 3,014 | 42,397 | 0.80 |
| 10 | 240 | 217,020 | 3,499 | 0.429 | 0.482 | 286.7 | -88.1 | 14.9 | 0.315 | 0.797 | 81,331 | 10,796 | 16,609 | 57,349 | 1.04 |
| 11 | 264 | 225,585 | 8,565 | 0.404 | 0.470 | 283.5 | -60.8 | 15.7 | 0.354 | 0.831 | 79,496 | 8,586 | 47,896 | 66,694 | 1.16 |
| 12 | 288 | 225,718 | 133 | 0.397 | 0.470 | 292.5 | -94.2 | 26.7 | 0.300 | 1.126 | 74,166 | 6,946 | 60,822 | 77,545 | 1.34 |
| 13 | 312 | 225,800 | 82 | 0.377 | 0.470 | 359.6 | -12.0 | 11.6 | 0.263 | 1.046 | 10,649 | 69,152 | 84,454 | 76,871 | 1.41 |
| 14 | 336 | 225,887 | 87 | 0.351 | 0.470 | 54.7 | 98.2 | 43.4 | 0.275 | 1.102 | 4,507 | 73,033 | 108,178 | 89,054 | 1.57 |
| 15 | 360 | 225,949 | 62 | 0.340 | 0.470 | 324.0 | -47.5 | 11.5 | 0.254 | 0.905 | 65,402 | 10,546 | 94,532 | 120,656 | 1.58 |
| 16 | 384 | 225,976 | 27 | 0.334 | 0.470 | 330.1 | -90.9 | 60.9 | 0.239 | 0.969 | 52,925 | 22,182 | 107,149 | 118,473 | 1.57 |
| 17 | 408 | 226,018 | 42 | 0.378 | 0.470 | 302.6 | -89.2 | 31.8 | 0.256 | 0.802 | 75,829 | 4,540 | 57,608 | 102,015 | 1.52 |
| 19 | 456 | 226,107 | 89 | 0.395 | 0.470 | 322.7 | -51.1 | 13.8 | 0.250 | 0.749 | 76,950 | 10,117 | 41,929 | 83,957 | 1.31 |
| 22 | 528 | 226,135 | 28 | 0.395 | 0.470 | 353.2 | 61.7 | 68.5 | 0.261 | 1.041 | 17,272 | 69,823 | 58,429 | 67,457 | 1.24 |
| 23 | 552 | 226,169 | 34 | 0.397 | 0.470 | 221.0 | 77.4 | 143.6 | 0.306 | 0.867 | 79,308 | 7,894 | 58,242 | 65,480 | 1.14 |
| 24 | 576 | 226,193 | 24 | 0.397 | 0.470 | 292.4 | 57.7 | 125.3 | 0.306 | 0.865 | 80,375 | 6,851 | 46,790 | 76,932 | 1.13 |

**Result — Arm B (arrival kernel + `wind_rot_deg` gene), every scored window.**

| day | hours | obs burned | Δburn | consensus IoU | Ellipse(E41) IoU | ERA5 toward° | station toward° | disagree° | p0 median | wind× median | wind_rot° median | downwind miss | crosswind miss | downwind FP | crosswind FP | area ratio |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 24 | 14,397 | 14,397 | 0.200 | 0.185 | 32.8 | 74.5 | 41.8 | 0.133 | 0.698 | -1.1 | 931 | 10,442 | 727 | 0 | 0.37 |
| 2 | 48 | 27,859 | 13,462 | 0.177 | 0.261 | 139.9 | 100.8 | 39.1 | 0.183 | 0.478 | -17.1 | 20,333 | 2,060 | 2,833 | 144 | 0.48 |
| 3 | 72 | 46,567 | 18,708 | 0.571 | 0.570 | 182.5 | 127.9 | 54.6 | 0.436 | 0.542 | 44.1 | 12,585 | 1,511 | 3,853 | 6,466 | 1.00 |
| 4 | 96 | 74,429 | 27,862 | 0.679 | 0.612 | 215.5 | 118.8 | 96.6 | 0.527 | 0.828 | 33.3 | 6,933 | 2,922 | 14,252 | 6,420 | 1.17 |
| 5 | 120 | 121,656 | 47,227 | 0.550 | 0.507 | 318.0 | 178.7 | 139.3 | 0.476 | 0.883 | 13.1 | 30,225 | 3,905 | 1,321 | 36,020 | 1.03 |
| 6 | 144 | 161,126 | 39,470 | 0.507 | 0.548 | 331.1 | 53.7 | 82.7 | 0.434 | 0.980 | 26.9 | 47,678 | 5,533 | 2,459 | 49,276 | 1.02 |
| 7 | 168 | 185,522 | 24,396 | 0.481 | 0.523 | 295.3 | -41.2 | 23.5 | 0.513 | 1.068 | 29.2 | 55,096 | 10,712 | 11,890 | 51,695 | 1.12 |
| 8 | 192 | 200,001 | 14,479 | 0.466 | 0.505 | 291.4 | -82.7 | 14.1 | 0.475 | 0.995 | 41.3 | 75,252 | 13,228 | 6,308 | 32,896 | 1.05 |
| 9 | 216 | 213,521 | 13,520 | 0.464 | 0.486 | 288.9 | -93.6 | 22.5 | 0.473 | 1.187 | 64.5 | 88,023 | 14,227 | 6,839 | 19,604 | 0.93 |
| 10 | 240 | 217,020 | 3,499 | 0.457 | 0.482 | 286.7 | -88.1 | 14.9 | 0.483 | 1.236 | 29.9 | 90,919 | 14,830 | 7,302 | 19,141 | 0.78 |
| 11 | 264 | 225,585 | 8,565 | 0.442 | 0.470 | 283.5 | -60.8 | 15.7 | 0.440 | 1.112 | 55.3 | 98,729 | 15,585 | 7,939 | 18,504 | 0.70 |
| 12 | 288 | 225,718 | 133 | 0.441 | 0.470 | 292.5 | -94.2 | 26.7 | 0.477 | 1.151 | 43.1 | 101,403 | 13,044 | 6,037 | 20,406 | 0.65 |
| 13 | 312 | 225,800 | 82 | 0.441 | 0.470 | 359.6 | -12.0 | 11.6 | 0.470 | 1.016 | 40.1 | 31,870 | 82,659 | 8,799 | 17,644 | 0.61 |
| 14 | 336 | 225,887 | 87 | 0.441 | 0.470 | 54.7 | 98.2 | 43.4 | 0.454 | 0.899 | 17.0 | 20,845 | 93,771 | 13,423 | 13,020 | 0.61 |
| 15 | 360 | 225,949 | 62 | 0.441 | 0.470 | 324.0 | -47.5 | 11.5 | 0.441 | 0.732 | 38.5 | 98,084 | 16,594 | 979 | 25,464 | 0.61 |
| 16 | 384 | 225,976 | 27 | 0.441 | 0.470 | 330.1 | -90.9 | 60.9 | 0.407 | 1.027 | 18.4 | 83,545 | 31,160 | 1,688 | 24,755 | 0.61 |
| 17 | 408 | 226,018 | 42 | 0.441 | 0.470 | 302.6 | -89.2 | 31.8 | 0.424 | 0.773 | 8.7 | 106,300 | 8,447 | 3,472 | 22,971 | 0.61 |
| 19 | 456 | 226,107 | 89 | 0.441 | 0.470 | 322.7 | -51.1 | 13.8 | 0.428 | 0.777 | 31.7 | 101,071 | 13,765 | 944 | 25,499 | 0.61 |
| 22 | 528 | 226,135 | 28 | 0.441 | 0.470 | 353.2 | 61.7 | 68.5 | 0.451 | 0.922 | 19.1 | 37,087 | 77,777 | 7,596 | 18,847 | 0.61 |
| 23 | 552 | 226,169 | 34 | 0.440 | 0.470 | 221.0 | 77.4 | 143.6 | 0.422 | 0.890 | 24.0 | 92,225 | 22,673 | 14,997 | 11,446 | 0.61 |
| 24 | 576 | 226,193 | 24 | 0.440 | 0.470 | 292.4 | 57.7 | 125.3 | 0.407 | 1.123 | 11.1 | 101,819 | 13,103 | 6,073 | 20,370 | 0.61 |

How to read it: "disagree°" is the circular difference between
`era5_toward_deg` and `station_toward_deg` (0° = agree exactly, 180° =
opposite). "downwind miss"/"crosswind miss" are truth-burned cells the
0.5-threshold consensus mask didn't call, split by whether their
displacement from the *fixed ignition centroid* has a positive dot
product with that window's ERA5 "toward" vector; "downwind/crosswind FP"
are the mirror image (consensus called it, truth didn't). "area ratio" is
the pre-existing `area_ratio_mean` field (mean member burned area /
observed burned area), read here as a proxy for how large a burning set
each arm's ensemble is carrying. Both arms see the *same* truth and ERA5
columns by construction (only the gene differs), so those columns are
listed once conceptually but repeated per table for a self-contained
read.

**Prediction vs. result, clause by clause.**

- *"ERA5 direction is right on the daily mean but wrong on the two or
  three windows that carry most of the burned area."* **Held.** The E41
  Ellipse null's per-fire mean Ellipse IoU on Brattain (0.47 pooled over
  the full series, the number E41 called "right" and this campaign's best
  or near-best of the six fires) is consistent with "right on average" —
  most windows (14 of 21) disagree with the station reading by 55° or
  less. But
  the three windows that carry the most burned area (days 4/5/6, 50.6% of
  the season's total between them) are precisely the three worst-agreeing
  windows among every window with meaningful growth (83°–139°, more than
  double the next-worst substantial-growth window). No cherry-picking was
  needed to find this pattern — sorting all 21 windows by disagreement and
  separately by burned-area share puts the same three windows at the top
  of both lists (mixed in with two very-late, near-zero-growth windows
  that also disagree badly but don't matter for area).
- *"Arm B's gene diversity covers exactly those windows."* **Partly
  held.** On two of the three (days 5 and 6, hours 120/144), Arm B's
  downwind miss drops sharply against Arm A's: 52,147 → 30,225 (−42.0%)
  and 86,673 → 47,678 (−45.0%). But on the third (day 4, hour 96 — the
  *second*-worst-disagreeing window of the three), the drop is only
  5.0% (7,296 → 6,933) — Arm A's own downwind miss is comparatively small
  there to begin with (7,296, an order of magnitude below days 5/6), so
  there is less for the gene to fix, but that also means the correspondence
  isn't "the gene fixes the three bad-direction windows" so much as "the
  gene fixes the two windows where Arm A was *also* badly wrong in an
  absolute sense." And the gene's benefit isn't confined to the
  bad-direction windows either: day 7 (hour 168, disagreement only 23.5°)
  shows a comparable 40.7% downwind-miss reduction, and days 8–9 (14–23°
  disagreement) still show 19–24% reductions. **"Exactly those windows"
  overstates it** — the gene helps most, in absolute terms, on the two
  biggest-growth/worst-disagreement windows, but it is not narrowly
  confined to windows where ERA5 and the station disagree; it also erodes
  Arm A's advantage steadily through the whole first half of the run, and
  past day 10 (once growth has essentially stopped) the sign of the
  effect flips — Arm B's downwind miss becomes *worse* than Arm A's on
  most of the remaining, near-zero-growth windows, which does not bear on
  the prediction (nothing is left to miss by then) but is worth flagging
  as a real reversal, not noise: of the 11 scored windows from day 11 on,
  `Δburn` is under 150 cells for 10 of them (every one but day 11
  itself).

**What it means.** The causal chain E30b's pilot couldn't see is visible
here. Arm A's arrival kernel, driven only by the fixed ERA5 daily-mean
direction, fails hardest at exactly the two windows where the real fire
made its two largest single-day jumps (days 5 and 6, a combined 38.3% of
the season's total growth) *and* where ERA5 disagrees most with the
station log (139°, 83°) — the model's downwind push simply points the
wrong way on the days the fire is moving fastest. Arm A cannot make this
up later: instead of relocating its front, its ensemble inflates in every
direction (mean member area climbs from ~0.7–0.8× observed at days 6–9 to
a peak of 1.58× by day 15), which is visible directly in the head/flank
counts — by day 16, Arm A's downwind + crosswind false positives
(107,149 + 118,473 = 225,622) very nearly equal Brattain's *entire*
226,193-cell burned area, i.e. Arm A is close to painting the whole
domain "burning" while still under-covering the correct downwind extent.
This is a structural failure of a single fixed direction, not a
borderline miss: no amount of `p0`/`wind_scale` tuning inside one
direction can fix a front that's pointed the wrong way on the two days
that matter most.

Arm B's gene does two separable things, and the data only cleanly
supports one of them as "targeted." It substantially recovers the
downwind miss specifically on the two catastrophic days (−42%/−45% on
days 5/6), which is real, large, and lines up with the prediction. But it
also avoids Arm A's late-run over-growth altogether — Arm B's area ratio
*falls* from ~0.93–1.17× (days 3–9) to a flat 0.61× for the entire back
half of the run (days 13–24), rather than climbing past 1.5× the way Arm
A's does. That second effect looks less like "the gene corrected the
wind on the bad days" and more like "a population that isn't dragged
badly off-course early on settles into a smaller, better-contained state
later and stays there" — a knock-on consequence of the early recovery,
not a second, independent direction fix. The learned `wind_rot_deg`
*median* itself doesn't spike at days 5/6 (13.1°, 26.9° — smaller than
several later, low-stakes windows like day 9's 64.5°), so whatever Arm B
is doing at the two critical windows looks more like per-member angular
*diversity* letting different members catch different parts of the true
downwind extent (some members happen to be rotated toward wherever the
real fire actually went that day) than a population-wide *learned*
correction — consistent with the E30b pilot's own read of its small final
rotations, and exactly the diversity-vs-learning question E45
(`SMC_WIND_ROT_SIGMA=0`) is designed to separate. This experiment cannot
tell diversity and learning apart on its own — it has no per-window
spread/IQR of `wind_rot_deg`, only the median — so this is a plausible
reading of the data, not a proven mechanism.

**Arm A's extra wall time, read from the report files' timestamps (not
independently instrumented — read with the "shared box, do not compare
across batches" caveat, though this comparison is *within* one batch's
own two concurrent jobs, not across batches).** Arm B's report file was
written at 20:25:10, Arm A's at 20:32:49 — about 7.6 minutes later, and
consistent with the batch's own total wall time (1971.9 s ≈ 32.9 min,
matching Arm A as the last job to finish) if both jobs started close to
the load-gate check at 19:59:51. That gives roughly 25 min for Arm B and
33 min for Arm A, both inside the pilot's 37–48 min/arm range for a full
Brattain run (this ran at 2 workers, not 4, so faster than the pilot on
its own is expected and is not a wall-time comparison across batches).
The `area_ratio_mean` trajectory above gives a concrete, numbers-backed
reason for the gap rather than a guess: Arm A's mean member burned area
climbs to 1.52–1.58× the observed area for days 14–17 (already 1.34–1.41×
on days 12–13, just before) of a 24-window run (more burning/burned
cells for the cellular-automaton step to update
every tick, for a large fraction of the run), while Arm B's settles at a
much smaller, flat 0.61× for the same stretch (days 13–24). A larger,
still-growing burning set costs more compute per tick under this driver,
which plausibly accounts for most of the ~30% wall-time gap between the
two arms without needing to invoke anything about the diagnostics code
itself (`head_flank_decompose` and `median_gene` are both O(grid cells)
or O(members) per window, negligible next to the driver's own per-tick
cost either way).

**Questions this raises.**

- Is the gene's day-5/6 recovery really per-member diversity rather than
  population-level learning? E45 (`SMC_WIND_ROT_SIGMA=0`, diversity with
  no learning) is designed to answer exactly this, but a cheaper partial
  answer specific to Brattain would be adding a per-window spread/IQR of
  `wind_rot_deg` to `diag` (a small addition to `WindowDiag`) and re-
  reading these same two report files' `final_genomes`-adjacent state —
  not done here, since the brief asked for medians, not spread. Open.
- Why does day 4 (hour 96) — the second-worst ERA5/station disagreement
  of the whole series — get so little benefit from the gene, when the
  two worse-disagreeing neighbours get so much? One candidate: Arm A's
  own downwind miss there is already small in absolute terms (7,296,
  plus 1,786 cross-wind miss), versus 54,576 combined downwind + cross-
  wind false positives the same window (27,025 + 27,551) — the window's
  dominant error for Arm A is a burst of over-prediction, not an
  under-prediction, so there is little "downwind miss" left for a
  direction gene to fix. Not confirmed here. Open.
- The late-run reversal (Arm B's downwind miss exceeding Arm A's from day
  11 on) coincides with `Δburn` collapsing to near zero — is this a real
  effect of the gene's diversity spreading members' fronts past a
  fire that has already stopped growing, or an artifact of scoring a
  near-static truth mask against two different ensemble states with no
  state correction (`SMC_STATE_CORRECTION` was left off, per the Round 7
  plan)? Open; likely low-stakes for the campaign's own scoring (consensus
  IoU is flat for both arms over this stretch) but worth a look if a
  future task reads `downwind_miss` on a mostly-contained fire.
- Does this same pattern (2–3 disagreement windows carrying most of the
  growth, Arm A missing hardest exactly there) show up on the other five
  fires, or is Brattain's clean top-3-overlap a coincidence of this one
  fire's growth curve? Not tested — this experiment is Brattain-only, by
  design (read-mostly, one fire, the fire the question was about). Open.
- On the 139°-off window (day 5), a "ERA5 points the wrong way" story
  would seem to predict a crosswind-heavy miss, not a downwind-heavy one
  — yet the miss there is overwhelmingly counted `downwind_miss`
  (post-hoc, added during the whole-branch review, no new numbers run).
  The reconciling detail is what "downwind" means to
  `diag::head_flank_decompose`: a cell counts as downwind whenever its
  vector from the fixed ignition centroid has a positive dot product
  with ERA5's own toward-bearing — a 180°-wide half-plane test around
  ERA5's axis, not a narrow cone around it. A 139° rotation of that axis
  still leaves most of the original half-plane overlapping the rotated
  one (only a rotation approaching 180° would flip the classification
  for most cells), so this classifier is too coarse to distinguish "the
  fire grew toward the true wind" from "the fire grew toward ERA5's
  wrong one" on a single ~139° window — both would read mostly
  `downwind_miss` under ERA5's own axis. A narrower angular bucket (e.g.
  quadrants instead of a half-plane) would be needed to test the
  wrong-direction story directly. Open.

**Verdict.** Finding, not a score-family test. Clause 1 of the
pre-registered prediction ("ERA5 direction is right on the daily mean but
wrong on the two or three windows that carry most of the burned area")
**held** — the three windows carrying 50.6% of Brattain's total burned
area are the same three (among all meaningfully-growing windows) with by
far the largest ERA5/station disagreement. Clause 2 ("Arm B's gene
diversity covers exactly those windows") **partly held** — the gene
delivers its single largest, clearest benefit on two of the three
(days 5/6, −42%/−45% downwind miss), but "exactly" overclaims: the third
window (day 4) barely benefits, and the gene helps by a comparable or
larger margin on lower-disagreement windows too (day 7 especially). The
mechanism this experiment can actually support is narrower and more
useful than the prediction's "exactly those windows" framing: Arm A fails
on Brattain because a single fixed direction is badly wrong on the fire's
two biggest growth days, and it cannot recover by growing more (it just
over-predicts everywhere, days 14–17 — up to 1.58× observed); Arm B does
not fully fix the direction on those two days either (downwind miss stays
substantial, 30,225 and 47,678 cells) but reduces it enough, and avoids
the late over-growth, to end up far ahead on final IoU. Not a lever test
— no configuration change follows from this file alone.

**Later.** E45 (`SMC_WIND_ROT_SIGMA=0`) is the natural next step to
separate diversity from learning in this same gene, on all six fires; if
it runs on Brattain specifically, its own per-window read (if instrumented
the same way, e.g. by extending `WindowDiag` with a spread field) could
directly test whether it reproduces the days-5/6 downwind-miss recovery
seen here with the sigma-0 (diversity-only) variant.
