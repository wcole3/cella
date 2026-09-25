# E46 — station hourly wind as the driver input · fire-specific: input on Ferguson, mechanism on Chimney

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
workers · `station_b` batch: `binary_git 8407748` (clean HEAD, verified
in the batch summary and all 18 raw reports), load(1 min) 5.09 at
launch → 7.61 at finish, wall time 14389.1 s ≈ 4.00 h at 2 workers ·
`station_c` batch: `binary_git 8d8fbb3` (clean HEAD, verified in the
batch summary and all 18 raw reports), load(1 min) 4.80 at launch →
7.94 at finish, wall time 14729.9 s ≈ 4.09 h at 2 workers (**every wall
time here: shared box, 2 workers, do not compare against each other or
against E44's/E45's**) · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** All three arms are in; none of the four pre-registered
clauses holds as written. (a) "ties E33 everywhere" **fails**: only
three of six fires tie (Bear, Buck, Chimney); Brattain and Pier lose
beyond 2 sd and Ferguson gains beyond 2 sd (+9.41 sd) — station wind
alone, with no arrival kernel and no gene, is not a no-op for the
Bernoulli config. (b) "recovers most of the gene's gain on Ferguson and
Chimney" **splits**: on Ferguson it holds and overshoots — station wind
alone recovers *more than the entire* gene's measured gain, by two
operational definitions (+214%/+404%) — while on Chimney it fails
outright, moving the score in the *opposite* direction from E33
(−0.077). (c) "beats (b) by less than 1 sd on ≥ 4 fires" **fails** (only
2 of 6 qualify, under either reading), and its own trigger clause — "if
(c) beats (b) beyond 2 sd on ≥ 3 fires, the gene does more than repair
the input" — **falls one fire short** (2 of 6: Buck, Chimney) of the
evidentiary bar it set for that strong claim, even though (c) ties or
beats (b) on 5 of 6 fires overall. The clean story is fire-specific:
**Ferguson is an input problem** (station wind alone already gets the
best score in the whole experiment; the gene, added on top, costs a
little) and **Chimney is a mechanism problem** (station wind alone
actively breaks Chimney relative to E33; only adding the gene back
recovers it, a +5.25 sd swing from (b) to (c) — the largest of any (c)
vs (b) delta, and at +0.162 raw IoU points the largest absolute score
movement anywhere in this write-up). A post-hoc controller check
(labelled below, not re-run) found that the 10× Ferguson speed swing is
partly a structural
averaging artefact — ERA5's domain+time vector-mean wind "averages
toward calm" in a way a single station point's own vector mean does
not, true on five of six fires to some degree — so Ferguson's story may
conflate direction repair with a speed-scale difference the knob as
built cannot separate; named as the next experiment. Zero
`station_fallback_windows` on every fire, every seed, all three arms.
See Results 1–3, the three-arm summary table, and "What it means" below
for the full picture.

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

## Result 2 — arm (b): `station_b` vs Arm B

Provenance: the batch summary and all 18 raw reports
(`exp46_station_wind_input_b/*.json`) carry `binary_git 8407748`,
matching the clean HEAD this batch was launched from. 18 jobs (3 seeds ×
6 fires), 2 workers, load(1 min) 5.09 → 7.61, wall time 14389.1 s ≈ 4.00
h (**shared box, not a claim about anything but this run**).

Mean one-window-ahead consensus IoU, three seeds, against Arm B's own
five-seed baseline from E44 (`exp44_arm_b_5seed_summary.json`'s
`arm_b_sd` block, `r7_common.arm_b_baseline()`), generated directly by
`r7_common.summary_table()`:

| Fire | Arm B mean | Arm B sd | station_b mean | station_b sd | Delta (sd) | verdict |
|---|---|---|---|---|---|---|
| Bear | 0.473 | 0.005 | 0.468 | 0.012 | −0.004 (−0.84 sd) | tie |
| Brattain | 0.437 | 0.034 | 0.349 | 0.001 | −0.088 (−2.59 sd) | **beyond 2 sd (loss)** |
| Buck | 0.640 | 0.005 | 0.599 | 0.008 | −0.041 (−7.64 sd) | **beyond 2 sd (loss)** |
| Chimney | 0.489 | 0.031 | 0.358 | 0.020 | −0.132 (−4.28 sd) | **beyond 2 sd (loss)** |
| Ferguson* | 0.386 | 0.014 | 0.434 | 0.026 | +0.048 (+3.56 sd) | **beyond 2 sd (gain)** |
| Pier* | 0.521 | 0.023 | 0.488 | 0.015 | −0.033 (−1.45 sd) | beyond 1 sd (loss) |

