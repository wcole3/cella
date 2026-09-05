# E32 — ensemble size: 8 to 128 members · finding — skill flattens at 32 on five fires; Buck alone keeps gaining, and Brier keeps improving

_Round: Round 5 — 2026-09-05: the methods themselves_

**Why.** 32 members was a guess in E24 and never questioned. Members cost
memory and time linearly, and a probability map from 8 members has steps
of 12.5 %. Where does forecast skill stop improving, and does calibration
(Brier) stop at the same place?

**Method.** `exp_r5_members.py`: the recommended configuration with M = 8,
16, 64 and 128 members (M = 32 is E33 seed 0), one seed each, all six
fires. Judged against E33's per-fire sd. Wall time and memory scale with
M: the 128-member runs took four times the 32-member ones.

![Small multiples: mean forecast IoU against members, one panel per fire, with the Circle null and the E33 noise band](figures/e32-members.svg)

| Fire | 8 | 16 | 32 | 64 | 128 | E33 sd | Circle |
|---|---|---|---|---|---|---|---|
| Bear | 0.409 | 0.460 | 0.478 | **0.497** | 0.493 | 0.015 | 0.541 |
| Brattain | 0.404 | 0.409 | 0.415 | 0.414 | 0.413 | 0.004 | 0.450 |
| Buck | 0.552 | 0.577 | 0.600 | 0.616 | **0.630** | 0.039 | 0.670 |
| Chimney | 0.422 | 0.419 | 0.446 | 0.435 | 0.438 | 0.012 | 0.372 |
| Ferguson* | 0.318 | 0.339 | 0.353 | 0.344 | 0.349 | 0.007 | 0.373 |
| Pier* | 0.527 | 0.537 | 0.535 | 0.535 | 0.537 | 0.003 | 0.559 |

Mean Brier (lower is better; Circle in the last column):

| Fire | 8 | 16 | 32 | 64 | 128 | Circle |
|---|---|---|---|---|---|---|
| Bear | 0.0571 | 0.0501 | 0.0473 | 0.0480 | 0.0477 | 0.0547 |
| Brattain | 0.1069 | 0.1125 | 0.1051 | 0.1125 | 0.1153 | 0.1229 |
| Buck | 0.0513 | 0.0475 | 0.0480 | 0.0428 | **0.0411** | 0.0408 |
| Chimney | 0.1177 | 0.1158 | 0.1355 | 0.1253 | 0.1258 | 0.1591 |
| Ferguson* | 0.1452 | 0.1410 | 0.1360 | 0.1349 | **0.1327** | 0.1675 |
| Pier* | 0.1137 | 0.1099 | 0.1120 | 0.1092 | **0.1068** | 0.1047 |

(*holdout. Effective sample size after learning stayed at 92–96 % of M
at every size, so the filter is not collapsing at any M.)

**Findings.**

- **8 members is clearly too few**: −0.03 to −0.07 IoU against 32 on
  Bear, Buck, Ferguson and Chimney, all well past the bar, and the worst
  Brier on four fires. The consensus of eight coin-flips is not a
  probability map.
- **Skill flattens at 32 on five of six fires.** From 32 to 128, Brattain,
  Chimney, Ferguson and Pier move by less than their sd; Bear gains 0.019
  at 64 (one sd) and holds. Doubling members past 32 buys nothing
  detectable in IoU on these fires.
- **Buck keeps gaining**: 0.600 → 0.616 → 0.630, monotone over five sizes,
  and its Brier falls from 0.0480 to 0.0411, reaching the Circle's 0.0408.
  The IoU gain (+0.030) is inside Buck's own sd (0.039), so by the bar it
  is a tie, but a monotone trend over five points plus a matching Brier
  trend is not what noise looks like. Buck is the fire whose members
  disagree most about *when* it stops (E33's outlier seed is Buck), and
  more members average that disagreement better.
- **Brier improves past 32 more reliably than IoU**: 128 is best or
  within 0.001 of best on Bear, Buck, Ferguson and Pier. Calibration
  keeps benefiting from members after the 0.5-threshold map has stopped.
  Chimney's 32-member Brier (0.1355) is the outlier here and is seed
  noise: E33's Chimney Brier sd is 0.0065 and this seed sits at the top of
  that spread.

**Verdict.** Keep 32 as the default: it is the knee for the consensus map
on five of six fires. Use 64–128 when the product is the probability map
itself (Brier keeps improving) or when members disagree about stopping,
as on Buck; never go below 16. Future comparisons of methods can stay at
32 without fear of being member-limited.
