# E30a — the arrival-time kernel on a flat grid · finding — arrival (minimum travel time) holds its shape under the exponential law but not under rear-focus; the rear-focus law itself only matches Anderson's magnitude at low wind

_Round 6 (2026-09-11) · v2 (minimum travel time), v1 (heat accumulator, superseded) kept below for the record · 3 seeds · synthetic grids, no fire · example `cella_lib/examples/wildfire_ros.rs` (`arrival_flat`, `illuminate`, `lb` modes) · runner `exp_r6_arrival_flat.py` · results `exp30a_arrival_flat.json` · figure [figures/e30a-arrival-flat.svg](figures/e30a-arrival-flat.svg) · terms: [GLOSSARY.md](GLOSSARY.md)_

## v2 — minimum travel time (the main result)

**In short.** v1 of the arrival rule (a per-cell "heat" accumulator) had
two flaws its own tables exposed: heat only came from currently-burning
neighbours, so a cell whose neighbours all burned out before its heat
reached 1 never ignited (forcing p0 = 0.44 and, for the length-to-breadth
table, burn_duration = 500 as workarounds); and heat summed contributions
in a way that flattened direction ratios (v1's own closed-form check
measured 1.09 against a required 1.60). A controller fix round redefined
`spread: "arrival"` as **minimum travel time**, the standard fire-CA
formulation: every fuel cell keeps an *arrival time* in ticks, and a
still-unburned cell relaxes its own arrival time to the smallest
`neighbour's arrival + cost-to-cross-that-edge` over every
burning-or-already-burned neighbour, every tick. This has no death
threshold at all — a burnt-out cell is still a source forever — so the
pre-registered p0 = 0.12 now runs cleanly. The exponential wind law's own
closed-form check (jitter off, so it is a pure test of the direction
law) now lands within 13% of the required value, and arrival's
elongation-vs-size curve stays close to flat under that law at every
wind. But the picture is not uniformly good news: the rear-focus law,
even under minimum travel time, still shows a clear elongation-collapses-
with-size pattern at wind ≥ 2 m/s — the opposite of what this task's own
pre-registered prediction expected — and its length-to-breadth ratio
under-shoots Anderson (1983) by a widening margin as wind rises. Both are
reported as measured, not smoothed over.

**Question.** Does making wind set ignition *time* instead of ignition
*chance* give a kernel whose elongation does not collapse with size? And
what `c2` (or rate law) makes its length-to-breadth match Anderson 1983?

**What changed from v1.** `cella_lib/src/wildfire/mod.rs`:
`WildfireDerived::heat` → `WildfireDerived::arrival` (ticks; `0` for a
cell that starts already burning or burned, `+inf` otherwise). Each
tick, a still-unburned fuel cell with a burning-or-burned neighbour `j`
computes `arrival[cell] = min(arrival[cell], min_j(arrival[j] +
cost_j))`, `cost_j = jitter(cell) · norm_j / (p_base[cell] · dir[j] ·
slope[cell, j])` (`norm_j` = 1 cardinal, `√2` diagonal — a genuine
distance-over-speed calculation, not the same use of `1/norm` that is
already baked into `dir[j]` for the Bernoulli probability), clamped to
`>= 1` tick, and the cell ignites the first tick its own tick number
reaches that value. A neighbour only counts as a source once it shows up
as burning-or-burned in the *previous* tick's snapshot, so a same-tick
ignition can never be (mis)used as a source — checked directly by a unit
test. `burn_duration` no longer has any influence on *when* a cell
catches (only on how long it stays visibly burning, and so spot-
eligible). Nothing about `wind_law`, `arrival_jitter`, or the Bernoulli
rule changed in this fix round.

**How we measured it.** Same three synthetic-grid measurements as v1,
re-run with the pre-registered settings (no more artificial p0 = 0.44 or
burn_duration = 500 — the death threshold that required them is gone):

1. **Flat-grid front speed** (`arrival_flat` mode; 240 × 120 uniform
   fuel, a full-height burning column at x = 0..2): both rules, wind
   0/2/5/8 m/s, p0 0.12/0.22/0.44, burn duration 5/10, 3 seeds.
2. **Point-ignition elongation vs. size** (`illuminate` mode): a 3×3
   ignition at the centre of a 400×400 uniform grid, wind toward +x at
   0/2/5/8 m/s, elongation at 2/5/10/20 % burned, both rules, 3 seeds,
   p0 = 0.12, burn duration = 5 (the pre-registered trio's low end — no
   longer ruled out, since arrival cannot die). A second pass swaps in
   the rear-focus law (arrival rule only) to check the addendum's other
   clause.
