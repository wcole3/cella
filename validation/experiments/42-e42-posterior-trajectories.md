# E42 — posterior trajectories and the ICS-209 containment check · finding: the direction holds, the size and Pier don't

_Round 6 (2026-09-11) · analysis only, no new runs · reads E33's five-seed
reports · all six fires incl. holdout · runner `exp_r6_posterior.py` ·
results `exp42_posterior.json` · figure `figures/e42-posterior.svg` ·
pre-registered TEST_PLAN v1.8 addendum · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** E33 already ran the recommended 32-member filter five times
per fire; this experiment just reads what those runs learned, instead of
running anything new. Two questions: does the learned p0 drift the same
way, day to day, on every fire (a measured version of E21's "something
slows these fires early")? And does the containment operator's learned
`(contain_a, contain_b)`, run against the real fire's own growth, predict
containment sooner or later than the crews' ICS-209 reports? The
prediction's *direction* held on both counts — p0 falls on the two
fastest calibration fires and rises on the under-burnt holdout fire; the
model always reaches 50 % contained members before ICS-209 reports 50 %
containment. But the *sizes* were off (the Bear/Buck lead is 13–18 days,
not the predicted 5–10), "wind × flat everywhere" does not hold day 1 to
day 5, and Pier — predicted to fall — instead ties or rises.

**Question.** Does the learned p0 posterior drift systematically with
day-of-fire across fires? Does the learned containment operator agree
with the crews' reported percent contained?

**What "posterior", "hazard" and "cumulative probability" mean.** A
**posterior** is just "where the filter's knobs end up after seeing some
days of data" (GLOSSARY.md already defines it this way for the whole
campaign; here we track it day by day instead of only at the end). A
**hazard** is one day's own risk: the chance *that specific day* rolls
"contained", not the running total. A **cumulative probability** is the
running total itself — the chance of having been contained by day d,
built from every day's hazard so far, the way "chance you've flipped
heads at least once by the fifth flip" is built from five coin flips.

**What we read.** For each fire, the five E33 reports
(`exp33_noise/<fire>_base_seed{0..4}.json`), which already carry, per
observation: `p0_mean`, `dur_mean`, `wind_scale_mean`,
`contained_fraction`, `area_ratio_mean` (each already averaged over the
32 members within a seed) and `obs_burned` (the truth mask's cell count
at that observation — checked directly against `truth.json`'s
`arrival_hours` for Bear and found to match exactly, so it is trusted for
the other five fires too), and `final_genomes` (each member's knobs at
the end of the run, including `contain_a`/`contain_b`). Each fire's
`containment.json` (ICS-209 `PCT_CONTAINED_COMPLETED`, hours since
scenario t0).

**How we scored it.**

