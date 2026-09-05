# E34 — operator ablation inside the filter · finding — the operators sit on a plateau; what moves the score is when the population locks in

_Round: Round 5 — 2026-09-05: the methods themselves_

**Why.** E25 chose β 10, σ 0.2 and 20 % immigrants from one run each and
called immigrants "the diversity floor worth keeping". With the noise
floor known (E33) and the stopping rule changed since (E28), which
operator actually carries the filter's gain? And is genome crossover,
which the offline GA uses and the filter never had, worth adding?

**Method.** `exp_r5_operators.py`: one knob moved at a time from the base
configuration (β 10, σ 0.2, immigrants 0.2, containment only, M 32, seed
0): immigrants 0 / 0.1 / 0.4, σ 0.1 / 0.4, β 5 / 20, and crossover 0.5 /
1.0 (new: a resampled child takes each gene from either of two parents
before mutation). 54 runs. Judged against E33's per-fire sd.

![Matrix of operator settings against fires: change in mean forecast IoU, ties faint](figures/e34-operators.svg)

Change in mean consensus IoU from the base (bold = beyond the fire's sd):

| setting | Bear | Brattain | Buck | Chimney | Ferguson* | Pier* |
|---|---|---|---|---|---|---|
| immigrants 0 | −0.001 | −0.001 | −0.016 | −0.009 | −0.007 | −0.002 |
| immigrants 0.1 | **−0.046** | +0.001 | +0.011 | **−0.027** | **−0.013** | −0.003 |
| immigrants 0.4 | −0.003 | **−0.007** | +0.012 | −0.009 | **−0.033** | **+0.004** |
| σ 0.1 | −0.002 | +0.004 | −0.009 | −0.006 | **−0.016** | 0.000 |
| σ 0.4 | −0.004 | **−0.006** | +0.003 | 0.000 | **−0.032** | **−0.008** |
| β 5 | +0.005 | −0.003 | +0.013 | +0.002 | **−0.023** | +0.002 |
| β 20 | +0.006 | **+0.007** | +0.024 | **−0.016** | −0.004 | −0.001 |
| crossover 0.5 | **+0.027** | 0.000 | +0.014 | −0.007 | −0.007 | **+0.004** |
| crossover 1.0 | +0.009 | **+0.007** | −0.007 | −0.010 | **−0.017** | +0.003 |
| E33 sd | 0.015 | 0.004 | 0.039 | 0.012 | 0.007 | 0.003 |

(*holdout. Brier: β 5 is best on Bear, Ferguson and Pier and β 20 worst
on four fires, as in E25; crossover 0.5 has the best Buck Brier (0.0420)
and the worst Brattain (0.1167); immigrants 0 is *better* than the base
on Chimney (0.1203 vs 0.1355) and Buck. Effective sample size 28–31 in
every setting except β 20 (26–29).)

**Findings.**

- **Thirty-eight of fifty-four cells are ties.** No setting beats the
  base on more than two fires; none loses on more than three. With the
  containment operator doing the stopping, the filter is not sensitive to
  its GA operators within a factor of two either way. E25's preference
  for 20 % immigrants was a single-seed reading: immigrants 0 is a tie on
  all six fires here.
- **Ferguson is the one fire that punishes noise in the genome.** It
  loses under σ 0.4 (−0.032), immigrants 0.4 (−0.033), β 5 (−0.023), σ 0.1
  and crossover 1.0. Ferguson's answer (p0 ≈ 0.35, the top of what the
  filter finds anywhere) sits near the edge of the prior, and each of
  those settings dilutes a population that has found it: fresh draws
  land far below, big mutations wander off, soft selection keeps the
  stragglers. The one fire that needed learning most is the one that
  needs the learned state protected.
- **The large cells are lock-in events, not operator effects.** Bear under
  immigrants 0.1 (−0.046, three sd) tracks the base for eight days and
  then decays to a flat 0.37 from day 13 on, every member contained; the
  base holds 0.46. Immigrants 0 and 0.4 on the same fire are ties, so the
  0.1 result cannot be a dose effect. **Every one of the 54 runs ends with
  100 % of members contained and a per-day score that is flat for the
  last five to ten days.** The score at which a population freezes is
  decided by the containment dice in the days before, and that is a
  larger source of spread than any operator: it is what E33 saw on Buck
  seed 3, and it is why E38 exists.
- **Crossover 0.5 is the only setting that gains beyond the bar on more
  than one fire** (Bear +0.027, Pier +0.004, Buck +0.014 inside its sd,
  nothing lost). The Bear gain is a late-fire effect too (its tail sits at
  0.51 against the base's 0.46), so it may be the same dice in a kinder
  mood. One seed cannot separate the two; it earns replicates, not a
  default change.
- β 20 buys Buck +0.024 and Brattain +0.007 for Chimney −0.016 and the
  worst calibration, repeating E25: sharper selection sharpens the wrong
  things as often as the right ones.

**Verdict.** Keep β 10, σ 0.2, immigrants 0.2 (they are on a plateau, and
0.2 immigrants is a cheap insurance against a bad prior, E35). Do not add
crossover to the default yet; replicate crossover 0.5 over five seeds
alongside E38 before deciding. The operator question is closed for now;
the open question is the lock-in, which no operator setting touches.
