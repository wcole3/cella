# E49 — the containment operator under the 4× clock · finding: no bug, the floor is never touched, and Chimney's low fraction was one seed

_Round 7 (2026-09-25) · analysis (audit) + one launched batch · runner
`exp_r7_e49.py` (`--arm {fix,sweep}`, one arm per batch — `fix` is
built for completeness but **not launched**, see "Branch decision"
below; `sweep` is the batch this experiment runs; `diag` is one extra
diagnostic job added in Phase 2) → `exp49_containment_4x_clock_sweep.json`,
`exp49_containment_4x_clock_diag.json` · analysis
`exp_r7_e49_analysis.py` → `exp49_analysis.json` · Arm B
preset (`r7_common.ARM_B`: arrival kernel, rear-focus wind law, 4×
clock, `arrival_x4` prior, ±90° wind-rotation gene) plus the new opt-in
`SMC_CONTAIN_GROWTH_FLOOR` knob, three values, four calibration fires
(Bear, Brattain, Buck, Chimney), seed 0, `assim` mode, 32 members, 2
workers, one batch, 12 runs · holdout (Ferguson, Pier) untouched by this
experiment · pre-registered TEST_PLAN v1.9, §9 · terms:
[GLOSSARY.md](GLOSSARY.md)_

**In short.** The containment operator scales correctly with the 4×
clock (constants table below), so the pre-registered sweep branch ran:
the operator's one fixed threshold, the growth floor, at 1e-5, 1e-4 and
1e-3 on the four calibration fires, one seed. All twelve reports came
back **byte-identical** within each fire. That is not a broken knob: a
unit test shows the three values change the containment chance of a
stalled member, the report now echoes the floor the driver holds, and a
one-job diagnostic that recorded all 147 of Bear's containment draws
found the smallest daily growth any still-burning member ever had was
0.052 — about 50 times the largest floor. Members are contained while
they still grow a few percent a day, long before growth gets near the
floor, so the floor never enters the calculation. The question E49 was
built on also mostly dissolves: over E44's five seeds, Chimney's final
contained fraction under Arm B is 0.794 (sd 0.222), a tie with E33's
0.806 (sd 0.100); the 0.94 → 0.59 drop E30b saw was seed 0 alone. And
the ICS-209 lead does not shrink: Bear and Buck still lead by 13–18
days, Chimney by 8.7 (E42: 9.1).

**Question.** Chimney's contained fraction fell to 0.56–0.59 under both
E30b arms (E33: 0.94) while IoU rose (E30b pilot;
`48-e30b-uncapped-clock-direction-gene-pilot.md`) — is the containment
operator's period, growth-rate window, or per-tick rate constant failing
to scale with `SMC_STEPS_SCALE`, or is the fraction correct and the fire
genuinely grows for longer under the faster clock?

**What we read.** The containment operator itself
(`cella_lib/src/wildfire/driver.rs`, `WildfireDriver::period_end` /
`period_steps` / `apply`, ≈ lines 150–260), `SMC_STEPS_SCALE`'s own
handling (`cella_lib/examples/wildfire_smc/knobs.rs`,
`apply_steps_scale` / `steps_per_day_from`), the containment genes'
prior (`cella_lib/examples/wildfire_smc/priors.rs`,
`GENE_CONTAIN_A`/`GENE_CONTAIN_B`/`model.burn_duration`), the widened
4× prior file (`validation/scripts/experiments/priors/arrival_x4.json`),
and everywhere `modes/open.rs`/`modes/assim.rs` key off a count
(`SMC_ASSIM_EVERY`, `SMC_MAX_DAYS`, the observation-window step count).

**Constants table.** Every quantity the containment path reads, whether
it scales with `SMC_STEPS_SCALE`, and the evidence.