3. **Length-to-breadth at 10 % size** (`lb` mode, arrival rule only):
   wind 2/5/8 m/s, `c2` ∈ {0.131, 0.2, 0.3, 0.45} under the exponential
   law plus once under rear-focus, against Anderson's `LB(U)`, p0 =
   0.12, burn duration = 5 — chosen for consistency with (2); neither
   parameter enters the arrival-time relaxation's direction *ratios* at
   all (`p_base` is common to every direction and cancels; burn duration
   never appears in the formula), so one representative value stands in
   for the full pre-registered (p0, duration) grid. Each (law, c2, wind)
   is reported twice: the default `arrival_jitter = 0.2` (3 seeds, mean)
   and a single deterministic `arrival_jitter = 0` reading, so the
   closed-form checks below can be read straight off the table.

**Result — Table 1: flat-grid front speed (cells/tick), both rules,
burn duration 5** (duration 10 is identical to 3 decimals for *both*
rules under v2 — see finding 4):

| p0 | Bern 0 | Bern 2 | Bern 5 | Bern 8 | Arr 0 | Arr 2 | Arr 5 | Arr 8 |
|---|---|---|---|---|---|---|---|---|
| 0.12 | 0.474 | 0.477 | 0.496 | 0.515 | 0.123 | 0.134 | 0.153 | 0.174 |
| 0.22 | 0.701 | 0.709 | 0.728 | 0.755 | 0.227 | 0.246 | 0.280 | 0.319 |
| 0.44 | 0.960 | 0.968 | 0.983 | 0.994 | 0.453 | 0.493 | 0.560 | 0.637 |

Arrival's own speed is now exactly proportional to p0 (0.227/0.123 =
1.85 ≈ 0.22/0.12 = 1.83; 0.453/0.123 = 3.68 ≈ 0.44/0.12 = 3.67) — a
direct, explainable consequence of `cost = jitter·norm / (p_base ·
dir · slope)` being linear in `p_base`, unlike v1's saturating,
non-proportional heat accumulation.

**Result — Table 2a: elongation vs. size (mean of 3 seeds), exponential
law, both rules, four winds.** See also the figure.

| wind | rule | 2 % | 5 % | 10 % | 20 % | range |
|---|---|---|---|---|---|---|
| 0 m/s | bernoulli | 1.157 | 1.101 | 1.061 | 1.050 | 0.107 |
| 0 m/s | arrival | 1.016 | 1.013 | 1.008 | 1.008 | 0.009 |
| 2 m/s | bernoulli | 1.105 | 1.098 | 1.092 | 1.079 | 0.026 |
| 2 m/s | arrival | 1.036 | 1.035 | 1.032 | 1.032 | 0.005 |
| 5 m/s | bernoulli | 1.214 | 1.114 | 1.147 | 1.200 | 0.100 |
| 5 m/s | arrival | 1.160 | 1.159 | 1.156 | 1.153 | 0.007 |
| 8 m/s | bernoulli | 1.080 | 1.050 | 1.048 | 1.520 | 0.473 |
| 8 m/s | arrival | 1.340 | 1.328 | 1.330 | 1.277 | 0.063 |

**Result — Table 2b: elongation vs. size (mean of 3 seeds), rear-focus
law, arrival rule only** (Bernoulli under rear-focus died before 2 %
burned on all 3 seeds at wind ≥ 2 m/s — see finding 5):

| wind | 2 % | 5 % | 10 % | 20 % | range |
|---|---|---|---|---|---|
| 0 m/s | 1.017 | 1.013 | 1.008 | 1.008 | 0.009 |
| 2 m/s | 1.849 | 1.846 | 1.845 | 1.493 | 0.356 |
| 5 m/s | 5.700 | 4.581 | 2.543 | 1.358 | 4.342 |
| 8 m/s | 10.000 | 4.903 | 2.472 | — (not reached by 20,000 ticks) | — |

**Result — Table 3: length-to-breadth at 10 % size, arrival rule, both
laws, against Anderson's `LB(U)`, default jitter (0.2, mean of 3 seeds)
and jitter 0 (deterministic).**

| wind | law | c2 | LB (jitter 0.2) | LB (jitter 0) | Anderson LB(U) | LB(0.2)/Anderson |
|---|---|---|---|---|---|---|
| 2 m/s | exponential | 0.131 | 1.032 | 1.036 | 1.505 | 0.69 |
| 2 m/s | exponential | 0.2 | 1.064 | 1.080 | 1.505 | 0.71 |
| 2 m/s | exponential | 0.3 | 1.134 | 1.165 | 1.505 | 0.75 |
| 2 m/s | exponential | 0.45 | 1.263 | 1.319 | 1.505 | 0.84 |
| 2 m/s | rear_focus | — | 1.845 | 2.000 | 1.505 | **1.23** |
| 5 m/s | exponential | 0.131 | 1.156 | 1.192 | 3.192 | 0.36 |
| 5 m/s | exponential | 0.2 | 1.308 | 1.372 | 3.192 | 0.41 |
| 5 m/s | exponential | 0.3 | 1.538 | 1.643 | 3.192 | 0.48 |
| 5 m/s | exponential | 0.45 | 1.864 | 1.901 | 3.192 | 0.58 |
| 5 m/s | rear_focus | — | 2.543 | 2.567 | 3.192 | 0.80 |
| 8 m/s | exponential | 0.131 | 1.330 | 1.398 | 7.028 | 0.19 |
| 8 m/s | exponential | 0.2 | 1.584 | 1.697 | 7.028 | 0.23 |
| 8 m/s | exponential | 0.3 | 1.905 | 1.924 | 7.028 | 0.27 |
| 8 m/s | exponential | 0.45 | 2.041 | 2.012 | 7.028 | 0.29 |
| 8 m/s | rear_focus | — | 2.471 | n/a (20,000-tick budget exhausted) | 7.028 | 0.35 |

