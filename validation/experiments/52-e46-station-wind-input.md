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
TEST_PLAN v1.9, §9 · `station_a` batch: `binary_git e55b0a6` (clean
HEAD, verified in the batch summary and all 18 raw reports), load(1 min)
5.19 at launch → 10.10 at finish, wall time 1084.1 s ≈ 18.1 min at 2
workers · `station_b`/`station_c` batches: `binary_git`/load/wall
[PENDING] (**every wall time here: shared box, 2 workers, do not
compare against each other or against E44's/E45's**) · terms:
[GLOSSARY.md](GLOSSARY.md)_

**In short.** Arm (a) is in; arms (b) and (c) are not yet. The
prediction's clause (a) — "ties E33 everywhere" — **does not hold**: only
three of six fires tie (Bear, Buck, Chimney); Brattain and Pier lose
beyond 2 sd (−5.00 sd, −3.27 sd) and Ferguson gains beyond 2 sd (+9.41
sd, the largest move of any arm in this write-up so far). Station wind
alone, with no arrival kernel and no gene, is not a no-op for the
Bernoulli config — it moves half the fires by more than the noise floor,
in both directions. The deterministic nulls (Circle, Ellipse,
persistence, both lagged variants) are unchanged from E33's own values
to three decimals, as expected (they read ERA5 directly, never the
station log, regardless of this knob), and every one of the 18 windows
across all six fires had a station sample — zero fallback windows for
arm (a). See Result 1 below for the full tables and a look at why the
"only cares about wind speed" premise undersells how much even a
Bernoulli/exponential config's *rate* (not direction) depends on which
wind speed it is handed.

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

## Result 1 — arm (a): `station_a` vs E33

Provenance: the batch summary and all 18 raw reports
(`exp46_station_wind_input_a/*.json`) carry `binary_git e55b0a6`,
matching the clean HEAD this batch was launched from. 18 jobs (3 seeds ×
6 fires), 2 workers, load(1 min) 5.19 → 10.10, wall time 1084.1 s ≈ 18.1
min (**shared box, not a claim about anything but this run** — for
order-of-magnitude context only, this arm has no arrival kernel and no
4× clock, unlike every E44/E45 batch, so a much shorter wall time than
those is expected on that basis alone, not because the box was any less
busy).

Mean one-window-ahead consensus IoU, three seeds, against E33's own
five-seed baseline (`exp33_noise.json`, `r7_common.e33_baseline()`),
generated directly by `r7_common.summary_table()`:

| Fire | E33 mean | E33 sd | station_a mean | station_a sd | Delta (sd) | verdict |
|---|---|---|---|---|---|---|
| Bear | 0.479 | 0.015 | 0.490 | 0.008 | +0.011 (+0.73 sd) | tie |
| Brattain | 0.416 | 0.004 | 0.396 | 0.010 | −0.019 (−5.00 sd) | **beyond 2 sd (loss)** |
| Buck | 0.590 | 0.039 | 0.580 | 0.003 | −0.010 (−0.26 sd) | tie |
| Chimney | 0.434 | 0.012 | 0.438 | 0.005 | +0.004 (+0.34 sd) | tie |
| Ferguson* | 0.344 | 0.007 | 0.414 | 0.005 | +0.070 (+9.41 sd) | **beyond 2 sd (gain)** |
| Pier* | 0.535 | 0.003 | 0.525 | 0.004 | −0.010 (−3.27 sd) | **beyond 2 sd (loss)** |

`*` = holdout pair. Per-seed IoU, plainly (seeds 0–2, sorted, not the
rounded means above): Bear 0.480, 0.494, 0.495; Brattain 0.390, 0.391,
0.408; Buck 0.577, 0.581, 0.582; Chimney 0.433, 0.440, 0.442; Ferguson
0.410, 0.413, 0.419; Pier 0.521, 0.525, 0.529 — every fire's three seeds
sit close together (station_a's own sd is at or below E33's five-seed sd
on four of six fires; Brattain's 0.010 and Pier's 0.004 sit slightly
above E33's 0.004 and 0.003 — the two fires that also moved beyond 2 sd,
consistent with real per-seed spread, not a single outlier), so none of
these moves is one outlier seed.

Brier, the four nulls (persistence, Circle, Ellipse, lagged), and final
contained fraction:

| Fire | Brier E33 (5-seed mean) | Brier station_a (3-seed mean) | Circle | Ellipse | Persistence | Lagged persistence | Lagged Circle | Contained, 3 seeds (min–max) | Fallback windows |
|---|---|---|---|---|---|---|---|---|---|
| Bear | 0.0510 | 0.0501 | 0.541 | 0.513 | 0.091 | 0.902 | 0.912 | 1.000–1.000 | 0 |
| Brattain | 0.1090 | 0.1145 | 0.450 | 0.469 | 0.017 | 0.889 | 0.900 | 1.000–1.000 | 0 |
| Buck | 0.0468 | 0.0473 | 0.670 | 0.701 | 0.205 | 0.957 | 0.944 | 1.000–1.000 | 0 |
| Chimney | 0.1276 | 0.1242 | 0.372 | 0.247 | 0.119 | 0.882 | 0.867 | 0.562–0.719 | 0 |
| Ferguson* | 0.1380 | 0.1327 | 0.373 | 0.503 | 0.007 | 0.919 | 0.915 | 1.000–1.000 | 0 |
| Pier* | 0.1093 | 0.1116 | 0.559 | 0.566 | 0.199 | 0.953 | 0.946 | 1.000–1.000 | 0 |