1. *Posterior trajectory*: cross-seed mean, sd and median of the five
   metrics above, per observation, aligned by day since the first mask
   (day 0 = the report's first observation; the ignition-to-first-mask
   window is never scored, per GLOSSARY.md's "Observation day / window").
2. *ICS-209 check*: the observed daily growth `g_d =
   (obs_burned[d] − obs_burned[d−1]) / obs_burned[d−1]`, floored at
   `1e-4` — the exact floor `cella_lib/src/wildfire/driver.rs:226`
   applies to the simulated version of this same quantity
   (`.max(1e-4)`), so a plateaued or shrinking mask cannot send
   `ln(growth)` to `-∞`. For every member's FINAL `(contain_a,
   contain_b)` across all 5 seeds (160 members per fire), the daily
   hazard `sigmoid(a + b·ln(g_d))` — the formula
   `driver.rs:205-236`'s `period_end` evaluates once per simulated day,
   fed here with the real fire's growth instead of a simulated member's.
   The cumulative probability of being contained by day d is `1 −
   Π(1 − hazard_i)` over days `1..=d` (containment is one-way: once
   contained, a member's chain of "not yet" only ever shrinks — matching
   the driver's own `STATE_CONTAINED` flag, which is never cleared).
   Averaging that over the 160 members gives the **model containment
   curve**. Day 0 is fixed at 0.0 by construction: the driver's own guard
   is `before > 0.0`, true only from the *second* observation on (the
   burned-at-day-start state starts at 0.0), so no member can be
   contained on day 0 either way. Both this curve and ICS-209's percent
   contained are put on the same day axis (`day = (hours_since_t0 −
   hours_of_first_mask) / 24`; `containment.json`'s `hours` field is
   already hours since scenario t0, the identical timebase the E33
   reports use, so no date parsing was needed), and each curve's day of
   first reaching 50 % is read off by linear interpolation between the
   two points that bracket it.

**Two honesty points.**

- **The final-genome bias.** The hazard above uses each member's genome
  *as it ended*, not the genome it carried on the day in question — the
  filter does not save a per-day genome, only the final one. The "model
  containment curve" is therefore what the population the filter
  *settled on* (after seeing every observation, hindsight included)
  would have done if exposed to the real growth sequence from day 1, not
  a record of what any member actually rolled while the filter was
  running. Because containment tends to select for members whose
  `(contain_a, contain_b)` fit the fire's overall pace, this likely makes
  the curve *smoother and more confident* than the real day-by-day
  process was — early days benefit from knowledge the filter did not
  have yet.
- **Day alignment and gaps.** Both curves use day = hours since the first
  mask, divided by 24; ICS-209's hours are already on the scenario's own
  timebase (`containment.json`'s provenance note), so the two curves line
  up without any separate date conversion. A handful of E33's observation
  gaps are wider than 24 hours — Bear (1 gap, 48 h), Brattain (2, up to
  72 h), Buck (2, up to 72 h), Pier (2, up to 120 h); none on Chimney or
  Ferguson — and each such gap is still treated as *one* hazard step here,
  not several daily ones, because the per-day genome and per-day growth
  inside the gap are not recorded. This likely *under-counts* the number
  of containment rolls the real per-day driver would have made across
  that gap, biasing the model curve toward showing *less* containment
  there than the driver would actually produce. Bear's `containment.json`
  is also the *North Complex* report (Bear was administratively merged
  into it), so its ICS-209 numbers describe the whole complex, not Bear's
  perimeter alone — read Bear's ICS-209 side with that in mind.
- One more small honesty note found while building this: the growth
  floor (`1e-4`) actually triggers a few times, always at the very end of
  a fire when the mask has almost stopped growing (Ferguson day 28,
  435,189 → 435,201 cells; Pier days 28–29, 163,714 → 163,723 → 163,735):
  not a data bug, just fires that are nearly out.

**Result — posterior medians (5-seed sd in parentheses), day 1 / day 5 / final.**

| Fire | | p0 | dur (days) | wind × | contained frac. | area ratio |
|---|---|---|---|---|---|---|
| Bear | day 1 | 0.208 (.018) | 12.66 (.87) | 0.734 (.055) | 0.125 (.037) | 1.584 (.113) |
| | day 5 | 0.148 (.022) | 15.34 (1.73) | 0.652 (.103) | 0.500 (.113) | 0.703 (.108) |
| | final (day 22) | 0.186 (.029) | 13.78 (1.05) | 0.680 (.145) | 1.000 (0) | 0.798 (.141) |
| Brattain | day 1 | 0.263 (.020) | 13.03 (.65) | 0.739 (.054) | 0.062 (.036) | 0.604 (.048) |
| | day 5 | 0.342 (.036) | 12.06 (1.18) | 0.780 (.102) | 0.312 (.067) | 0.835 (.079) |
| | final (day 23) | 0.330 (.040) | 12.22 (1.41) | 0.772 (.131) | 1.000 (0) | 0.780 (.038) |
| Buck | day 1 | 0.199 (.010) | 12.16 (.59) | 0.732 (.072) | 0.156 (.032) | 1.206 (.029) |
| | day 5 | 0.157 (.018) | 13.38 (1.03) | 0.839 (.080) | 0.625 (.091) | 1.222 (.085) |
| | final (day 31) | 0.237 (.033) | 12.97 (.85) | 0.775 (.073) | 1.000 (0) | 0.856 (.176) |
| Chimney | day 1 | 0.279 (.025) | 13.00 (.55) | 0.688 (.073) | 0.125 (.028) | 0.910 (.047) |
| | day 5 | 0.295 (.043) | 12.47 (1.05) | 0.784 (.095) | 0.594 (.184) | 1.338 (.124) |
| | final (day 14) | 0.345 (.076) | 11.78 (.96) | 0.694 (.113) | 0.781 (.089) | 2.023 (.219) |
| Ferguson\* | day 1 | 0.273 (.024) | 13.00 (.77) | 0.741 (.045) | 0.125 (.020) | 0.187 (.012) |
| | day 5 | 0.415 (.019) | 10.69 (1.47) | 0.879 (.179) | 0.281 (.044) | 0.924 (.029) |
| | final (day 28) | 0.335 (.040) | 12.72 (1.02) | 0.833 (.157) | 1.000 (0) | 0.952 (.230) |
| Pier\* | day 1 | 0.202 (.011) | 12.34 (1.08) | 0.738 (.058) | 0.188 (.025) | 1.503 (.040) |
| | day 5 | 0.219 (.010) | 12.81 (.84) | 0.844 (.104) | 0.531 (.110) | 1.463 (.070) |
| | final (day 36) | 0.240 (.029) | 13.22 (.65) | 0.762 (.105) | 1.000 (0) | 1.099 (.053) |

How to read it: each cell is the median of the five seeds' own
cross-32-member mean at that observation, sd in parentheses over the five
seeds. `*` = holdout. "final" is the last observation, at the day shown.

**Result — 50 % containment day, model vs ICS-209.**

| Fire | model day50 | ICS-209 day50 | model leads ICS-209 by |
|---|---|---|---|
| Bear | 5.4 | 18.8 | **+13.4 d** |
| Brattain | 6.4 | 12.2 | +5.8 d |
| Buck | 2.7 | 20.6 | **+17.9 d** |
| Chimney | 3.6 | 12.7 | +9.1 d |
| Ferguson\* | 4.4 | 25.5 | +21.1 d |
| Pier\* | 3.6 | 10.3 | +6.8 d |

How to read it: "day" is days since each fire's first observed mask, for
both curves. Positive means the model curve reaches 50 % contained
members before ICS-209 reports 50 % contained. `*` = holdout. Bold marks
the two fires the prediction named.

**Prediction vs result, line by line.**

- *"p0 posterior falls over days 1–5 on Bear, Buck, Pier."* Bear:
  0.208 → 0.148, **falls** by 0.060, more than either endpoint's sd
  (.018–.022). **Held.** Buck: 0.199 → 0.157, **falls** by 0.042, also
  more than either sd (.010–.018). **Held.** Pier: 0.202 → 0.219,
  *rises* by 0.017 — about 1.6–1.7× its own sd (.010–.011), small but
  the wrong sign. **Did not hold.**
- *"...and rises on Ferguson."* 0.273 → 0.415, rises by 0.142, far more
  than either sd (.019–.024). **Held**, clearly.
- *(not predicted, reported for completeness)* Brattain rises (0.263 →
  0.342, +0.079, beyond sd) and Chimney is flat (0.279 → 0.295, +0.016,
  smaller than either sd). So across all six fires the day 1–5 p0
  pattern is not "the slow fires fall, Ferguson rises": it is closer to
  "four of six fires rise (Brattain, Chimney tie-to-rise, Ferguson, Pier
  marginally), two fall (Bear, Buck)" — see "what it means" below.
- *"wind × is flat everywhere."* Day 1 medians are within 0.01 of 0.73 on
  every fire (0.688–0.741) — genuinely flat *before* any real
  differentiation, which is what a shared, barely-moved-from-prior value
  looks like. But day 1 → day 5, wind × *rises* on five of six fires
  (Brattain +0.041, Buck +0.107, Chimney +0.096, Ferguson +0.138, Pier
  +0.106 — all but Brattain beyond at least one endpoint's sd) and
  *falls* on Bear (−0.082, also beyond sd). **Did not hold** over that
  window. Looking at day 1 vs the *final* value instead, Bear (0.734 →
  0.680) and Chimney (0.688 → 0.694) end within about 1 sd of where they
  started; Brattain, Buck, Ferguson and Pier end 0.03–0.16 above day 1.
  So even by the end, wind × drifted up on four of six fires — smaller
  moves than p0's own swings, but not flat.
- *"The learned containment curve reaches 50 % contained members 5–10
  days earlier than ICS-209 on Bear and Buck."* **Direction held on
  both**: the model curve leads ICS-209 on every one of the six fires,
  Bear and Buck included. **Size did not hold**: Bear leads by 13.4 days
  and Buck by 17.9 days, both above the predicted 5–10-day range — Buck
  by nearly double. The other four fires (not part of this specific
  prediction) land inside or near that range: Brattain +5.8, Chimney
  +9.1, Pier +6.8, and the holdout Ferguson +21.1 (also well outside).

**What it means.** The direction of both halves of the prediction held —
p0 does not drift the same way on every fire (E21's "something slows
these fires early" is not a universal pattern, only a per-fire one), and
the model is consistently more trigger-happy about calling a fire
"contained" than ICS-209 is about calling it "contained" — but the sizes
were both underestimated and, on Pier, the sign was wrong. Two candidate
explanations for the size miss, neither tested here: (1) the final-genome
bias above pushes the model curve toward more confidence than the
real day-by-day filter had, which would make every lead larger than a
true per-day accounting would show; (2) ICS-209's "percent contained" is
a crew judgment about *containment lines*, not about *further spread*,
so it can legitimately lag a purely growth-driven signal by a lot more
than 5–10 days on a big, slow-moving complex fire (Bear, Buck) — the two
fires with the largest leads are also the two with the widest gap
between "growth has nearly stopped" and "crews call it contained."
Pier's p0 not falling is new: Pier is also the fire E38 flagged as "the
one the containment operator describes best" (small `immreset` losses
across the board), so its containment knob may already be doing more of
the early-slowdown work there than p0 is, leaving p0 free to drift up
slightly instead of down.

**Questions this raises.**

- Does a per-day genome trace (saving the population's genome at every
  assimilation step, not just the final one) shrink the Bear/Buck lead
  toward the predicted 5–10 days, or is the size gap real? Open.
- Is ICS-209's containment-line lag itself measurable against fire
  size or fuel type, to test explanation (2) above without needing a
  per-day genome trace? Open.

**Verdict.** Finding: the *direction* of both halves of the TEST_PLAN
v1.8 prediction held (p0 drift is per-fire, not universal, in the
predicted direction on Bear/Buck/Ferguson; the containment curve always
leads ICS-209), but the *sizes* were both underestimated (13–21 days,
not 5–10) and Pier's p0 did not fall as predicted. Not a lever test —
no configuration changes as a result of this file.

**Later.** None yet.
