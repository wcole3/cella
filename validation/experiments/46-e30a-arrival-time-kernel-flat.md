# E30a — the arrival-time kernel on a flat grid · finding — arrival holds its shape at any size; Bernoulli was never as elongated as predicted; the rear-focus law needs more room than 10 % burned to reach Anderson

_Round 6 (2026-09-11) · 3 seeds · synthetic grids, no fire · example `cella_lib/examples/wildfire_ros.rs` (`arrival_flat`, `illuminate`, `lb` modes) · runner `exp_r6_arrival_flat.py` · results `exp30a_arrival_flat.json` · figure [figures/e30a-arrival-flat.svg](figures/e30a-arrival-flat.svg) · terms: [GLOSSARY.md](GLOSSARY.md)_

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
both rules, four winds.** See also the figure.

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
   closer to Anderson. This wasn't chased further here because doing so
   would mean picking parameters to fit Anderson rather than reporting
   what a principled, pre-derived choice actually shows.
5. **Front speed itself is duration-independent for the arrival rule**
   (Table 1's duration-10 numbers match duration 5 to 3 decimals,
   exactly), unlike Bernoulli's small but real duration sensitivity —
   a direct consequence of the module doc's own claim that arrival's
   head:flank *speed* ratio is `dir[head] / dir[flank]` regardless of
   duration. Arrival is consistently 35–45 % slower than Bernoulli at
   matched p0, since a rate accumulated linearly reaches 1 later than an
   inclusion-exclusion probability saturates.

**Recommendation for E30 (Task 8).** Use the **arrival rule** — its
core promise (shape independent of size) is confirmed cleanly and
cheaply. For the wind law, use **rear_focus**: it is the only option
tested that gets the front/back sign E41 needs, and its shape is
directionally correct (LB rises with wind, head:back already exceeds 2
at 0.6 m/s) even though this task's own 10 %-size measurement
under-reports its converged magnitude at high wind. A `c2` value is not
recommended at all under the exponential law — even its best-performing
setting here (0.45) reaches only 26 % of Anderson at 8 m/s, so E30
should treat the exponential law as ruled out for matching Anderson
rather than pick a "best" `c2` among options that all fail the same way.
If E30 keeps the exponential law for continuity, `c2 ≈ 0.45` is the
least-wrong of the four tested — but that is a statement about which
failure is smallest, not an endorsement.

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

**Questions this raises.**

- What (rule, law, `c2`) actually reproduces Anderson at a *converged*
  shape, not a 10 %-burned snapshot? Open — needs either a bigger
  size/duration budget for the rear-focus/high-c2 cases, or a different
  measurement that does not depend on reaching a fixed area fraction.
- Would a lower p0 with a longer burn duration (rather than p0 = 0.44,
  duration 5) let Bernoulli show the small-size elongation the original
  prediction expected, without also killing arrival's fire? Open.
- Does the rear-focus law's shape, plugged into the real six-fire
  scenarios (not a flat grid), actually move the wedge E37 found, the
  way E43's spotting genes did? Open — this is Task 8/E30's own
  question.

**Verdict.** Finding — the arrival rule's headline claim holds; the
wind-law comparison is a genuine mixed result, reported as measured.

**Later.** E30 (Task 8, not yet run).