Closed-form check at `c2 = 0.131`, `v = 8` (jitter 0, isolating the
direction law): `(head + back) / (2·flank) = cosh(c2·v)` should be
`cosh(1.048) = 1.601`; measured **1.398** (13 % short — within the unit
test's 15 % bound). Closed-form head:back ratio at 0.6 m/s (no
simulation, `dir[head] / dir[back]` from the wind law directly):
**exponential (default c2 = 0.131) = 1.17; rear_focus = 2.59**.

**Prediction check, line by line** (v2's own prediction, TEST_PLAN
v1.8): *"elongation flat with size at every wind for both laws;
jitter-0 LB equals cosh(c2·v) within 15 % under the exponential law and
is within 20 % of Anderson under rear-focus at 2/5/8 m/s; p0 0.12 fires
no longer die."*

1. *"elongation flat with size at every wind for both laws."*
   **Confirmed for the exponential law** (Table 2a: arrival's own range
   is ≤ 0.063 at every wind). **Refuted for rear-focus** (Table 2b): flat
   only at calm (no anisotropy to begin with); at 2 m/s it holds through
   10 % then drops 19 % by 20 %; at 5 m/s it falls monotonically and
   dramatically (5.70 → 1.36, a factor of 4.2); at 8 m/s it clamps at
   the metric's own maximum (10.0) at 2 % and falls to 2.47 by 10 %,
   never reaching 20 % inside the step budget. See finding 2.
2. *"jitter-0 LB equals cosh(c2·v) within 15 % under the exponential
   law."* **Confirmed at the one point the unit test checks** (c2 =
   0.131, 8 m/s: 1.398 vs 1.601, 13 % short). **Not confirmed in
   general** — Table 3's own jitter-0 column shows the gap widening
   sharply as `c2·v` grows (e.g. c2 = 0.45, v = 8: cosh = 18.3, measured
   2.01, 89 % short). See finding 3.
3. *"[jitter-0 LB] within 20 % of Anderson under rear-focus at 2/5/8
   m/s."* **Confirmed only at 5 m/s** (0.80, within bound). **Refuted at
   2 m/s** (measured *exceeds* Anderson by 33 %: jitter-0 LB 2.00 vs
   1.505) **and at 8 m/s** (n/a — did not reach 10 % burned inside the
   20,000-tick budget; the default-jitter mean it did reach, 2.47, is
   35 % of Anderson, 65 % short).
4. *"p0 0.12 fires no longer die."* **Confirmed.** Every row of Table 3
   reports 3/3 seeds reaching 10 % burned (the raw `exp30a_arrival_flat.json`
   still carries the `reached_seeds`/`total_seeds` fields from the v1
   plumbing, now always 3/3); the 60×60-grid unit test
   (`arrival_rule_reaches_the_far_edge_without_dying`) confirms this
   directly at the *default* `arrival_jitter` (0.2, not silenced),
   p0 = 0.12, burn_duration = 5 — the exact combination that died in v1.

**What it means.**

1. **The death threshold is genuinely gone**, and with it goes the
   entire class of workaround parameters (p0 = 0.44, burn_duration =
   500) v1 needed. This was the more basic of the two v1 flaws and the
   fix is unambiguous.
