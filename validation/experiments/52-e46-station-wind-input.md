# E46 — station hourly wind as the driver input · RESULTS PENDING

_Round 7 (2026-09-25) · three arms, 3 seeds (0–2), six fires, 2 workers,
one arm per batch, run one at a time: `station_a` (E33's recommended
config + station wind, 18 runs), `station_b` (Arm B preset, gene off,
station wind, 18 runs), `station_c` (Arm B preset, gene on, station
wind, 18 runs) · new knob `SMC_WIND_SOURCE=era5|station` (default
`era5`, unchanged behaviour) · runner `exp_r7_e46.py`
(`--arm {station_a,station_b,station_c}`, restructured to one arm per
batch the same way Task 5 restructured `exp_r7_e45.py`) →
`exp46_station_wind_input_a.json`, `_b.json`, `_c.json` (+ raw reports
under each name's own directory) · arm (a) compared against E33's
five-seed mean/sd (`exp33_noise.json`, `r7_common.e33_baseline()`); arms
(b)/(c) compared against Arm B's own five-seed mean/sd from E44
(`exp44_arm_b_5seed_summary.json`'s `arm_b_sd` block,
`r7_common.arm_b_baseline()`); (c) also compared directly against (b),
using Arm B's own sd as the bar (pre-registered) · pre-registered
TEST_PLAN v1.9, §9 · `binary_git` [PENDING — filled in after each batch]
· load(1 min) and wall time [PENDING — filled in after each batch,
shared box, do not compare across batches or against E44's/E45's] ·
terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** _[Placeholder — this section is written before any batch
has run. It is filled in after arm (a) (Phase 2 of this task), again
after arm (b) (Phase 3), and finalised once arm (c) is in (Phase 4),
with the prediction below checked clause by clause and the "input vs
mechanism" verdict stated plainly.]_

**Question.** Is the coarse ERA5 daily-mean wind the real problem behind
the `wind_rot_deg` gene's gains (E44, E45, E48), or does the gene do more
than repair a bad input — i.e. would most of its gain survive even if
the driver were handed a better wind input to begin with?

**What we changed.** A new knob, `SMC_WIND_SOURCE=era5|station`
(`cella_lib/examples/wildfire_smc/knobs.rs`'s `WindSource` enum,
`SMC_WIND_SOURCE` env var). `era5` (default, unset) is byte-identical to
before this knob existed — the deterministic nulls, `SMC_DIAG=1`'s
`era5_*` diagnostic fields, and the driver's own forcing all keep
reading the scenario's own ERA5 schedule exactly as before. `station`
replaces the driver's per-window forcing (`modes/open.rs`'s assim loop,
`ens.set_forcing(...)`) with the station log's vector mean over that
same window — same `cur.hours..next.hours` bounds the loop already uses,
same daily cadence, no sub-daily driver change — computed by a new
`nulls::station_wind_schedule` (dispatched from `nulls::wind_schedule_
for`, the single point both the real run and its unit tests call). It
reuses [`station_vector_mean`](../../cella_lib/examples/wildfire_smc/nulls.rs),
the same vector-mean function the
Ellipse null's `ellipse_station` variant already calls, rather than
duplicating that math; the only new arithmetic is converting its
`(speed, "toward" radians)` return back to the scenario's own
`from_deg` weather-report bearing, by inverting `wind_toward_grid_deg`
(`toward = (from_deg + 90).rem_euclid(360)`, so `from_deg = toward -
90` reproduces the same toward angle once it is fed back through that
same function — a new `nulls::station_window_from_deg`). A window with
no station rows in `[cur.hours, next.hours)` — including when the fire
has no `station_hourly.json` at all — falls back to *that window's own
ERA5 entry unchanged*, not `station_vector_mean`'s own separate
nearest-row imputation (which exists for a different caller, the
Ellipse null, and would silently paper over a gap instead of reporting
it); the fallback is counted by a new `nulls::station_window_has_
samples` predicate. Two new report fields, added only (nothing existing
changes): `wind_source` (`"era5"`/`"station"`) and
`station_fallback_windows` (the count above, always `0` under
`wind_source == "era5"`, since the station log is never consulted at
all in that case). The deterministic nulls (persistence, Circle,
Ellipse, their lagged variants) and `SMC_DIAG=1`'s `era5_*`/`station_*`
diagnostic fields are unaffected by this knob either way — they keep
reading `sc.wind` (and, separately, the station log for `ellipse_
station`/the diag fields) directly; only the ensemble's own forcing
changes. The knob's seam is the assim loop already used for every mode
this run needs (`open`/`assim`/`evolve`-forecast share `modes::open::
run`); `map`/`evolve`'s *fit* half (`main.rs`'s own `weather_schedule`,
used only by `modes::map` and the fit half of `modes::evolve`) is
untouched — this experiment's three arms all run in `assim` mode, so
that path is never exercised here. Unit tests
(`cella_lib/examples/wildfire_smc/nulls.rs`,
`e46_station_wind_source_tests`; `knobs.rs`, `wind_source_tests`): a
synthetic station log blowing a known, constant wind reproduces that
exact vector mean in the schedule (checked both directly against
`station_vector_mean` and against the known input, a steady west wind);
`WindSource::Era5` (unset) returns `sc.wind` itself, untouched, even
when a station log is available; a window with no station rows falls
back to its own ERA5 entry and is counted, both when the log has a gap
and when there is no log at all; `SMC_WIND_SOURCE` unset/`"era5"`/an
unrecognised value all parse to `Era5` (a typo is a silent no-op, not a
panic, consistent with every other `SMC_*` knob).