Circle, Ellipse, plain persistence and both lagged nulls (computed by
averaging each raw report's own `scores[*].radial_iou`/`ellipse_iou`/
`persistence_iou` over every scored window, and reading the report's own
`mean_lagged_*` fields) are **identical to three decimals across all
three seeds on every fire, and match E44's own null table for these same
six fires exactly** — expected, and a useful cross-check: the nulls
read the scenario's own ERA5 schedule directly, never the station log,
so `SMC_WIND_SOURCE` cannot move them. **Zero station_fallback_windows**
on every fire, every seed — every one of the 15–30 scored windows per
fire had at least one station-log row inside its bounds, so arm (a)'s
IoU table above reflects the station wind at full coverage, not a mix
with ERA5 fallbacks. Chimney is the one fire whose three seeds do not
all fully contain (56.2%, 65.6%, 71.9% — this fire's contained fraction
does not fully close under this config either, the same pattern E44's
Arm B also showed for Chimney, though with different specific seeds).

**Why arm (a) does not simply tie: station wind is not just "the same
signal at finer grain."** The prediction's reasoning — "a round Bernoulli
blob only cares about wind speed" — undersold the input change. The
Bernoulli/exponential config's spread kernel is driven by both the wind
*speed* (`wind_scale`, `model.wind_law`'s magnitude term) and, more
weakly, its *direction* (the exponential law's own, much gentler
front/back skew — see Wind law in the glossary). Computing the same
per-window vector mean this experiment's knob now feeds the driver,
directly from each fire's `scenario.json`/`station_hourly.json` (not a
report field — a post-hoc diagnostic for this write-up, same
`station_vector_mean` math, not a duplicate implementation used by the
model itself):

| Fire | Mean ERA5 speed (m/s) | Mean station speed (m/s) | Speed ratio (station/ERA5) | Mean absolute direction difference (°) |
|---|---|---|---|---|
| Bear | 0.768 | 1.154 | 1.50× | 56.0 |
| Brattain | 1.897 | 1.905 | 1.00× | 51.5 |
| Buck | 1.182 | 3.435 | 2.91× | 50.5 |
| Chimney | 1.461 | 2.605 | 1.78× | 14.8 |
| Ferguson* | 0.423 | 4.307 | 10.19× | 33.4 |
| Pier* | 0.523 | 1.037 | 1.98× | 91.3 |

("Mean absolute direction difference" is the circular difference between
the station vector mean's own "toward" bearing and the window's ERA5
"toward" bearing, averaged over every window, unrotated by any gene —
the same quantity E48's "ERA5-vs-station disagreement" glossary entry
describes, computed here as a per-fire seasonal average rather than
per-window.) No single column here cleanly separates the three fires
that moved from the three that tied: Ferguson's huge gain lines up with
by far the largest speed change (10.19×, an order of magnitude more
wind than ERA5 says), and Pier's loss lines up with both a large speed
change (1.98×) and the largest direction disagreement of any fire
(91.3°) — but Brattain lost just as sharply (beyond 2 sd) with almost no
speed change at all (1.00×) and a direction disagreement (51.5°) no
larger than Buck's or Chimney's, both of which tied. Speed alone is not
the whole story either: Buck's speed nearly triples (2.91×) and still
ties. Read plainly, not oversold: **station wind is a materially
different input from ERA5 on several of these fires, in ways a
"same shape, finer grain" mental model does not capture, and this
Bernoulli config is sensitive enough to that difference to move by more
than the noise floor on half the fires** — but no single scalar (speed
ratio, direction difference) here explains which half.

**Arm (a)'s own prediction clause.** "(a) ties E33 everywhere (a round
Bernoulli blob only cares about wind speed)." **Fails as tested** — ties
on 3 of 6 fires (Bear, Buck, Chimney), loses beyond 2 sd on 2 (Brattain,
Pier), gains beyond 2 sd on 1 (Ferguson). The parenthetical reasoning
itself is also not well supported by this arm's own data (see table and
discussion above): station wind changes the model's forecast by
swapping in a different *speed*, and possibly direction-weighting
interaction, not by "grain" alone — this is itself the honest answer to
part of this experiment's question, ahead of arms (b)/(c): even the
*input* alone, with no gene and no arrival kernel, is not neutral.

## Result 2 — arm (b): `station_b` vs Arm B · PENDING

_[Filled in after arm (b)'s batch (Phase 3).]_

## Result 3 — arm (c): `station_c` vs Arm B, and vs arm (b) directly · PENDING

_[Filled in after arm (c)'s batch (Phase 4).]_

**Station fallback windows, per fire, per arm.** Arm (a): zero fallback
windows on every fire (table in Result 1 above) — every scored window
had a station sample. Arms (b)/(c) use the same six scenario station
logs, so their fallback counts are expected to match arm (a)'s
(zero everywhere); confirmed once those batches are in (Phase 3/4) —
if any arm's count differs from arm (a)'s on the same fire, that would
itself be a bug (the station log and the scenario's window boundaries
do not depend on which arm is running) and will be reported explicitly,
not silently reconciled.

**Prediction checked clause by clause.** _[Filled in at Phase 4.]_

**What it means.** _[Filled in at Phase 4 — the verdict on "input vs
mechanism": does station wind alone (arm b) recover most of the gene's
gain, or does the gene (arm c) still add most of its value on top of a
corrected input?]_

**Questions this raises.** _[Filled in at Phase 4.]_

**Verdict.** _[Filled in at Phase 4.]_

**Later.** Not yet revisited.