| Constant | Where read | Units | Scales with the clock? | Evidence |
|---|---|---|---|---|
| `steps_per_day` (containment period) | `WildfireDriver::period_steps` (driver.rs) | ticks/day | **Yes — automatically.** `steps_per_day_from` derives it from the scenario's own (scaled) `steps_per_hour`; `apply_steps_scale` multiplies `steps_per_hour` in place before anything else reads it. | knobs.rs `steps_per_day_from`/`apply_steps_scale`; test `smc_steps_scale_of_4_quadruples_the_clock_and_keeps_windows_in_sync`, assertion "the driver's containment period matches the scaled clock" |
| Growth-rate window (`before`→`burned` span between two `period_end` calls) | `WildfireDriver::period_end` (driver.rs ≈228–247) | one period = one simulated day | **Yes — by construction.** The window is defined as exactly one period, and the period itself scales (row above), so the window always spans 24h of forcing regardless of tick count. | driver.rs `period_end`; same test as above |
| `growth = (burned − before) / before` | `period_end` | dimensionless fraction | **N/A — needs no scaling.** A fractional day-over-day growth ratio, not a tick count; its value depends on how much the fire actually grew in one (correctly-scaled) day, not on how many ticks made up that day. | driver.rs:239 |
| `.max(1e-4)` growth floor | `period_end` | dimensionless fraction | **N/A — needs no scaling**, same reasoning as growth itself: a floor on a fraction, not a per-tick rate. This is the operator's one genuine fixed-value "threshold" — see "Branch decision." | driver.rs:239 (now `self.contain_growth_floor`, opt-in override added this task — unset/default is the same `1e-4` byte-for-byte) |
| Containment logit `a + b·ln(growth)` | `period_end` | dimensionless | **N/A.** `contain_a`/`contain_b` are free genes the filter fits per run against whatever growth values it sees; no fixed literal to scale. | driver.rs:240; `GENE_CONTAIN_A`/`GENE_CONTAIN_B` (driver.rs:57–59) |
| `model.burn_duration` gene range | `priors.rs::default_genes` / `arrival_x4.json` | ticks (cell residency in the "Burning" state before "BurnedOut" — `cella_lib/src/wildfire/mod.rs` doc table: `("burn_duration", Some("Fire"), Some("steps"))`) | **Yes — by construction, not automatically.** The default range is `[5, 20]` ticks (2.4–9.6h at the base 50-tick/day clock); nothing in the code multiplies it by `SMC_STEPS_SCALE`. But Arm B's own prior file, `arrival_x4.json`, already widens it to `[20, 80]` — exactly ×4 — so the *value Arm B actually uses* is correctly scaled, by the prior file's own construction, not by any automatic mechanism in the driver. A run using the default prior at `SMC_STEPS_SCALE=4` without also swapping to `arrival_x4.json` (or an equivalent) would **not** get this scaling — worth flagging for any future 4×-clock run that forgets to set `SMC_PRIOR`. | `validation/scripts/experiments/priors/arrival_x4.json` (`"model.burn_duration", "range": [20, 80]`) vs `priors.rs::default_genes`'s `[5, 20]`; `cella_lib/src/wildfire/mod.rs:2456` |
| `tau_days` decay gene | `WildfireDriver::apply` (driver.rs ≈192–214) | real days (`exp(-hours / (24·tau))`) | **Yes — automatically.** `hours` is computed from `step / steps_per_day * 24` when no forcing supplies it, and `steps_per_day` is the already-scaled value (row 1); the decay itself is a function of real hours, never of tick count. | driver.rs `apply`, the `hours` computation and `decay` formula |
| `SMC_ASSIM_EVERY` | `modes/assim.rs::maybe_assimilate` | count of observation windows (days), not steps | **Yes — by construction (already day-based).** Thinned via `obs_idx.is_multiple_of(assim_every)`, where `obs_idx` counts scored observations, never steps. | modes/assim.rs:28–30 |
| `SMC_MAX_DAYS` | `modes/open.rs` assim loop | count of scored observation windows | **Yes — by construction (already day-based).** Gated on `scores.len() >= knobs.max_days`, where `scores` grows one entry per observation, never per step. | modes/open.rs:415 |
| Observation-window step count | `modes/evolve.rs::observation_steps`, `modes/open.rs:229` | steps, derived from hours | **Yes — automatically.** Computed as `hours * sc.steps_per_hour`, the same already-scaled field row 1 uses. | modes/evolve.rs:70–93; modes/open.rs:229 |