**The three arms.** All `assim` mode, 32 members, seeds 0–2, six fires,
2 workers per batch, one arm per batch (never together):

1. **`station_a`** — E33's own recommended config
   (`r5_common.BASE_ENV`: Bernoulli spread, no arrival kernel) +
   `SMC_WIND_SOURCE=station`. Nothing else changed from E33's own
   five-seed run.
2. **`station_b`** — the Arm B preset (`r7_common.ARM_B`: arrival
   kernel, rear-focus wind law, 4× clock, the `arrival_x4` prior) with
   `SMC_WIND_ROT_GENE` **unset** (no learned per-member wind-direction
   offset) + `SMC_WIND_SOURCE=station`.
3. **`station_c`** — the Arm B preset unchanged (gene on, ±90°) +
   `SMC_WIND_SOURCE=station`.

**Why we expected it to matter.** Two earlier findings motivate this
experiment. E48 found that on Brattain, the three windows carrying just
over half the season's burned area are exactly the windows where ERA5
and the station log disagree most sharply on wind direction (83°–139°
apart), and that Arm B's `wind_rot_deg` gene cuts the resulting downwind
miss by 42–45% on the two biggest of those three — i.e. at least some of
the gene's gain looks like it could be *correcting a bad input* rather
than doing something ERA5 itself could never support. E41 separately
found that ERA5's wind direction is close to actively wrong (not just
coarse) on Ferguson and Chimney specifically, and that direction carries
real shape signal on those two fires. If the gene's gain is mostly input
repair, feeding the driver a better input directly (station wind, no
gene) should recover most of that gain on its own; if the gene does more
than repair the input, station wind alone should fall well short of what
the gene achieves even once the input itself is fixed.

**How we scored it.** Per fire, mean and sd of one-window-ahead
consensus IoU across each arm's three seeds (`r7_common.fire_stats`).
Arm (a) is a Bernoulli/E33-config arm, so it is compared against E33's
own five-seed mean and sd (`r7_common.e33_baseline()`; Bear 0.479/0.015,
Brattain 0.416/0.004, Buck 0.590/0.039, Chimney 0.434/0.012,
Ferguson 0.344/0.007, Pier 0.535/0.003) — the same noise floor every
non-arrival-kernel Round 7 arm has been judged against since E33 itself.
Arms (b) and (c) are arrival-kernel arms, so they are compared against
Arm B's own five-seed mean and sd from E44 instead
(`exp44_arm_b_5seed_summary.json`'s `arm_b_sd` block,
`r7_common.arm_b_baseline()`) — the noise floor every arrival-kernel arm
has been judged against since E44 produced it (E45, and now E46). Arm
(c) is additionally compared directly against arm (b) — the
pre-registered clause below — using Arm B's own sd (not a fresh sd
computed from (b) or (c) alone, both only three-seed) as the bar for
"beats by less than/beyond N sd," the same convention `r7_common.
summary_table`'s `baseline` argument already supports. The same
delta-in-sd verdict wording every Round 7 write-up uses applies
throughout: "tie" (within 1 sd), "beyond 1 sd (gain|loss)", "**beyond 2
sd (gain|loss)**" (`r7_common.verdict`). `station_fallback_windows` is
read directly off each raw report and tabulated per fire per arm
alongside the score table, not folded into the IoU numbers themselves.

**Prediction, written before the run (TEST_PLAN v1.9, §9, quoted
verbatim).** "(a) ties E33 everywhere (a round Bernoulli blob only cares
about wind speed); (b) recovers most of the gene's gain on Ferguson and
Chimney (E41: ERA5 direction is wrong there and direction carries signal
on those two); (c) beats (b) by less than 1 sd on ≥ 4 fires. If (c)
beats (b) beyond 2 sd on ≥ 3 fires, the gene does more than repair the
input." Checked clause by clause once all three arms are in (Phase 4 of
this task).

## Result 1 — arm (a): `station_a` vs E33 · PENDING

_[Filled in after arm (a)'s batch (Phase 2).]_

## Result 2 — arm (b): `station_b` vs Arm B · PENDING

_[Filled in after arm (b)'s batch (Phase 3).]_

## Result 3 — arm (c): `station_c` vs Arm B, and vs arm (b) directly · PENDING

_[Filled in after arm (c)'s batch (Phase 4).]_

**Station fallback windows, per fire, per arm.** _[Filled in once all
three batches are in — a window with no station sample falls back to
its own ERA5 entry and is counted; the same scenario station logs are
shared by all three arms, so a given fire's fallback count should not
vary by arm, and the table will say so explicitly if it does.]_

**Prediction checked clause by clause.** _[Filled in at Phase 4.]_

**What it means.** _[Filled in at Phase 4 — the verdict on "input vs
mechanism": does station wind alone (arm b) recover most of the gene's
gain, or does the gene (arm c) still add most of its value on top of a
corrected input?]_

**Questions this raises.** _[Filled in at Phase 4.]_

**Verdict.** _[Filled in at Phase 4.]_

**Later.** Not yet revisited.