`*` = holdout pair. Per-seed IoU, plainly (seeds 0–2, sorted): Bear
0.457, 0.467, 0.481; Brattain 0.348, 0.350, 0.350; Buck 0.591, 0.597,
0.607; Chimney 0.336, 0.364, 0.374; Ferguson 0.405, 0.448, 0.451; Pier
0.475, 0.483, 0.505 — Brattain's three seeds are unusually tight (sd
0.001), the rest show ordinary spread.

Brier, the four nulls, and final contained fraction:

| Fire | Brier Arm B (5-seed mean) | Brier station_b (3-seed mean) | Circle | Ellipse | Persistence | Lagged persistence | Lagged Circle | Contained, 3 seeds (min–max) | Fallback windows |
|---|---|---|---|---|---|---|---|---|---|
| Bear | 0.0507 | 0.0522 | 0.541 | 0.513 | 0.091 | 0.902 | 0.912 | 1.000–1.000 | 0 |
| Brattain | 0.1020 | 0.1296 | 0.450 | 0.469 | 0.017 | 0.889 | 0.900 | 1.000–1.000 | 0 |
| Buck | 0.0396 | 0.0439 | 0.670 | 0.701 | 0.205 | 0.957 | 0.944 | 1.000–1.000 | 0 |
| Chimney | 0.0947 | 0.1264 | 0.372 | 0.247 | 0.119 | 0.882 | 0.867 | 0.469–0.812 | 0 |
| Ferguson* | 0.1356 | 0.1171 | 0.373 | 0.503 | 0.007 | 0.919 | 0.915 | 0.906–1.000 | 0 |
| Pier* | 0.1074 | 0.1232 | 0.559 | 0.566 | 0.199 | 0.953 | 0.946 | 1.000–1.000 | 0 |