2. **Minimum travel time does not, by itself, guarantee a size-
   independent shape — that depends on the direction law.** Under the
   mild, smoothly-varying exponential law, arrival's elongation is
   close to flat (matching the original E30a brief's own claim). Under
   the sharply peaked rear-focus law, arrival still shows a strong
   elongation-collapses-with-size pattern, structurally the *same
   qualitative failure* E37 first found in Bernoulli, just for a
   different mechanical reason: a rear-focus point ignition starts as
   an almost one-dimensional spine (only the exact downwind cardinal
   direction is fast), which reads as extremely elongated at 2 % burned
   (up to the metric's own clamp of 10.0 at 8 m/s), and only gradually
   thickens toward a more elliptical shape as slower directions
   accumulate enough ticks to catch up. "Minimum travel time" fixes
   *Bernoulli's* saturation mechanism, but a strongly anisotropic
   direction law can still produce a shape whose *transient* is far
   more stretched than its (much rounder) longer-run character — a
   genuinely different, and unsolved, way to get "shape depends on
   size."
3. **The closed-form check is a good approximation only for mild
   anisotropy.** It was derived by treating the fire's reach in three
   cardinal directions (head, back, flank) as directly proportional to
   each direction's speed and combining them into one ratio — a fair
   approximation when the whole shape is close to an ellipse, which is
   true for small `c2·v`, but increasingly wrong as `c2·v` grows and the
   true second-moment shape (what `elongation()` actually measures)
   diverges from a clean ellipse. This is why the unit test's single
   checked point (c2 = 0.131, v = 8, product 1.05) passes comfortably
   while c2 = 0.45 at the same wind (product 3.6, "predicted" cosh =
   18.3) misses by an order of magnitude — not a discretization bug, a
   property of the closed form's own derivation.
4. **Front speed is duration-independent for arrival, exactly as
   designed** (Table 1's duration-10 column is identical to duration 5
   to 3 decimals, for *both* rules under these settings — Bernoulli's
   own duration-independence here is coincidental to this speed
   regime, not a general property the way it is for arrival by
   construction).
5. **Bernoulli can now fail outright, not just saturate, under a
   strongly directional law.** Under rear-focus at wind ≥ 2 m/s,
   Bernoulli's point ignition died before 2 % burned on 3/3 seeds at
   every wind tested (0 rows recorded past wind = 0 in Table 2b): its
   ignition window is bounded by `burn_duration` (5 ticks here), and
   rear-focus's crosswind/back probabilities are low enough that the
   whole 3×3 patch can burn out before successfully igniting any
   neighbour. Arrival never has this failure mode — a burnt-out cell
   remains a source forever, so a slow direction just takes longer,
   never "never." This is a genuine, additional point in arrival's
   favour beyond the shape claim, not one either prediction named.

**Recommendation for E30 (Task 8), from v2.** Use the **arrival rule**
— it is strictly better than Bernoulli in every measurement here (never
dies, exact size-independence under the exponential law, duration-
independent speed) — but **do not pair it with `rear_focus` and expect
size-independent shape**: that combination reproduces E37's original
failure mode (elongated-only-while-small) for a new reason. If E30's
priority is the front/back *sign* E41 found (rear-focus's head:back ≥ 2
already at 0.6 m/s, confirmed here), accept that its shape will still
depend on fire size and calibrate at (or near) the size actually being
compared against; if E30's priority is a stable shape across sizes,
stay with the exponential law but do not expect it to reach Anderson's
magnitude — even c2 = 0.45 (the most extreme value tested) reaches only
29 % of Anderson at 8 m/s. No single `c2` is recommended for matching
Anderson under the exponential law: the shortfall *grows* with wind
(0.84 → 0.58 → 0.29 at c2 = 0.45 across 2/5/8 m/s), so any one value is
only "least wrong" at whichever wind it happens to be tuned to.

**Discipline: the Bernoulli path is unchanged (this fix round too).**
`step_chunk_bernoulli` was not touched in this fix round (only
`step_chunk_arrival` and `WildfireDerived::arrival` changed). The
pre-existing wildfire snapshot/hash stress tests
(`cella_lib/tests/long_suite.rs`, `stress_2d_wildfire_t1/t4/t8`, with
and without spotting) pass unmodified against their stored hashes at 1,
4 and 8 threads after this fix round's changes.

**Questions this raises.**

- What direction law is both size-independent under arrival *and*
  reaches Anderson's magnitude? Neither law tested here is — open, and
  now the central question for Task 8/E30.
- Does rear-focus's transient (very elongated when small, rounder as it
  grows) resemble anything in the six real fires' own early growth, or
  is it purely an artifact of a single point ignition on a uniform
  grid? Open.
- Would a longer step budget (past 20,000 ticks) let rear-focus at
  8 m/s finish thickening toward a stable ratio, and would that ratio
  be closer to or further from Anderson? Open — Table 2b's own 8 m/s
  row did not reach 20 % burned inside the budget used here.

**Verdict.** Finding — arrival's death-threshold and closed-form flaws
from v1 are fixed, but the rear-focus law's own shape is not yet the
size-independent, Anderson-matching kernel E30 needs; the exponential
law is size-independent but does not reach Anderson at all. Neither
prediction clause about rear-focus (size-independence, LB within 20 %
at all three winds) was fully confirmed.

**Later.** E30 (Task 8, not yet run).

---

## v1 — heat accumulator (superseded, kept for the record)

_This section is the original E30a report, unedited except for this
heading. It describes the version of the arrival rule that shipped
first and was found, by the tables below, to have a death threshold and
a saturated head; see the controller fix-round message and the v2
section above for what replaced it. `params.spread = "arrival"` now
means the v2 (minimum-travel-time) rule; nothing here still describes
the code as it exists after this fix round._

**In short.** E37 found the fire model's large fires come out round
because its Bernoulli spread rule rolls one ignition-probability coin
per tick per burning neighbour, which saturates once enough neighbours
are burning — wind changes how *often* a cell catches, not how *long*
it takes. This task added a second rule, **arrival**: the same
per-direction wind/slope number is used as a *rate* accumulated into a
per-cell **heat** counter until it reaches 1, so direction sets ignition
*time* instead. On a flat, uniform grid (no terrain or fuel
heterogeneity, so wind is the only possible source of shape), arrival's
elongation stays flat within ±0.02 as a point-ignition fire grows from
2 % to 20 % of a 400×400 grid, at every wind tested — the prediction
holds. Bernoulli's own elongation, though, never got anywhere near the
predicted 1.5 at 2 % burned in the first place (it peaked at 1.08), so
the predicted collapse from "very elongated" to "round" could not be
observed — there was nothing to collapse from. The length-to-breadth
table shows the default wind law falls well short of Anderson (1983)'s
reference curve at every wind and every `c2` tried; a new **rear-focus**
wind law (added by controller ruling after E41) gets the *sign* right
immediately (head:back ≥ 2 at 0.6 m/s, as E41 needed) but only
approaches Anderson's magnitude at low wind — at high wind it is still
visibly widening within the 10 % burned window the table measures, so
the numbers reported here are a lower bound, not a converged shape.

**Question.** Does making wind set ignition *time* instead of ignition
*chance* give a kernel whose elongation does not collapse with size? And
what `c2` (or rate law) makes its length-to-breadth match Anderson 1983?

**What we changed.** `cella_lib/src/wildfire/mod.rs` gained:

- `params.spread: "bernoulli" | "arrival"` (default `"bernoulli"`, so
  every existing config and every existing test is byte-for-byte
  unchanged — proven below). Under `"arrival"`, each fuel cell with a
  burning neighbour adds `p_base × dir[j] × slope[cell, j] × jitter` to
  its `heat` every tick, for every burning neighbour `j`, and ignites
  once `heat >= 1`. `jitter` is a per-cell log-normal multiplier
  (`params.arrival_jitter`, default 0.2 σ), drawn once per cell for the
  whole run from a dedicated `cell_rand` stream, so runs stay
  bit-reproducible and ensembles (different seeds) still see different
  cells catch at slightly different rates.
- `params.wind_law: "exponential" | "rear_focus"` (default
  `"exponential"`, the existing kernel). `"rear_focus"` is an
  Anderson-1983 ellipse template, `dir[j] = exp(c1·v) · r(θ_j)/r_max`,
  `r(θ) = 1/(a − c·cosθ)`, `a = LB(v)` (this plan's own formula, clamped
  `[1, 8]`), `c = √(a² − 1)`, `r_max = a + c`. At `v = 0`, `a = 1`,
  `c = 0`, so it reduces to the exponential law's own no-wind case
  exactly (checked by unit test).

**Why we expected it to matter.** E37 traced Bernoulli's roundness to a
saturating *probability*: once a cell has several burning neighbours,
each direction's own odds stop mattering because *any* of them is
enough. A *rate*, by contrast, never saturates — a slow direction just
takes longer, so the head:flank *speed* ratio (`dir[head] / dir[flank]`)
should survive at any size and any burn duration. Separately, E41 (the
Ellipse null) found that the shape signal in the six fires' real (ERA5)
wind is a front/back *sign* (rear-focus LB ≈ 1.1, head:back ≈ 2.4), which
the existing exponential kernel's own head:back ratio (`exp(2·c2·v)` =
1.17 at 0.6 m/s) is far too weak to produce.

