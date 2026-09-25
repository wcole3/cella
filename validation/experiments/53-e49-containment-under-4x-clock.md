# E49 — the containment operator under the 4× clock · RESULTS PENDING

_Round 7 (2026-09-25) · analysis (audit) + one launched batch · runner
`exp_r7_e49.py` (`--arm {fix,sweep}`, one arm per batch — `fix` is
built for completeness but **not launched**, see "Branch decision"
below; `sweep` is the batch this experiment runs) → PENDING · Arm B
preset (`r7_common.ARM_B`: arrival kernel, rear-focus wind law, 4×
clock, `arrival_x4` prior, ±90° wind-rotation gene) plus the new opt-in
`SMC_CONTAIN_GROWTH_FLOOR` knob, three values, four calibration fires
(Bear, Brattain, Buck, Chimney), seed 0, `assim` mode, 32 members, 2
workers, one batch, 12 runs · holdout (Ferguson, Pier) untouched by this
experiment · pre-registered TEST_PLAN v1.9, §9 · terms:
[GLOSSARY.md](GLOSSARY.md)_

**In short.** PENDING.

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

**Result 1 — sweep.** PENDING.

**What it means.** PENDING.

**Questions this raises.** PENDING.

**Verdict.** PENDING.

**Later.** PENDING.