**Audit finding: everything the containment operator itself depends on
scales correctly** — either automatically through the scenario's own
(scaled) `steps_per_hour`, or, for `model.burn_duration`, by construction
through the `arrival_x4.json` prior file Arm B already uses (its range
is widened exactly ×4, matching `SMC_STEPS_SCALE=4`). No scaling bug was
found in the operator itself. (Out of scope for this table, but noted
for context: `model.p0`, the per-tick ignition probability the *spread*
model reads — not the containment operator — is also a per-tick
quantity that does not auto-scale; `arrival_x4.json` partially
compensates by lowering its floor from 0.08 to 0.02, exactly ÷4, but
leaves the ceiling at 0.6 unchanged. This is a spread-model question,
not a containment-operator one, and is left for a future task rather
than folded into this one.)

**Branch decision.** TEST_PLAN.md v1.9's E49 entry: "If something does
not scale that should: fix it ... and re-run Chimney seeds 0–4 on Arm B
... If everything scales: instead sweep the containment threshold on
the four calibration fires only ... one seed, three values ... threshold
values are Task 7's own choice." The audit above found nothing that
fails to scale and should — **branch 3 (sweep) applies**, not branch 2
(fix). `exp_r7_e49.py --arm fix` is still built (Chimney seeds 0–4, Arm
B unchanged) for completeness and to keep the pre-registered branch
buildable/dry-runnable, but it is **not launched** — there is no bug to
fix, so re-running Arm B unchanged would just reproduce E44's own
Chimney numbers.

**Threshold choice (post-hoc, stated before launching).** The operator's
one genuine fixed-value "threshold" is the `.max(1e-4)` growth floor
(new opt-in field `WildfireDriver::contain_growth_floor`, knob
`SMC_CONTAIN_GROWTH_FLOOR`, default/unset = `1e-4`, byte-identical to
every report before this knob existed — unit-tested in
`cella_lib/src/wildfire/driver.rs`,
`contain_growth_floor_knob_moves_containment_for_a_stalled_member`, plus
the existing `WildfireDriver::default()` round-trip tests). The other
candidate raised by the task brief, `GENE_CONTAIN_A`/`GENE_CONTAIN_B`,
are free genes the SMC filter fits per member per run — there is no
single fixed value to hold constant across a sweep without changing what
"the same run" means, so they are not the sweep target. **Three values,
one order of magnitude below and above the operator's own pre-existing
default:** `1e-5`, `1e-4` (baseline, matches every existing report),
`1e-3`.

**Design.** `--arm sweep`: four calibration fires only (Bear, Brattain,
Buck, Chimney — Ferguson and Pier are holdout, untouched), seed 0, Arm B
preset with `SMC_CONTAIN_GROWTH_FLOOR` at each of the three values above,
`assim` mode, 32 members, 2 workers, one batch, 4×3 = 12 runs. Output
`exp49_containment_4x_clock_sweep.json`.

**Score family.** Contained fraction and one-window-ahead consensus IoU,
across the three threshold values (this is a calibration-fire sweep, not
a promotion test against a baseline — no delta-in-sd verdict column).