**How we measured it.** Three synthetic-grid measurements, all wind-only
(no terrain, no fuel classes beyond one, so nothing but the wind kernel
can produce shape):

1. **Flat-grid front speed** (`arrival_flat` mode; E19's own set-up: 240
   × 120 uniform fuel, a full-height burning column at x = 0..2, wind
   toward +x or calm): both spread rules, wind 0/2/5/8 m/s, p0
   0.12/0.22/0.44, burn duration 5/10, 3 seeds.
2. **Point-ignition elongation vs. size** (`illuminate` mode; E12's
   elongation, the second-moment measure): a 3×3 ignition at the centre
   of a 400×400 uniform grid, wind toward +x at 0/2/5/8 m/s, elongation
   recorded when the burned fraction (Burning + BurnedOut) first crosses
   2 %, 5 %, 10 % and 20 %, both rules, 3 seeds. p0 = 0.44, burn duration
   = 5 (see "A calibration note" below for why).
3. **Length-to-breadth at 10 % size** (`lb` mode, arrival rule only): the
   same point ignition, elongation read at the 10 % checkpoint, for wind
   2/5/8 m/s, `c2` ∈ {0.131, 0.2, 0.3, 0.45} under the exponential law,
   plus once under the rear-focus law, against Anderson's own `LB(U)`.
   Also the closed-form head:back ratio at 0.6 m/s for both laws (no
   simulation needed for that number — it is `dir[head] / dir[back]`
   from the wind law directly). p0 = 0.44, burn duration = 500 (see
   below).