Nulls again match arm (a)'s and E44's own tables exactly, to three
decimals (unaffected by `SMC_WIND_SOURCE`, as designed). **Zero
`station_fallback_windows`** on every fire, every seed — the same six
station logs, the same window boundaries as arm (a); no discrepancy to
report. Brier improves on Ferguson (0.117 vs Arm B's 0.136) and is
worse everywhere else, most sharply on Chimney (0.126 vs 0.095) —
tracking the IoU table above, not surprising given Brier and consensus
IoU are different scores of the same underlying probability field.
Containment: Chimney (46.9%/68.8%/81.2%, none of the three seeds fully
closes — a wider spread than arm (a)'s 56.2–71.9% on the same fire) and
Ferguson (one seed at 90.6%, the other two full) both leave members
uncontained by the run's end, the same two fires E44's own Arm B table
flagged for partial containment.

Learned gene medians (three-seed median of each seed's own final-
ensemble median; no `wind_rot_deg` row — the gene is unset in this arm):

| Fire | `model.p0` med. (range) | `model.burn_duration` med. (range, hours) | `wind_scale` med. (range) |
|---|---|---|---|
| Bear | 0.078 (0.053–0.137) | 64.5 (44.0–68.0) | 0.548 (0.524–0.839) |
| Brattain | 0.132 (0.117–0.213) | 46.0 (44.0–61.5) | 0.720 (0.659–0.845) |
| Buck | 0.072 (0.033–0.269) | 41.0 (38.0–51.5) | 0.671 (0.586–0.906) |
| Chimney | 0.305 (0.254–0.494) | 45.5 (36.5–52.5) | 0.581 (0.361–0.583) |
| Ferguson* | 0.386 (0.256–0.400) | 62.5 (52.5–64.5) | 0.939 (0.860–1.250) |
| Pier* | 0.100 (0.072–0.164) | 47.5 (47.5–57.5) | 0.971 (0.658–1.056) |

**"Recovers most of the gene's gain" — operational definition (post-hoc
note, TEST_PLAN v1.9 does not define this quantitatively).** The
pre-registered prediction names "the gene's gain" without a number.
Two readings are both defensible from data already on disk, and are
reported side by side rather than picking one silently:

- **Definition A (primary — multi-seed, but conflates the arrival
  kernel with the gene):** the gene's gain = Arm B's own five-seed mean
  (E44, ERA5, gene **on**) minus E33's five-seed mean (Bernoulli, no
  arrival kernel) — the total measured improvement the Arm B bundle
  showed over the pre-arrival-kernel baseline, all attributed here to
  "the gene" only in the loose sense the prediction's own phrasing
  uses it (Arm B's headline addition over the already-separately-tested
  arrival kernel, E30/E30a). "Recovered fraction" = (station_b's own
  mean − E33's mean) ÷ (Arm B's mean − E33's mean).
- **Definition B (secondary — isolates the gene proper, but single-seed
  and older): the gene's gain = Arm B (E30b pilot, seed 0, ERA5, gene
  on, 0.398 Ferguson / 0.498 Chimney) minus Arm A (same pilot, seed 0,
  ERA5, gene **off**, arrival kernel otherwise identical — the same
  toggle arm (b) makes here, just under ERA5 instead of station wind:
  0.386 Ferguson / 0.382 Chimney,
  `48-e30b-uncapped-clock-direction-gene-pilot.md`'s own Result table).
  "Recovered fraction" = (station_b's own mean − Arm A's seed-0 IoU) ÷
  (Arm B pilot's seed-0 IoU − Arm A's seed-0 IoU).

| Fire | Def. A: gene's gain | station_b's gain over E33 | Def. A recovered | Def. B: gene's gain (pilot) | station_b vs Arm A (pilot) | Def. B recovered |
|---|---|---|---|---|---|---|
| Ferguson* | +0.042 | +0.091 | **+214%** | +0.012 | +0.048 | **+404%** |
| Chimney | +0.055 | −0.077 | **−139%** | +0.116 | −0.024 | **−21%** |

Both definitions agree on the shape of the answer even though they
disagree on scale: **on Ferguson, station wind alone recovers not just
"most" but *more than all* of the gene's measured gain, by either
definition — it overshoots.** On Chimney, station wind alone does not
recover any of the gene's gain by either definition — it moves in the
**opposite direction**, below both E33 and Arm A. Read together with
arm (a)'s own diagnostic (Ferguson's station wind speed is 10.19× its
ERA5 mean — an order-of-magnitude input error, not a fine-grained one),
the likeliest reading is not "the gene and station wind do the same
job on Ferguson" but that Ferguson's ERA5 input was so far off in
*magnitude* that almost any correction — with or without the arrival
kernel, with or without the gene, as arm (a) already showed — helps
enormously, while Chimney's problem is something the learned
per-member *direction* correction supplies that the station log's own
(differently wrong, per E48's own per-window finding on Brattain, and
plausibly here too) direction does not replicate.

**Arm (b)'s own prediction clause.** "(b) recovers most of the gene's
gain on Ferguson and Chimney." **Holds for Ferguson, fails for
Chimney** — a fire-specific split, not a clean "input repair" story for
this clause as a whole. Ferguson's result is consistent with the
prediction's letter (station wind does recover the gene's gain there,
and then some) but arguably not its intended spirit (the mechanism
looks like "fixing an order-of-magnitude speed error," not "fixing the
direction the gene was correcting for"). Chimney's result directly
contradicts the clause: the gene's own gain on Chimney is not
recoverable from station wind alone — station wind alone is *worse*
than not touching the wind input at all (E33) and worse than the same
arrival-kernel config with the gene off under ERA5 (Arm A, pilot).

## Result 3 — arm (c): `station_c` vs Arm B, and vs arm (b) directly

Provenance: the batch summary and all 18 raw reports
(`exp46_station_wind_input_c/*.json`) carry `binary_git 8d8fbb3`,
matching the clean HEAD this batch was launched from. 18 jobs (3 seeds ×
6 fires), 2 workers, load(1 min) 4.80 → 7.94, wall time 14729.9 s ≈ 4.09
h (**shared box, not a claim about anything but this run**).

Mean one-window-ahead consensus IoU, three seeds, against Arm B's own
five-seed baseline from E44, and against arm (b) directly (Arm B's own
sd as the bar — the pre-registered clause), both generated directly by
`r7_common.summary_table()`:

| Fire | Arm B mean | Arm B sd | station_c mean | station_c sd | Δ vs Arm B (sd) | verdict vs Arm B | station_b mean | Δ (c) vs (b) (sd) | verdict (c) vs (b) |
|---|---|---|---|---|---|---|---|---|---|
| Bear | 0.473 | 0.005 | 0.477 | 0.010 | +0.004 (+0.77 sd) | tie | 0.468 | +0.009 (+1.61 sd) | beyond 1 sd (gain) |
| Brattain | 0.437 | 0.034 | 0.363 | 0.001 | −0.074 (−2.19 sd) | **beyond 2 sd (loss)** | 0.349 | +0.014 (+0.41 sd) | tie |
| Buck | 0.640 | 0.005 | 0.618 | 0.013 | −0.022 (−4.12 sd) | **beyond 2 sd (loss)** | 0.599 | +0.019 (+3.52 sd) | **beyond 2 sd (gain)** |
| Chimney | 0.489 | 0.031 | 0.519 | 0.016 | +0.030 (+0.97 sd) | tie | 0.358 | +0.162 (+5.25 sd) | **beyond 2 sd (gain)** |
| Ferguson* | 0.386 | 0.014 | 0.413 | 0.033 | +0.027 (+1.98 sd) | beyond 1 sd (gain) | 0.434 | −0.022 (−1.59 sd) | beyond 1 sd (loss) |
| Pier* | 0.521 | 0.023 | 0.500 | 0.018 | −0.021 (−0.91 sd) | tie | 0.488 | +0.012 (+0.53 sd) | tie |

`*` = holdout pair; the "Δ (c) vs (b)" column uses Arm B's own sd as the
bar, per the pre-registered clause, not a fresh sd computed from either
three-seed arm. Per-seed IoU, plainly (seeds 0–2, sorted): Bear 0.466,
0.481, 0.484; Brattain 0.362, 0.363, 0.364; Buck 0.609, 0.610, 0.633;
Chimney 0.501, 0.528, 0.529; Ferguson 0.376, 0.424, 0.439; Pier 0.480,
0.507, 0.513.

Brier, the four nulls, and final contained fraction:

| Fire | Brier Arm B (5-seed mean) | Brier station_c (3-seed mean) | Circle | Ellipse | Persistence | Lagged persistence | Lagged Circle | Contained, 3 seeds (min–max) | Fallback windows |
|---|---|---|---|---|---|---|---|---|---|
| Bear | 0.0507 | 0.0511 | 0.541 | 0.513 | 0.091 | 0.902 | 0.912 | 0.969–1.000 | 0 |
| Brattain | 0.1020 | 0.1095 | 0.450 | 0.469 | 0.017 | 0.889 | 0.900 | 1.000–1.000 | 0 |
| Buck | 0.0396 | 0.0406 | 0.670 | 0.701 | 0.205 | 0.957 | 0.944 | 1.000–1.000 | 0 |
| Chimney | 0.0947 | 0.0799 | 0.372 | 0.247 | 0.119 | 0.882 | 0.867 | 0.781–0.938 | 0 |
| Ferguson* | 0.1356 | 0.1175 | 0.373 | 0.503 | 0.007 | 0.919 | 0.915 | 1.000–1.000 | 0 |
| Pier* | 0.1074 | 0.1118 | 0.559 | 0.566 | 0.199 | 0.953 | 0.946 | 1.000–1.000 | 0 |

Nulls match arms (a)/(b)'s and E44's own tables exactly, to three
decimals, as expected. **Zero `station_fallback_windows`** on every
fire, every seed — matching arms (a) and (b) exactly, fire for fire, on
all three arms now in. Brier improves sharply on Chimney (0.080 vs Arm
B's 0.095) and Ferguson (0.118 vs 0.136), tracking both fires' IoU
gains; worse on Bear, Brattain, Buck, Pier, all modestly. Containment:
Chimney closes markedly better under the gene (78.1%/93.8%/93.8%) than
under arm (b)'s gene-off station run (46.9%/68.8%/81.2%) — consistent
with Chimney's large IoU recovery — while Bear now shows one seed short
of full containment (96.9%) for the first time in this experiment
(arms a and b both fully closed Bear on all three seeds).

Learned gene medians, including `wind_rot_deg` (the gene this arm turns
back on):

| Fire | `model.p0` med. (range) | `model.burn_duration` med. (range, h) | `wind_scale` med. (range) | `wind_rot_deg` med. (range, °) |
|---|---|---|---|---|
| Bear | 0.082 (0.061–0.108) | 50.0 (41.5–52.5) | 0.664 (0.400–0.773) | +6.8 (−11.5–+16.8) |
| Brattain | 0.082 (0.054–0.185) | 59.5 (44.5–65.5) | 0.925 (0.735–0.967) | +16.7 (+7.8–+45.2) |
| Buck | 0.082 (0.050–0.130) | 52.0 (45.5–59.5) | 0.837 (0.771–1.002) | +14.6 (−27.9–+30.3) |
| Chimney | 0.316 (0.259–0.342) | 55.5 (36.0–57.5) | 0.761 (0.571–0.848) | −38.7 (−42.2–−18.0) |
| Ferguson* | 0.338 (0.089–0.351) | 49.0 (46.0–53.0) | 0.748 (0.746–1.040) | −7.9 (−16.4–−6.0) |
| Pier* | 0.107 (0.061–0.163) | 52.5 (51.0–59.5) | 0.860 (0.857–0.919) | +18.9 (+11.8–+30.8) |

Chimney's learned rotation is notably **larger in magnitude under
station wind (−38.7°) than E44's own Arm B learned under ERA5 (−29.3°,
five-seed median)** — if station wind already supplied a materially
better direction, a smaller correction, not a larger one, would be the
naive expectation. This is the first hint (developed further in "What
it means," below) that Chimney's problem is not simply "ERA5's
direction is wrong and station's is right" — the gene is still doing
substantial, and apparently *more* work, on top of the corrected input.

**Arm (c)'s own prediction clauses.** "(c) beats (b) by less than 1 sd
on ≥ 4 fires. If (c) beats (b) beyond 2 sd on ≥ 3 fires, the gene does
more than repair the input." Reading the "Δ (c) vs (b)" column above:
Bear +1.61 sd (beyond 1 sd gain — not "less than 1 sd"), Brattain +0.41
sd (tie — satisfies "less than 1 sd"), Buck +3.52 sd (**beyond 2 sd
gain**), Chimney +5.25 sd (**beyond 2 sd gain**), Ferguson −1.59 sd
(beyond 1 sd **loss** — not "beats" (b) at all), Pier +0.53 sd (tie —
satisfies "less than 1 sd").

- **"(c) beats (b) by less than 1 sd on ≥ 4 fires": fails, under either
  reading.** Strictly ("a small positive win, 0 < Δ < 1 sd"): only
  Brattain and Pier qualify — 2 of 6. Loosely ("within 1 sd of (b) in
  either direction," i.e. a tie): also only Brattain and Pier — 2 of 6.
  Either way, well short of the ≥ 4 bar.
- **"If (c) beats (b) beyond 2 sd on ≥ 3 fires, the gene does more than
  repair the input": the antecedent is not met.** Only Buck and
  Chimney beat (b) beyond 2 sd — 2 of 6, one short of the ≥ 3 bar. This
  strong conclusion cannot be drawn at the pre-registered evidentiary
  threshold.

**Neither pre-registered branch describes what happened.** The actual
pattern is a third outcome the prediction's binary framing did not
anticipate: (c) ties or beats (b) on five of six fires (two of those
beyond 2 sd, two more beyond/at 1 sd, one a tie) and **loses** to (b) on
exactly one (Ferguson, beyond 1 sd) — not "uniformly small wins
everywhere" and not "large wins on ≥ 3 fires," but a fire-specific mix
where the gene helps substantially on some fires, helps a little or not
at all on most others, and actively *costs* a little on the one fire
whose problem (per Result 1/2) was an input-magnitude error the gene
was never meant to fix.

**Station fallback windows, per fire, per arm.** **Zero on every fire,
every seed, all three arms.** The same six station logs, the same
window boundaries, independent of which arm is running — confirmed
identical across (a), (b) and (c), fire for fire, with no discrepancy.
Every scored window in this experiment (15–30 per fire) had at least
one station-log row in range; none of the results above reflect a mix
with ERA5 fallback data.

## Three-arm summary (vs E33 and Arm B)

| Fire | E33 mean (sd) | Arm B mean (sd) | (a) mean (sd) | (a) vs E33 | (b) mean (sd) | (b) vs Arm B | (c) mean (sd) | (c) vs Arm B | (c) vs (b) |
|---|---|---|---|---|---|---|---|---|---|
| Bear | 0.479 (0.015) | 0.473 (0.005) | 0.490 (0.008) | tie | 0.468 (0.012) | tie | 0.477 (0.010) | tie | beyond 1 sd (gain) |
| Brattain | 0.416 (0.004) | 0.437 (0.034) | 0.396 (0.010) | **beyond 2 sd (loss)** | 0.349 (0.001) | **beyond 2 sd (loss)** | 0.363 (0.001) | **beyond 2 sd (loss)** | tie |
| Buck | 0.590 (0.039) | 0.640 (0.005) | 0.580 (0.003) | tie | 0.599 (0.008) | **beyond 2 sd (loss)** | 0.618 (0.013) | **beyond 2 sd (loss)** | **beyond 2 sd (gain)** |
| Chimney | 0.434 (0.012) | 0.489 (0.031) | 0.438 (0.005) | tie | 0.358 (0.020) | **beyond 2 sd (loss)** | 0.519 (0.016) | tie | **beyond 2 sd (gain)** |
| Ferguson* | 0.344 (0.007) | 0.386 (0.014) | 0.414 (0.005) | **beyond 2 sd (gain)** | 0.434 (0.026) | **beyond 2 sd (gain)** | 0.413 (0.033) | beyond 1 sd (gain) | beyond 1 sd (loss) |
| Pier* | 0.535 (0.003) | 0.521 (0.023) | 0.525 (0.004) | **beyond 2 sd (loss)** | 0.488 (0.015) | beyond 1 sd (loss) | 0.500 (0.018) | tie | tie |

`*` = holdout pair. "(a) vs E33" and "(c)/(b) vs Arm B" each use that
column's own governing baseline sd (E33's for (a), Arm B's for (b)/(c));
"(c) vs (b)" uses Arm B's sd as the bar, per the pre-registered clause.
Fallback windows: 0 on every fire, every arm (table above).

![Six per-fire bar groups: E33 and Arm B (their own baselines), and the three station-wind arms (a: Bernoulli, b: arrival kernel gene off, c: arrival kernel gene on), each coloured by whether it clears its own baseline's sd — Ferguson's (a)/(b) gains and Chimney's (b) loss followed by (c)'s recovery are the two moves to look for.](figures/e46-three-arms.svg)

**Prediction checked clause by clause (TEST_PLAN v1.9, §9, quoted
verbatim).**

1. *"(a) ties E33 everywhere (a round Bernoulli blob only cares about
   wind speed)."* **Fails.** Ties on 3 of 6 (Bear, Buck, Chimney);
   beyond-2sd loss on Brattain and Pier; beyond-2sd gain on Ferguson.
   The parenthetical reasoning is also not well supported: station wind
   changes the model's forecast primarily by swapping in a different
   *speed*, and the Bernoulli/exponential config is sensitive enough to
   that to move by more than the noise floor on half the fires (Result
   1).
2. *"(b) recovers most of the gene's gain on Ferguson and Chimney (E41:
   ERA5 direction is wrong there and direction carries signal on those
   two)."* **Holds for Ferguson, fails for Chimney.** On Ferguson,
   station wind alone (no gene) recovers *more than the entire*
   measured gain the gene's own arm showed over E33, by two independent
   operational definitions (+214%/+404%, Result 2) — an overshoot, not
   a partial recovery. On Chimney, station wind alone recovers *none*
   of the gene's gain — its own score is a net *loss* against E33
   (−0.077), the opposite of what "recovers most of the gain" predicts.
3. *"(c) beats (b) by less than 1 sd on ≥ 4 fires."* **Fails**, under
   either a strict ("small positive win") or loose ("tie") reading:
   only Brattain and Pier qualify, 2 of 6, short of the ≥ 4 bar (Result
   3).
4. *"If (c) beats (b) beyond 2 sd on ≥ 3 fires, the gene does more than
   repair the input."* **Antecedent not met.** Only Buck and Chimney
   clear beyond-2sd — 2 of 6, one short of the ≥ 3 bar the prediction
   set for declaring this conclusion. The pre-registered strong-evidence
   bar is not cleared, even though (c) beats or ties (b) on 5 of 6
   fires overall (Result 3) — real, fire-specific evidence the gene
   adds value beyond input repair on *some* fires (Buck, Chimney), just
   not at the strength or breadth the clause required to call it
   confirmed across the board.

**What it means.** No single verdict — "the coarse ERA5 wind is the
real problem" or "the gene does more than repair a bad input" — fits
all six fires; the honest answer is fire-specific, the same shape E45's
own mechanism study found for the gene in isolation. Two fires make
opposite, clean cases:

- **Ferguson: an input problem, not a mechanism problem.** Station wind
  alone, with no arrival kernel and no gene (arm a), already gains
  +9.41 sd over E33. Adding the arrival kernel without the gene (arm b)
  gains further (+3.56 sd over Arm B, the single best Ferguson score in
  this whole experiment, 0.434). Adding the gene back on top (arm c)
  is *worse* than (b) by 1.59 sd, though still a real gain over Arm B
  (+1.98 sd) — the gene does not help, and mildly hurts, once station
  wind has already fixed what needed fixing.
- **Chimney: a mechanism problem the input does not fix.** Station
  wind alone under the Bernoulli config (arm a) barely moves Chimney
  (a tie). Adding the arrival kernel without the gene (arm b) actively
  *breaks* Chimney — a beyond-2sd loss against Arm B, and a real loss
  against E33 too (0.358 vs 0.434), worse than not touching the wind
  input at all. Adding the gene back (arm c) recovers all of that loss
  and ties Arm B (+0.97 sd) — a +5.25 sd swing from (b) to (c), the
  largest of any (c)-vs-(b) delta and, at +0.162 raw IoU points, the
  largest absolute score movement anywhere in this write-up. The gene's
  own learned
  rotation on Chimney under station wind (−38.7°, Result 3) is *larger*
  in magnitude than what it learned under ERA5 in E44 (−29.3°,
  five-seed median) — if station wind had simply supplied "the right
  direction," a smaller correction, not a larger one, is what a pure
  input-repair story predicts. Chimney's problem looks like something
  the per-member learned correction supplies structurally, not
  something either wind input alone hands the model for free.

**A controller check on the Ferguson result, post-hoc (not re-run).**
The 10.19× Ferguson station/ERA5 speed ratio reported in Result 1 is
real, not a units bug — but it is partly an **averaging artefact**, not
purely a measurement disagreement. `validation/scripts/
convert_pytorchfire.py` builds each ERA5 daily wind entry as the
domain-mean (space) *and* already-vector-mean (time) u/v pair for that
day, then `speed = hypot(u, v)`; the converter's own provenance note
says plainly that "a swinging wind averages toward calm." The station
log, by contrast, is a single point — `station_vector_mean`'s per-window
vector average (the same convention, reused here, not duplicated) has
no domain-space averaging step to cancel against, so it retains more of
the day's actual wind magnitude. Checked across all six fires (this
write-up's own numbers, Result 1's diagnostic table): ERA5's per-window
speed means range 0.423–1.897 m/s; the station's per-window vector-mean
speed means range 1.037–4.307 m/s — **every fire's station speed is at
or above its ERA5 speed, five of six meaningfully so** (Brattain is the
one near-exception, 1.905 vs 1.897 m/s, within 1%, despite a 51.5°
direction disagreement there — the one fire whose station-wind loss is
hardest to blame on a speed-scale artefact). Ferguson's swing is the
most extreme case of this structural pattern, not a fire-specific
coincidence. **Consequence:** `SMC_WIND_SOURCE=station`, as built,
changes two things at once — the wind's *direction* (the intended
comparison) and its *speed-averaging methodology* (domain+time
vector-mean vs a single point's own vector-mean) — and this write-up's
"input vs mechanism" verdict, especially for Ferguson, cannot fully
separate which of the two is doing the work. A fairer future comparison
would hold the speed treatment fixed while swapping only the direction
(or vice versa) — named as the next experiment below, not run here.

**Questions this raises.**

- Does Ferguson's gain survive if the speed-averaging artefact is
  controlled for — i.e. does *direction alone* (station bearing, ERA5-
  style domain+time-averaged speed) still gain over E33/Arm B, or does
  the gain disappear once the speed scale is held fixed? The controller
  check above cannot answer this without a new run.
- Why does Brattain lose under station wind in *every* arm (a, b, and
  c all lose to their own baseline, none beyond a tie) despite its
  speed staying almost unchanged (1.00× ratio) — is the 51.5° direction
  disagreement alone enough to explain a loss the ±90° gene cannot
  correct, or is something else about Brattain's station log (siting,
  terrain channeling — the scenario notes 40–70 km station-to-fire
  distances, per the glossary's "Station weather" entry) responsible?
- Chimney's learned `wind_rot_deg` grows *larger* under station wind
  than under ERA5 (−38.7° vs −29.3°) rather than shrinking — is the
  gene compensating for a station-log direction that is itself biased
  in a consistent way on this fire (a siting or terrain effect, not
  ERA5's error), or is this within the kind of per-seed spread E44/E45
  already found the gene shows regardless of input?
- Three seeds is this experiment's own count (not E44's five) for every
  arm; Brattain's near-zero station_b sd (0.001) suggests real
  precision at three seeds on some fires, but Ferguson's arm (c) sd
  (0.033, the largest three-seed sd in this whole write-up) is a
  reminder that fewer seeds means noisier verdicts, especially exactly
  where this write-up's story turns on Ferguson's own number.

**Verdict.** **Fire-specific, not one story — input problem on
Ferguson, mechanism problem on Chimney, neither pre-registered branch
of the (c)-vs-(b) prediction confirmed at its own evidentiary bar.**
The gene is not redundant with a better wind input (Chimney's +5.25 sd
swing from (b) to (c) is real and large), and a better wind input is
not redundant with the gene either (Ferguson's (b) beats (c)) — the two
interventions trade off fire by fire, echoing E45's own "fire-specific,
not one mechanism" verdict on the gene in isolation. The pre-registered
"≥ 3 fires beyond 2 sd" bar for declaring "the gene does more than
repair the input" is not cleared (2 of 6), so that strong claim is not
confirmed as tested — but the data are also not consistent with the
opposite claim either. That opposite claim, "station wind alone
recovers most of what the gene does," was only pre-registered and
checked for the two fires TEST_PLAN v1.9 actually named (Ferguson,
Chimney; clause 2 above): it held, and overshot, for Ferguson, and
failed outright for Chimney. Mechanically extending "recovered
fraction" to the other four fires does not give a clean count either
way — Bear's and Pier's own Arm B-over-E33 "gene gain" (E44) is itself
negative or near zero there, so "fraction of the gain recovered" is not
a meaningful quantity to divide by on those two fires, and this
write-up does not claim one. Read plainly: **this experiment does not
settle "input vs mechanism" as a single answer, because the six fires
do not agree with each other, and the two fires the prediction named
specifically disagree with each other.**

**Later.** The averaging-artefact controller check above names the
clear next step, not run in this task: an arm that holds wind *speed*
scale fixed (ERA5-style domain+time vector-mean magnitude) while
substituting only the station log's *direction*, and/or the reverse
(station vector-mean speed with ERA5's own direction) — this would
separate the Ferguson result's two conflated causes (direction repair
vs. a structural speed-averaging difference between the two inputs) in
a way this task's binary `SMC_WIND_SOURCE=era5|station` knob cannot.
Brattain's station-wind loss (present in all three arms here, unrelated
to the gene) is unexplained and worth its own look, possibly alongside
E48-style per-window diagnostics (`SMC_DIAG=1`) on this fire's station
log specifically. Neither revisited yet.