**Noise floor.** Not applicable to the sweep branch (TEST_PLAN.md v1.9:
"the sweep branch is a calibration-fire comparison, not judged against
the six-fire noise floor"). For context only, Arm B's own five-seed sd
from E44 (`r7_common.arm_b_baseline()`) is reported alongside the
single-seed sweep numbers, labelled as a caveat, not a verdict bar.

**Prediction, written before the run (TEST_PLAN.md v1.9 E49, quoted
verbatim).** "the fraction is correct, not a bug — the fire really
grows for longer under the faster clock, and the ICS-209 lead E42
measured (13–21 days) shrinks toward the real 5–10 days."

**How the ICS-209 lead clause will be checked (Phase 2, analysis only,
no new run).** E44's Chimney Arm B five-seed reports
(`validation/results/experiments/exp44_arm_b_5seed/Chimney_2016_armB_seed{0..4}.json`)
already exist on disk. The same method E42 used
(`validation/experiments/42-e42-posterior-trajectories.md`,
`exp_r6_posterior.py`: daily hazard `sigmoid(a + b·ln(g_d))` from each
member's FINAL `(contain_a, contain_b)` fed the real fire's own observed
daily growth, `.max(1e-4)` floor, cumulative probability
`1 − Π(1 − hazard_i)`, 50%-crossing day by linear interpolation, compared
against `containment.json`'s ICS-209 `PCT_CONTAINED_COMPLETED` on the
same day axis) will be re-run on these Arm B (4× clock) reports instead
of E33's (1× clock) reports, Chimney only. E42's own Chimney number
(1× clock, E33 config): model day50 3.6, ICS-209 day50 12.7, lead +9.1
days — already inside the "real" 5–10 day range the prediction names, so
this clause's test is whether Arm B's own lead is not larger than E42's,
not whether it lands in 5–10 (it may already be there). If a
re-analysis script is needed it will be written under
`validation/scripts/experiments/` and named in Phase 2, not duplicated
from `exp_r6_posterior.py`.

**Stop rule.** None (TEST_PLAN.md v1.9).

**Result 1 — sweep.** Four calibration fires × three growth floors,
seed 0, Arm B preset. "Day 50 %" is the first day (days since the first
observed mask, linear interpolation) the report's own in-run contained
fraction reaches 0.5. `a`/`b` are the medians of the 32 members' final
`contain_a`/`contain_b`. Δ is against E44's Arm B seed-0 report for the
same fire (the floor-1e-4 twin). All twelve reports carry `binary_git
b77692d` (checked).

| Fire | Floor | Mean consensus IoU | Mean Brier | Final contained | Day 50 % | median `a` | median `b` | Δ IoU vs E44 s0 | Δ Brier vs E44 s0 |
|---|---|---|---|---|---|---|---|---|---|
| Bear | 1e-5 | 0.4662 | 0.0506 | 1.000 | 3.0 | −3.758 | −1.315 | 0 | 0 |
| Bear | 1e-4 | 0.4662 | 0.0506 | 1.000 | 3.0 | −3.758 | −1.315 | 0 | 0 |
| Bear | 1e-3 | 0.4662 | 0.0506 | 1.000 | 3.0 | −3.758 | −1.315 | 0 | 0 |
| Brattain | 1e-5 | 0.4477 | 0.0949 | 1.000 | 5.4 | −3.054 | −1.106 | 0 | 0 |
| Brattain | 1e-4 | 0.4477 | 0.0949 | 1.000 | 5.4 | −3.054 | −1.106 | 0 | 0 |
| Brattain | 1e-3 | 0.4477 | 0.0949 | 1.000 | 5.4 | −3.054 | −1.106 | 0 | 0 |
| Buck | 1e-5 | 0.6472 | 0.0390 | 1.000 | 5.0 | −2.900 | −0.995 | 0 | 0 |
| Buck | 1e-4 | 0.6472 | 0.0390 | 1.000 | 5.0 | −2.900 | −0.995 | 0 | 0 |
| Buck | 1e-3 | 0.6472 | 0.0390 | 1.000 | 5.0 | −2.900 | −0.995 | 0 | 0 |
| Chimney | 1e-5 | 0.4984 | 0.1009 | 0.594 | 4.0 | −4.708 | −1.122 | 0 | 0 |
| Chimney | 1e-4 | 0.4984 | 0.1009 | 0.594 | 4.0 | −4.708 | −1.122 | 0 | 0 |
| Chimney | 1e-3 | 0.4984 | 0.1009 | 0.594 | 4.0 | −4.708 | −1.122 | 0 | 0 |

How to read it: every row within a fire is the same, to the last digit,
and every Δ is exactly zero. This is stronger than "the same to four
decimals": within each fire the three report files are **byte-identical**
(same MD5), and the floor-1e-4 file equals E44's seed-0 report field for
field once provenance (`binary_git`, `binary_built_utc`) and the fields
added after E44 ran (`wind_source`, `station_fallback_windows` from E46)
are set aside. So leaving the new knob unset reproduces E44 exactly, as
it was meant to. For context only (the noise-floor caveat in the
design): Arm B's five-seed mean consensus IoU sd from E44 is 0.005
(Bear), 0.034 (Brattain), 0.005 (Buck), 0.031 (Chimney); a Δ of exactly
zero needs no such yardstick.

Holdout fires (Ferguson, Pier): untouched. No run in this experiment
used them, and the ICS-209 re-analysis below leaves them out too.

**Result 2 — why the sweep is a null: plumbing bug or a floor nothing
reaches?** Byte-identical output fits two very different stories: (a)
the knob never reached the driver the members use, or (b) the floor
never changes a single containment draw. The evidence, in order:

- *The knob does reach the driver.* New unit test
  `sweep_floors_1e5_1e4_1e3_change_containment_unless_the_logit_is_saturated`
  (`cella_lib/src/wildfire/driver.rs`) runs the real driver inside a
  real ensemble, 200 members whose fire cannot spread (growth exactly 0
  before the floor), with `contain_a = −4`, `contain_b = −0.5`. At floors
  1e-5 / 1e-4 / 1e-3 the daily containment chance is 0.853 / 0.646 /
  0.366, and the contained fraction falls strictly as the floor rises.
  The runner builds its driver straight from `SMC_CONTAIN_GROWTH_FLOOR`
  (`modes/open.rs`), and the child process gets the variable (the
  runner's `--dry-run` prints it; `r5_common.run` passes it through).
  Each report now carries a `contain_growth_floor` field read from the
  driver itself (new field only); the diagnostic run below reports
  `0.001`, the value it was given.
- *The floor never binds.* One extra diagnostic job (`--arm diag`: Bear,
  seed 0, floor 1e-3, `SMC_DIAG=1`, one job, 1 worker, niced) recorded
  every daily containment draw: the member's burned count at the start
  and end of the day, the raw growth ratio **before** the floor, its
  `contain_a`/`contain_b`, and the outcome. Recording steps the ensemble
  exactly as before (unit test
  `recording_contain_draws_leaves_the_run_unchanged`), and this run's
  scores and final genomes match the sweep's Bear floor-1e-3 report
  exactly — so its draws *are* the sweep's draws, for all three floors.

| Bear, seed 0 (diagnostic run) | Value |
|---|---|
| Containment draws recorded | 147 (28 of them contained the member) |
| Smallest raw daily growth among still-burning members | 0.052 |
| Draws with growth exactly 0 | 0 |
| Draws with growth below 1e-3 / 1e-4 / 1e-5 | 0 / 0 / 0 |
| Median / 10th / 90th percentile raw growth | 0.282 / 0.127 / 0.665 |
| Largest change in any draw's containment chance between any two floors | 0 |
| Last draw (after which every member is contained) | day 12 (hour 312) |

  `growth.max(floor)` only changes anything when growth is below the
  floor, and on Bear no draw came within a factor of 50 of the largest
  floor. A draw's chance is therefore the same number under all three
  floors, the random numbers are the same, so every outcome is the same.
  Members get contained while they are still growing: the contained
  draws had growth between 0.054 and 0.64 a day (median 0.28), and at
  Bear's median genes a member growing 5 % a day already has about a
  55 % chance of being contained that day.

**Resolution: (b).** The floor is inert in practice — not a threshold
these fires ever touch. The knob works; there is nothing for it to act
on. One caution on scope: the draw-level evidence is Bear's only (one
cheap job, as the plan allowed); for Brattain, Buck and Chimney the only
evidence is that their three reports are byte-identical, which says no
draw *flipped*, not that none came near the floor. A floor that did bind
would usually still leave outcomes unchanged when the containment chance
is already near 1 (the unit test's second half shows this: `a = 0, b =
−2` gives 0.9999999999 / 0.99999999 / 0.999999 at the three floors),
so identity on those three fires is consistent with (b) but does not by
itself prove it.

**Result 3 — the ICS-209 lead (E42's method, re-run).** Script
`exp_r7_e49_analysis.py` imports E42's own functions from
`exp_r6_posterior.py` (not copied). Method unchanged: each member's
final `(contain_a, contain_b)` fed the real fire's observed daily
growth (floor 1e-4), cumulative containment chance averaged over the
members, and the 50 % day compared with ICS-209's 50 %-contained day on
the same axis (days since the first mask). Lead = ICS-209 day 50 −
model day 50; positive means the model says "contained" first. Check on
the method: pooling E33's 160 members reproduces E42's own numbers
exactly (Bear +13.4, Brattain +5.8, Buck +17.9, Chimney +9.1). The
scaffold named Chimney only; Bear and Buck are added because they are
where E42's 13–21-day figure came from (Ferguson's +21.1 was the top of
that range, but it is holdout and stays untouched). E44 reports carry
`binary_git b60c032`; E33's predate the provenance field.

| Fire | Reports | ICS-209 day 50 | Lead per seed 0–4 (days) | Mean (sd) | Pooled, 160 members | In-run lead, mean (sd) |
|---|---|---|---|---|---|---|
| Chimney | E33 (1× clock) | 12.7 | +9.8, +8.9, +5.9, +9.5, +9.5 | +8.7 (1.6) | +9.1 | +7.5 (2.6) |
| Chimney | E44 Arm B (4× clock) | 12.7 | +4.3, +9.5, +9.6, +4.6, +9.8 | +7.6 (2.9) | +8.7 | +9.2 (0.8) |
| Bear | E33 | 18.8 | +14.3, +12.6, +13.5, +13.7, +13.2 | +13.5 (0.6) | +13.4 | +14.6 (1.1) |
| Bear | E44 Arm B | 18.8 | +12.9, +13.0, +12.0, +12.2, +15.3 | +13.1 (1.3) | +13.0 | +15.0 (2.4) |
| Buck | E33 | 20.6 | +18.7, +17.6, +16.8, +18.0, +18.5 | +17.9 (0.7) | +17.9 | +16.6 (0.9) |
| Buck | E44 Arm B | 20.6 | +18.5, +17.9, +17.2, +17.3, +16.9 | +17.6 (0.6) | +17.4 | +16.4 (1.9) |
| Brattain | E33 | 12.2 | +5.3, +5.8, +5.5, +6.1, +6.4 | +5.8 (0.4) | +5.8 | +4.1 (1.3) |
| Brattain | E44 Arm B | 12.2 | +6.0, +6.2, +6.2, +5.2, +7.1 | +6.1 (0.7) | +6.2 | +5.8 (2.5) |

How to read it: "lead per seed" uses that seed's 32 final members only;
"pooled" is E42's own number (all five seeds' members together). sd is
the sample sd over the five seeds. "In-run lead" is a second,
non-hindsight number: ICS-209's day 50 minus the day the report's own
`contained_fraction` (what the members actually rolled during the run)
first reached 0.5. E42's hindsight curve uses genomes the filter chose
after seeing every day, so it can be earlier and smoother than what
happened in the run; the in-run number has no such bias but carries the
run's own resampling noise.

On Chimney the two measures point opposite ways and both sit inside
seed noise: the hindsight lead is 1.2 days shorter under Arm B (per-seed
mean +7.6 vs +8.7, i.e. 7.57 vs 8.72 unrounded, sds 2.9 and 1.6; pooled
+8.7 vs +9.1), while the
in-run lead is 1.7 days *longer* (+9.2 vs +7.5, sds 0.8 and 2.6). In
days since the first mask: Arm B's members actually reached 50 %
contained at day 3.5 on average (per seed 4.0, 4.7, 2.9, 2.9, 3.1) vs
E33's 5.2 (9.8, 4.2, 4.0, 5.0, 3.1). On Bear and Buck, the fires the
13–21-day figure came from, the lead moves by less than half a day
(pooled 13.4 → 13.0, 17.9 → 17.4).

**Result 4 — the fraction that started this.** E49's question came from
E30b's single seed: Chimney's final contained fraction 0.94 under E33
vs 0.56–0.59 under E30b's two arms. E44 has since run Arm B for five
seeds, and E33 has five seeds too:

| Chimney final contained fraction | seed 0 | 1 | 2 | 3 | 4 | Mean (sd) |
|---|---|---|---|---|---|---|
| E33 (1× clock) | 0.938 | 0.781 | 0.688 | 0.750 | 0.875 | 0.806 (0.100) |
| E44 Arm B (4× clock) | 0.594 | 0.844 | 1.000 | 0.531 | 1.000 | 0.794 (0.222) |

The means differ by 0.0125, far inside either sd. Seed 0 happens to be
E33's highest value and one of Arm B's two lowest; the "drop" was that
pairing. Arm B's spread is wider: two seeds (0 and 3) reach about 0.8
contained around days 6–7, then fall to about 0.2 by days 9–11 and climb
back to 0.59 / 0.53, while seeds 2 and 4 end fully contained.

**Prediction vs result, clause by clause.** (The prediction, quoted
verbatim above: "the fraction is correct, not a bug — the fire really
grows for longer under the faster clock, and the ICS-209 lead E42
measured (13–21 days) shrinks toward the real 5–10 days.")

- *"the fraction is correct, not a bug"* — **Held.** The audit found
  nothing in the operator that fails to scale with the clock, and the
  sweep and diagnostic found nothing wrong with the floor. More than
  that: over five seeds there is no low fraction left to explain
  (Result 4).
- *"the fire really grows for longer under the faster clock"* — **Did
  not hold.** On Chimney, Arm B's members reach 50 % contained earlier
  in the run, not later (day 3.5 vs 5.2 on average), and end at the same
  fraction (0.794 vs 0.806). Nothing here shows the simulated fire
  growing for longer under the 4× clock. (The hindsight curve's 50 % day
  is a little later under Arm B, 4.0 vs 3.6 pooled, but that 0.4-day
  move is inside seed noise and the in-run numbers go the other way.)
- *"the ICS-209 lead E42 measured (13–21 days) shrinks toward the real
  5–10 days"* — **Did not hold.** On the fires that had 13–21-day leads,
  the lead barely moves: Bear +13.4 → +13.0, Buck +17.9 → +17.4
  (pooled; per-seed means +13.5 → +13.1 and +17.9 → +17.6, both inside
  seed noise). Chimney was already inside 5–10 under E33 (+9.1) and
  stays there (+8.7); its per-seed mean moves 1.2 days shorter, inside
  seed noise, and its in-run lead moves 1.7 days longer.

**What it means.** The containment operator is not why Arm B behaves
differently: it scales with the clock, its one fixed threshold sits
more than an order of magnitude below any growth the members show, and
the fraction it produces on Chimney is the same, on average, as under
E33. The operator is governed entirely by the learned genes
`contain_a`/`contain_b` acting on growth rates of 5 % a day and up
(median 28 % on Bear's draws);
the floor only exists to stop `ln 0`, and on these runs it never had to.
The lead on ICS-209 (13–18 days on Bear and Buck) is a property of the
learned genes and of what "contained" means in the model, not of the
clock — changing the clock by 4× left it within half a day. The one real
difference Arm B shows on Chimney is a wider spread between seeds: two
seeds lose most of their contained members mid-run, as the filter
favours the members still burning while the real fire keeps growing.

**Questions this raises.**

- Why do Arm B's Chimney seeds 0 and 3 swing from about 0.8 contained
  down to about 0.2 and back? The filter is choosing uncontained members
  around days 7–11; is that the real fire's late run (so the model's
  containment is premature), or resampling noise on a 32-member
  ensemble?
- The model "contains" members that still grow 5–64 % a day (median
  28 % on Bear), 13–18 days
  before ICS-209 reports 50 % on Bear and Buck. Model containment and
  crews' percent contained measure different things (a member stopping
  vs a line around the perimeter); is a lead of this size a model error
  at all, or the expected gap between the two definitions?
- Is the floor inert on every fire, or just on Bear? A per-fire draw
  diagnostic (one job each) would settle it for Brattain, Buck and
  Chimney, which here have only the byte-identity evidence.

**Verdict.** No bug, and no threshold effect. The containment operator
is correct under the 4× clock; the growth floor is never reached, so
sweeping it changes nothing (byte-identical on all four calibration
fires; draw-level evidence on Bear). Chimney's low contained fraction
was a seed-0 coincidence that five seeds erase. Of the prediction's
three clauses, the first held and the other two did not. Single-seed
sweep: the sweep itself is one seed per fire, which is enough here only
because the answer is exact identity, not a small difference.

**Later.**

- If anything in the containment model is revisited, the lever is the
  learned genes (`contain_a`/`contain_b`) and what "contained" is scored
  against, not the growth floor. The floor knob stays opt-in and can be
  left alone.
- Carry forward: runs at a changed clock must also switch `SMC_PRIOR`
  (the `model.burn_duration` footgun in the constants table).
- The draw diagnostic (`SMC_DIAG=1`, `diag.contain_draws`) is available
  for the per-fire check in "Questions" if it is ever needed.

**Provenance.** Sweep: 12 jobs, `binary_git b77692d` on every report,
2 workers, `nice -n 10`, load (1 min) 5.34 at launch → 5.63 at finish,
wall time 7211.7 s. Diagnostic: 1 job, `binary_git 9d2032c` (the
diagnostic's own commit; its scores match the b77692d sweep report
exactly), 1 worker, niced, load 3.84 → 6.41, 639.0 s. Shared box: wall
times are provenance only, do not compare them. ICS-209 re-analysis:
analysis only, no runs. Means are computed from the full-precision
JSON; displayed values are rounded.