**A calibration note, since it affects how to read every table below.**
Two parameters had to be chosen for measurements 2 and 3 that the task
brief left open, and both were derived from the model's own rate
formula *before* looking at any elongation or length-to-breadth result,
not fitted to make either prediction true:

- **p0 = 0.44** (the top of the pre-registered flat-grid trio). A lower
  p0 in the same trio (0.12) let the arrival rule's point ignition die
  out under low wind: a lone downwind neighbour's rate × burn duration
  fell only just above 1, and the default `arrival_jitter` (σ = 0.2) can
  push an individual cell's draw below that margin, permanently
  starving that path (heat stops accumulating once its only supporting
  neighbour has burned out). p0 = 0.44 gives a ≈ 3× margin.
- **Burn duration 500 for the `lb` mode only** (5 everywhere else). At
  `c2 ≥ 0.3` or under `rear_focus`, the crosswind direction factor is so
  small (≈ 0.015 at rear-focus/8 m/s) that at burn duration 5 a single
  upstream neighbour cannot push a crosswind cell's heat anywhere near
  1 before burning out — the fire can only ever advance as a
  one-cell-wide spine, which starves at the grid edge before 10 % of a
  400×400 grid burns (confirmed with the `WF_DEBUG=1` env var: it dies
  at the *same* tiny fraction regardless of grid size, ruling out "just
  needs more room" — a fixed-width spine's *share* of an N×N grid only
  shrinks as N grows). Burn duration 500 gives even the worst case
  (rear-focus, 8 m/s) a margin of ≈ 3 for its crosswind direction to
  self-sustain, so the fire can grow into a measurable 2-D shape at all.
  This also means the `lb` table's numbers are a snapshot of a still
  slowly widening shape, not a converged one — see finding 4.

**Result — Table 1: flat-grid front speed (cells/tick), both rules.**

Duration 5 (duration 10 is the same for both rules to 3 decimals — see
finding 1):

| p0 | Bern 0 | Bern 2 | Bern 5 | Bern 8 | Arr 0 | Arr 2 | Arr 5 | Arr 8 |
|---|---|---|---|---|---|---|---|---|
| 0.12 | 0.474 | 0.477 | 0.496 | 0.515 | 0.256 | 0.265 | 0.280 | 0.297 |
| 0.22 | 0.701 | 0.709 | 0.728 | 0.755 | 0.417 | 0.433 | 0.457 | 0.481 |
| 0.44 | 0.960 | 0.968 | 0.983 | 0.994 | 0.689 | 0.715 | 0.763 | 0.822 |

**Result — Table 2: elongation vs. burned-area size (mean of 3 seeds),
both rules, four winds.**

| wind | rule | 2 % | 5 % | 10 % | 20 % | range |
|---|---|---|---|---|---|---|
| 0 m/s | bernoulli | 1.055 | 1.023 | 1.015 | 1.009 | 0.046 |
| 0 m/s | arrival | 1.030 | 1.021 | 1.016 | 1.012 | 0.018 |
| 2 m/s | bernoulli | 1.061 | 1.048 | 1.035 | 1.027 | 0.034 |
| 2 m/s | arrival | 1.017 | 1.013 | 1.010 | 1.009 | 0.008 |
| 5 m/s | bernoulli | 1.077 | 1.065 | 1.054 | 1.051 | 0.025 |
| 5 m/s | arrival | 1.029 | 1.030 | 1.034 | 1.032 | 0.005 |
| 8 m/s | bernoulli | 1.082 | 1.059 | 1.062 | 1.064 | 0.023 |
| 8 m/s | arrival | 1.073 | 1.081 | 1.088 | 1.090 | 0.017 |

**Result — Table 3: length-to-breadth at 10 % size, arrival rule, against
Anderson's `LB(U)`.**

| wind | law | c2 | LB (mean of 3 seeds) | Anderson LB(U) | LB / Anderson |
|---|---|---|---|---|---|
| 2 m/s | exponential | 0.131 | 1.010 | 1.505 | 0.67 |
| 2 m/s | exponential | 0.2 | 1.028 | 1.505 | 0.68 |
| 2 m/s | exponential | 0.3 | 1.023 | 1.505 | 0.68 |
| 2 m/s | exponential | 0.45 | 1.060 | 1.505 | 0.70 |
| 2 m/s | rear_focus | — | 1.201 | 1.505 | **0.80** |
| 5 m/s | exponential | 0.131 | 1.034 | 3.192 | 0.32 |
| 5 m/s | exponential | 0.2 | 1.081 | 3.192 | 0.34 |
| 5 m/s | exponential | 0.3 | 1.161 | 3.192 | 0.36 |
| 5 m/s | exponential | 0.45 | 1.336 | 3.192 | 0.42 |
| 5 m/s | rear_focus | — | 2.511 | 3.192 | **0.79** |
| 8 m/s | exponential | 0.131 | 1.094 | 7.028 | 0.16 |
| 8 m/s | exponential | 0.2 | 1.223 | 7.028 | 0.17 |
| 8 m/s | exponential | 0.3 | 1.413 | 7.028 | 0.20 |
| 8 m/s | exponential | 0.45 | 1.806 | 7.028 | 0.26 |
| 8 m/s | rear_focus | — | 2.553 | 7.028 | 0.36 |

Closed-form head:back ratio at 0.6 m/s (no simulation, `dir[head] /
dir[back]` from the wind law directly): **exponential (default c2 =
0.131) = 1.17; rear_focus = 2.59**.

**Prediction check, line by line.**

From the task brief:

1. *"Bernoulli: elongation at 8 m/s falls from > 1.5 at 2 % to < 1.3 at
   20 %."* **Refuted, but not the way it sounds.** The `< 1.3 at 20 %`
   half is trivially true (1.064). The `> 1.5 at 2 %` half is false:
   measured 1.082. Bernoulli was never elongated enough at 2 % for a
   "collapse" to be visible in the first place — at p0 = 0.44 (needed to
   keep the arrival rule alive, see the calibration note) even a 2 %-
   burned Bernoulli fire already has several simultaneously-burning
   neighbours around most of its perimeter, which is exactly the
   saturation mechanism E37 named.
2. *"Arrival: elongation within ± 0.15 across sizes at every wind."*
   **Confirmed, easily.** The largest range in Table 2 is 0.046
   (Bernoulli, calm — arrival's own worst case is 0.018); at every wind
   arrival's own range is ≤ 0.017.
3. *"Default c2 gives LB ≈ 1.6 at 8 m/s (Anderson: 7.9)."* **Refuted.**
   Measured LB at c2 = 0.131, 8 m/s is 1.094, well under the predicted
   1.6 (Anderson's own curve, clamped to 8, gives 7.03 at 8 m/s here, not
   7.9 — a small difference from rounding/clamping, not a discrepancy in
   the formula).
4. *"c2 ≈ 0.3–0.45 needed to approach Anderson at 5 m/s."* **Partly
   confirmed, partly refuted.** c2 = 0.45 is the closest of the four
   tested at every wind, so *more* c2 helps in the right direction, but
   "approach" overstates it: even c2 = 0.45 only reaches 42 % of
   Anderson at 5 m/s and 26 % at 8 m/s.
5. *"No single c2 matches at all three winds because the factor is
   exponential in v."* **Confirmed.** LB/Anderson falls from 0.70 (2
   m/s) to 0.42 (5 m/s) to 0.26 (8 m/s) at the best-performing c2 = 0.45;
   no c2 in the tested range gets close at more than one wind.

From the addendum:

6. *"The rear-focus law gives head:back ≥ 2 already at 0.6 m/s."*
   **Confirmed.** 2.59.
7. *"LB within 20 % of Anderson at 2, 5 and 8 m/s at every size, under
   the arrival rule."* **Confirmed only at 2 m/s (barely); refuted at 5
   and 8 m/s.** LB/Anderson is 0.80 at 2 m/s (20 % short, right at the
   boundary), 0.79 at 5 m/s (21 % short), 0.36 at 8 m/s (64 % short).
   Finding 4 below explains why.
8. *"Under the Bernoulli rule its elongation still collapses with
   size."* **Confirmed, and arrival collapses too** — see finding 4;
   this is not the clean "arrival holds, Bernoulli doesn't" split the
   sentence implies, because the two rules were not both tested at the
   same burn duration in the pre-registered tables (Bernoulli's rows
   above are at duration 5; a supplementary check at rear-focus/8 m/s,
   duration 500 — the value needed for rear-focus to grow past a thin
   spine at all — put both rules on the same footing, and both fell).

**What it means.**

1. **Arrival's own core claim holds.** Direction setting ignition time
   rather than ignition chance really does decouple shape from size on
   a uniform grid: every arrival row in Table 2 stays inside a 0.02-wide
   band, at every wind. This is the mechanism E37 asked for.
2. **Bernoulli's predicted collapse could not be observed, because
   Bernoulli was never elongated in the first place at these
   parameters.** The brief's numeric prediction (> 1.5 at 2 %) assumed
   a small Bernoulli fire is meaningfully stretched before it rounds
   out; at p0 = 0.44 it is already close to round by 2 % burned. A lower
   p0 might show more of a shape at small sizes (E37's own most-elongated
   illumination elites used p0 0.08–0.18), but p0 that low was ruled out
   here because it kills the *arrival* rule's fire before it can be
   compared on the same footing (the calibration note above) — a real
   tension between "low enough to show shape" and "high enough to
   survive," not explored further in this task.
3. **The default wind law is far short of Anderson at every wind
   tested, confirming E30a's premise from a different angle than E41:**
   it is not just too weak to produce a front/back sign (E41's finding),
   it is also too weak to produce the *magnitude* of stretch Anderson's
   curve calls for, even scanning c2 up to 0.45.
4. **The rear-focus law gets the sign right immediately but needs more
   room than a 10 %-burned point ignition gives it to show its full
   magnitude at high wind.** The supplementary rear-focus check (wind 8,
   burn duration 500, checkpoints 2/5/10/20 %) makes this visible
   directly: elongation *starts* far above Anderson's target (8.4 at
   2 % burned — a thin, barely-widened spine) and *falls* toward it as
   the shape thickens (4.98 at 5 %, 2.55 at 10 %, 1.37 at 20 % —
   overshooting past round). The 10 %-size checkpoint this task's Table
   3 reports is a snapshot mid-transition, not a converged shape; a
   later size or a shorter, more moderate burn duration might land much
   closer to Anderson. **v2 update: this "still transitioning" pattern
   turned out to be real physics of the rear-focus law itself, not an
   artifact of v1's heat accumulator or its burn_duration = 500 — see
   Table 2b above, measured under v2 at the pre-registered burn
   duration 5.**
5. **Front speed itself is duration-independent for the arrival rule**
   (Table 1's duration-10 numbers match duration 5 to 3 decimals,
   exactly), unlike Bernoulli's small but real duration sensitivity —
   a direct consequence of the module doc's own claim that arrival's
   head:flank *speed* ratio is `dir[head] / dir[flank]` regardless of
   duration. Arrival is consistently 35–45 % slower than Bernoulli at
   matched p0, since a rate accumulated linearly reaches 1 later than an
   inclusion-exclusion probability saturates.

**Recommendation for E30 (Task 8), as of v1 — superseded by the v2
recommendation above.** Use the **arrival rule** — its core promise
(shape independent of size) is confirmed cleanly and cheaply. For the
wind law, use **rear_focus**: it is the only option tested that gets the
front/back sign E41 needs, and its shape is directionally correct (LB
rises with wind, head:back already exceeds 2 at 0.6 m/s) even though
this task's own 10 %-size measurement under-reports its converged
magnitude at high wind. A `c2` value is not recommended at all under the
exponential law — even its best-performing setting here (0.45) reaches
only 26 % of Anderson at 8 m/s, so E30 should treat the exponential law
as ruled out for matching Anderson rather than pick a "best" `c2` among
options that all fail the same way. If E30 keeps the exponential law for
continuity, `c2 ≈ 0.45` is the least-wrong of the four tested — but that
is a statement about which failure is smallest, not an endorsement.

**Discipline: the Bernoulli path is unchanged.** `step_chunk_bernoulli`
is the pre-existing bit-packed stepper moved verbatim into its own
function, byte-for-byte; the dispatcher only adds a branch on
`params.spread`. Proof, not just claim: the pre-existing wildfire
snapshot/hash stress tests in `cella_lib/tests/long_suite.rs`
(`stress_2d_wildfire_t1/t4/t8`, with and without spotting) pass
unmodified against their stored hashes at 1, 4 and 8 threads, after this
task's changes — those hashes are FNV-1a digests of a 256×256 mixed-fuel
run's final grid state, so any change to the Bernoulli arithmetic, RNG
draw order, or chunking behaviour would have broken them.

**Questions this raises (as of v1).**

- What (rule, law, `c2`) actually reproduces Anderson at a *converged*
  shape, not a 10 %-burned snapshot? **Answered in part by v2**: neither
  law converges to Anderson within the budgets tested; still open.
- Would a lower p0 with a longer burn duration (rather than p0 = 0.44,
  duration 5) let Bernoulli show the small-size elongation the original
  prediction expected, without also killing arrival's fire? **Answered
  by v2**: yes, p0 = 0.12 no longer kills arrival, but Bernoulli still
  did not show the predicted small-size elongation under the exponential
  law (Table 2a) — and died outright under rear-focus (finding 5, v2).
- Does the rear-focus law's shape, plugged into the real six-fire
  scenarios (not a flat grid), actually move the wedge E37 found, the
  way E43's spotting genes did? Open — this is Task 8/E30's own
  question.

**Verdict (v1, superseded).** Finding — the arrival rule's headline
claim holds; the wind-law comparison is a genuine mixed result, reported
as measured.

**Later.** v2, above.
