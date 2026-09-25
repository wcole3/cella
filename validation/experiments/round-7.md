# Round 7 — 2026-09-23/25: the promotion test, its mechanism, and why it didn't clear the bar

_Score family: mixed — five-seed forecast consensus IoU against the Arm
B/E33 noise floors (E44, E45, E46 arms a/b/c), a per-window diagnostic
read with no score family of its own (E48), and an operator audit plus a
single-seed calibration sweep plus a read-only ICS-209 re-analysis (E49) ·
base = the E30b Arm B configuration (arrival kernel, rear-focus wind law,
4× clock, `arrival_x4` prior, `wind_rot_deg` gene ±90°) unless a row says
E33/Bernoulli · all six fires except E49's sweep (four calibration fires
only; Ferguson and Pier untouched) · pre-registered TEST_PLAN v1.9 ·
terms: [GLOSSARY.md](GLOSSARY.md)_

> A note on precision, once for this whole file (the individual E44/E45/
> E46/E48 write-ups do not each repeat it, E49 does): every mean, sd and
> delta quoted below is computed from the full-precision JSON the
> `wildfire_smc` reports carry; the values printed in prose and tables are
> rounded for readability. A "0" delta on a byte-identical pair (E49's
> growth-floor sweep) is an exact zero, not a rounded one — the write-ups
> say so explicitly where it matters.

## What we knew before

Round 6 ended with a one-seed pilot, not a result. E30b's Arm B (a 4×
faster clock, a widened `p0`/`burn_duration` prior, and a free per-member
`wind_rot_deg` gene) beat or tied E33 on all six fires in a single seed,
reversing the arrival kernel's own real-fire rejection (E30) — but it
rested on one seed and a noise floor borrowed from a different
configuration. Round 6's plan (`docs/superpowers/plans/round-7-experiments.md`)
set the bar plainly:

> **Does the E30b Arm B configuration deserve to replace the Bernoulli
> recommendation, and if so, why does it work?** Arm B becomes the
> recommended configuration only if Task 4 passes its stop rule **and**
> Task 5 or Task 6 gives a mechanism we can state in one sentence. A gain
> we cannot explain is a result to report, not a default to ship.

Round 7 runs that test. Five of Round 6's "still open" items bear on it
directly: the full five-seed E30b run and an E37b re-run at the 4× clock
(item 1); the mechanism ablations, diversity vs. learning (item 2); and
the ICS-209 follow-up on the containment operator (part of item 5). Two
of Round 6's items are untouched this round and remain open below: the
arrival + rear_focus + spotting genes combined (item 3), and the hull
overshoot at high wind (item 4). Two hygiene items Round 6 flagged —
`make clippy` not linting `cella_lib`'s own examples, and
`wildfire_smc.rs`'s size — were already fixed in Round 7 Task 1 (`make
clippy` now also runs from `cella_lib/`; `cella_lib/examples/wildfire_smc/`
is a small module tree instead of one 2,420-line file), so they are not
carried forward here.

## What we ran

| # | Question | Answer | Verdict |
|---|---|---|---|
| [E48](49-e48-brattain-arrival-diagnosis.md) | Why does Brattain fail under Arm A (no gene), window by window, and what does Arm B's gene actually correct? | ERA5 and the station log disagree most (83°–139°) on exactly the three windows that carry 50.6 % of Brattain's burned area; Arm A misses catastrophically downwind on the two biggest of those three, then over-grows everywhere instead of catching up. Arm B's gene cuts the downwind miss 42–45 % on those two — but helps by a similar or larger margin on a well-agreeing window too, so "exactly those windows" overclaims | finding, not a lever test |
| [E44](50-e44-full-e30b-arm-b.md) | Does the full five-seed Arm B beat E33, and does the 4× clock bring the three excluded fires (Brattain, Ferguson, Pier) inside E37/E37b's reachable-shape wedge? | Three of six fires beat E33 beyond 2 sd as predicted (Brattain, Chimney, Ferguson); Bear ties; but Pier loses by 4.60 sd (E33's own sd only 0.003) and Buck's +1.27 sd gain is real but not a tie. The 4× clock recovers illumination coverage past E37's own baseline on three of six fires, but only Ferguson newly enters the wedge, by a 0.01 margin | **REJECTED as tested — stop rule trips on Pier; Arm B not promoted** |
| [E45](51-e45-wind-rot-mechanism.md) | Does the `wind_rot_deg` gene help through per-member angular diversity, or through the filter learning one correct bearing? | σ0 (diversity, no learning) ties Arm B on four of six fires, Chimney gains, Buck is the one clear loss. ±20° (learning, narrow range) ties on three, loses beyond 2 sd on three (Bear, Buck, Chimney). Chimney and Bear make opposite, equally clean cases — one needs a specific learned value, the other needs width, not a value | **UNDETERMINED as one sentence — fire-specific, not one mechanism** |
| [E46](52-e46-station-wind-input.md) | Is the coarse ERA5 daily wind the real problem behind the gene's gains, or does the gene do more than repair a bad input? | Ferguson: station wind alone, no gene, already gains +9.41 sd over E33 — an input problem, and the gene adds nothing once it's fixed. Chimney: station wind alone actively breaks the fire (−4.28 sd vs. Arm B); only the gene recovers it (+5.25 sd, the largest single-fire swing and the largest absolute IoU movement in this round) — a mechanism problem. A post-hoc check found Ferguson's result is partly a structural wind-averaging artefact, not purely direction repair | fire-specific; neither pre-registered branch confirmed at its own evidentiary bar |
| [E49](53-e49-containment-under-4x-clock.md) | Does the containment operator fail to scale under the 4× clock, explaining Chimney's contained-fraction drop E30b's pilot saw? | No — every quantity the operator reads scales correctly (automatically, or, for `model.burn_duration`, by the `arrival_x4.json` prior's own ×4-widened range). The growth-floor sweep (1e-5/1e-4/1e-3) is byte-identical on all four calibration fires: the smallest daily growth any still-burning Bear member ever showed (0.052) is ~50× the largest floor tested. Chimney's five-seed contained fraction (0.794, sd 0.222) ties E33's (0.806, sd 0.100) — the 0.59 the pilot saw was seed 0 alone. The ICS-209 lead is unchanged (Bear/Buck still 13–18 days; Chimney 8.7, was 9.1) | finding: no bug, no threshold effect, no lever |
| E47 — arrival + rear_focus + spotting genes | *(not run)* | Per TEST_PLAN v1.9's pre-registered consequence text, E44's stop rule tripping means E47 does not run. Not attempted this round | skipped per stop rule |

## What we know now

1. **Brattain's Arm-A failure traces to two specific windows, not a
   general direction error** (E48). Of Brattain's 226,193 total burned
   cells, days 5 and 6 (hours 120/144) alone carry 20.9 % and 17.4 % of
   the season's total, and ERA5 disagrees with the station log on those
   two windows by 139.3° and 82.7° — by far the largest disagreement of
   any window with meaningful growth. Arm A (no gene) misses 52,147 and
   86,673 cells downwind on exactly those two windows, then cannot catch
   up: its mean member area balloons to 1.57–1.58× the observed burned
   area by days 14–16. Arm B's gene cuts the downwind miss 42.0 % and
   45.0 % on those two windows and avoids the late over-growth (area
   ratio settles at a flat 0.61× instead of climbing past 1.5×) — but it
   also helps by a comparable 40.7 % on day 7, where ERA5 and the station
   agree fairly well (23.5°), and it barely helps on day 4 (5.0 %) despite
   that window's own large disagreement (96.6°). The prediction's first
   clause ("ERA5 is right on the mean but wrong on the days that carry
   most of the growth") held cleanly; its second ("the gene covers
   exactly those windows") overclaimed.

2. **The full five-seed Arm B beats E33 on four fires but fails the
   pre-registered stop rule on Pier** (E44). Brattain (+5.73 sd), Chimney
   (+4.70 sd) and Ferguson (+5.69 sd) all beat E33 beyond 2 sd; Bear ties
   (−0.42 sd). Pier loses 4.60 sd (mean 0.521 vs. E33's 0.535, sd 0.003)
   — more than four times the stop rule's 1 sd bar, and not one bad seed:
   even excluding Pier's single worst seed, the remaining four still sit
   1.39 sd below E33. Buck gains +1.27 sd, a real but not a tie result,
   against a prediction of "within its own sd." Arm B's own five-seed sd
   — the noise floor every later arrival-kernel arm was judged against —
   is 8.86× E33's on Brattain and 7.45× on Pier, the two fires with the
   widest, sign-disagreeing per-seed spread in the learned `wind_rot_deg`
   gene. The 4× clock's own illumination test (E37b re-run at 4×) recovers
   coverage past E37's own base-model archive on three of six fires
   (Brattain 125 cells vs. E37's 112; Chimney 57 vs. 48; Ferguson 68 vs.
   52) and Brattain and Ferguson stop being unreachable *in size* — but of
   the three fires E37/E37b's wedge excluded, only Ferguson now sits
   inside it, and only by a 0.01 margin (elongation 1.62 vs. observed
   1.61) at the exact size threshold. Brattain recovers the size but not
   the shape (1.34 vs. an observed stretch of 1.83); Pier barely moves at
   all.

3. **The gene's own mechanism is fire-specific, not one sentence** (E45).
   Freezing the gene's mutation (`SMC_WIND_ROT_SIGMA=0`, diversity kept,
   learning removed) ties Arm B within 1 sd on four of six fires (Bear,
   Brattain, Ferguson, Pier), gains on Chimney (+1.13 sd), and loses only
   on Buck (−2.92 sd). Narrowing the gene's range to ±20° (learning kept,
   diversity narrowed) ties on three (Brattain, Ferguson, Pier) and loses
   beyond 2 sd on three (Bear −2.24 sd, Buck −2.62 sd, Chimney −2.82 sd,
   the largest single move in the experiment). Chimney and Bear make
   opposite, equally clean cases: Chimney's own learned median (−29.3°)
   sits entirely outside ±20°, so a narrow range cannot represent the
   correction it needs — a *learning* story; Bear's median (−7.6°) sits
   comfortably inside ±20°, yet the narrow arm still loses while the
   frozen arm ties — a *diversity* story, where a wide simultaneous spread
   across members matters independent of where the median lands. No
   single explanation fits both. The per-window IQR series adds a real
   caveat to any future read of this gene: under a frozen gene, the
   population's `wind_rot_deg` spread collapses from a birth-level ≈ 97°
   down to 0.7°–43° purely through resampling thinning which birth draws
   survive — with mutation entirely off. That means an IQR trend narrowing
   over a run, by itself, cannot tell "the filter learned a bearing" apart
   from "resampling degeneracy alone did it" — which is why the optional
   third batch (Arm B itself under `SMC_DIAG=1`) was ruled out and not
   run: it could not have discriminated the two stories either.

4. **Station wind as a driver input is fire-specific too, and the two
   fires the prediction named split cleanly in opposite directions**
   (E46). Feeding the driver the station log's own vector-mean wind
   instead of ERA5, under the plain Bernoulli/E33 config (arm a), does
   not "tie everywhere" as predicted — it ties on three of six fires, but
   loses beyond 2 sd on Brattain and Pier and gains beyond 2 sd on
   Ferguson (+9.41 sd, the single largest arm-a movement). Adding the
   arrival kernel with the gene off (arm b) shows the split most clearly:
   on **Ferguson**, station wind alone already recovers *more than the
   entire* measured gain the gene showed over E33, by two different
   operational definitions (+214 %/+404 %) — an input problem, not a
   mechanism problem, and the gene added back on top (arm c) is mildly
   worse than arm b there (−1.59 sd). On **Chimney**, station wind alone
   actively breaks the fire relative to Arm B (−4.28 sd, arm b) — worse
   than not touching the wind input at all — and only adding the gene
   back (arm c) recovers it, a +5.25 sd swing from (b) to (c), the largest
   of any (c)-vs-(b) delta in the experiment and, at +0.162 raw IoU
   points, the largest absolute score movement anywhere in Round 7. A
   controller check found after the fact that ERA5's daily wind is a
   domain-*and*-time vector mean ("a swinging wind averages toward calm,"
   in the converter's own provenance note) while the station log's vector
   mean is a single point with no such averaging — every fire's station
   speed sits at or above its ERA5 speed, Ferguson's 10.19× swing the most
   extreme case of a pattern true, to some degree, on five of six fires.
   So Ferguson's result cannot be cleanly split into "direction repair"
   vs. "a structural speed-averaging difference between the two inputs"
   — the next experiment this round names but does not run.

5. **The containment operator is not why Arm B behaves differently on
   Chimney; it was one seed** (E49). An audit of every constant the
   operator reads (`steps_per_day`, the growth window, `tau_days`,
   `SMC_ASSIM_EVERY`, `SMC_MAX_DAYS`) found each scales correctly with
   the 4× clock, either automatically through the scenario's own scaled
   `steps_per_hour`, or, for `model.burn_duration`, by construction
   through the `arrival_x4.json` prior file's own ×4-widened range — not
   an automatic mechanism, so a future 4×-clock run that forgets to also
   set `SMC_PRIOR` would not get this scaling, a footgun worth flagging.
   A three-value sweep of the operator's one fixed threshold (the growth
   floor, 1e-5/1e-4/1e-3) came back byte-identical on all four
   calibration fires — not just close, exactly identical, confirmed by
   MD5. A one-job diagnostic recording all 147 of Bear's containment
   draws found the smallest daily growth any still-burning member ever
   showed was 0.052, about 50× the largest floor tested — members are
   contained while still growing several percent a day, long before
   growth comes anywhere near the floor. The question's own premise
   mostly dissolved: across E44's five seeds, Chimney's contained fraction
   under Arm B is 0.794 (sd 0.222), a tie with E33's 0.806 (sd 0.100); the
   0.94 → 0.59 drop E30b's pilot saw was seed 0 alone, one of Arm B's two
   lowest against E33's own highest. The ICS-209 lead E42 measured does
   not shrink toward 5–10 days under the 4× clock either: Bear moves
   13.4 → 13.0 days, Buck 17.9 → 17.4 (pooled over members), both inside
   seed noise; Chimney was already inside the 5–10 range under E33 (9.1
   days) and stays there (8.7).

6. **E47 did not run.** Per TEST_PLAN v1.9's pre-registered consequence
   text, E44's stop rule tripping on Pier means "E47 does not run" — the
   arrival + rear_focus + spotting-genes combination stays untested this
   round, carried forward as still open (below).

## The promotion decision

The plan's own question: **"Does the E30b Arm B configuration deserve to
replace the Bernoulli recommendation, and if so, why does it work? Arm B
becomes the recommended configuration only if Task 4 passes its stop rule
and Task 5 or Task 6 gives a mechanism we can state in one sentence."**

Checked against both halves of that rule:

- **Task 4's stop rule tripped.** E44's five-seed Arm B loses to E33 on
  Pier by 4.60 sd — more than four times the pre-registered 1 sd bar, and
  robust to seed choice (the fires-4-of-5 mean is still 1.39 sd below
  E33). Per the stop rule's own text, this alone means **"Arm B is not
  promoted."**
- **Neither Task 5 nor Task 6 gave a one-sentence mechanism.** E45's own
  verdict states it plainly: "Undetermined as a single sentence — the
  evidence splits by fire, not by one universal explanation." E46's own
  verdict is the same shape: "Fire-specific, not one story — input
  problem on Ferguson, mechanism problem on Chimney, neither
  pre-registered branch of the (c)-vs-(b) prediction confirmed at its own
  evidentiary bar." A gain that needs a different explanation on every
  fire is not the "mechanism we can state in one sentence" the rule
  required.

**Arm B is not promoted.** Both conditions were required; neither is met.
This is not a close call on either count — the stop rule trips more than
four times over on its own, and two independent experiments designed to
supply the mechanism clause both concluded, independently, that no single
mechanism fits.

**The recommended production configuration is unchanged from Round 5/6:**
Bernoulli spread rule, 32 members, β 10, σ 0.2, immigrants 0.2,
containment-only stopping, E25's broad prior. Five-seed means (E33,
unchanged): Bear 0.479, Brattain 0.416, Buck 0.590, Chimney 0.434,
Ferguson 0.344, Pier 0.535.

**Method lesson.** Two independent designs (E45's gene-mechanism
ablation, E46's input-vs-mechanism split) both landed on the same shape
of answer — "fire-specific, not one thing" — for two different questions
about the same gene. That is itself evidence worth taking seriously: this
campaign's habit of asking for one mechanism that explains all six fires
may be the wrong frame for a per-member learned correction whose value is
that different fires need different amounts of it. A future design that
starts from "which fires need what" (E46's own split, Ferguson vs.
Chimney) rather than "what is the one true mechanism" would likely be
more informative than a third ablation aimed at forcing a single-sentence
answer where two honest experiments already said there isn't one.
Separately, E46's post-hoc finding — that a gridded reanalysis's domain
mean and a weather station's point mean are not the same "wind speed"
even when both convert to the same units — is a general trap for any
future comparison of the two input types, not specific to this round's
station-wind knob.

## Still open after this round

In order:

1. **Separate Ferguson's direction repair from its speed-averaging
   artefact** (E46's own "Later"): an arm that holds wind *speed* scale
   fixed (ERA5-style domain+time vector-mean magnitude) while
   substituting only the station log's *direction*, and/or the reverse.
   `SMC_WIND_SOURCE`'s binary `era5|station` knob cannot do this as built.
2. **Brattain's station-wind loss is unexplained.** It loses to its own
   baseline under all three of E46's arms (a, b, c), despite an almost
   unchanged wind speed (1.00× the ERA5 mean) — the largest direction
   disagreement of any fire that still ties or loses cleanly elsewhere.
   E48-style per-window diagnostics (`SMC_DIAG=1`) on this fire's station
   log specifically would be the cheap next step.
3. **Is the resampling-degeneracy IQR collapse `wind_rot_deg`-specific,
   or a general property of this ensemble's selection pressure?** (E45).
   A σ0-style freeze on a different gene (e.g. `model.p0`) was not tested
   and would settle it.
4. **Arrival + rear_focus + spotting genes (E43) together** — still not
   combined; carried from Round 6, untouched this round because E47 did
   not run.
5. **A finer angular neighbourhood, or a fitted template-LB correction**,
   for the rear-focus hull overshoot that remains at high wind (LB > 1.5)
   — carried from Round 6, named and left unfixed in E30a, not revisited.
6. **A fuel term in the containment operator.** E49's audit narrows this:
   if the containment model is ever revisited, the lever is the learned
   genes (`contain_a`/`contain_b`) and what "contained" is scored
   against, not the growth floor (which is now known to be inert on
   these fires) — carried from Round 4/5/6.
7. **Housekeeping deferred from this round's own review passes**, listed
   rather than fixed now because none is cheap enough to fold in:
   `validation/scripts/experiments/r7_common.py` still bundles the shared
   batch-launch gates (binary/load checks) and the per-fire summariser in
   one 409-line file (Task 2's own acceptance noted this and deferred
   splitting it); `r7_common._run_map` duplicates roughly five of
   `r5_common.run`'s own launch lines rather than calling it directly (the
   two report shapes, `assim` vs. `map`, do not share a parser); the IQR
   unit test added for E45 (`diag::iqr_gene`) covers the even-length
   quantile-interpolation case with its worked 10-value example but has
   no matching odd-length case.
8. **Crate coverage** was at ≈ 98.3 % coming into this round, below the
   99 % gate — addressed directly in this task (see "Coverage," below),
   not left open.

## Configuration after this round

**Unchanged** from Round 5/6, and confirmed as the recommendation by this
round's promotion decision above: Bernoulli spread rule, 32 members, β
10, σ 0.2, immigrants 0.2, containment-only stopping, E25's broad prior.
Five-seed means (E33): Bear 0.479, Brattain 0.416, Buck 0.590, Chimney
0.434, Ferguson 0.344, Pier 0.535.

**New, opt-in, all off by default:**

- `SMC_DIAG=1` — per-window diagnostics (E48, extended by E45 and E49):
  ERA5 and station wind vectors, learned-gene medians, the head/flank
  miss decomposition, `wind_rot_deg_iqr`, and (with `--arm diag` in
  E49's runner) per-draw containment records. `None`/omitted from every
  report when unset — byte-identical to before this knob existed.
- `SMC_WIND_ROT_SIGMA` — a per-gene mutation-size override, applied to
  `wind_rot_deg` only (E45). Unset leaves the gene mutating at the
  engine's own sigma exactly as before.
- `SMC_WIND_SOURCE=era5|station` — swaps the driver's per-window forcing
  from the scenario's ERA5 schedule to the station log's own vector mean
  (E46). Default/unset (`era5`) is byte-identical to before this knob
  existed; a window with no station rows in range falls back to that
  window's own ERA5 entry, counted in `station_fallback_windows`.
- `SMC_CONTAIN_GROWTH_FLOOR` — overrides the containment operator's
  `.max(1e-4)` growth floor (E49). Unset/default reproduces `1e-4`
  byte-for-byte; the knob validates its input (any non-positive or
  non-finite value panics with a named reason, fixed in `cff55c6` after
  review) and echoes the floor it used into the report.

**Arm B itself (arrival kernel, rear-focus wind law, 4× clock, the
`arrival_x4.json` prior, `wind_rot_deg` at ±90°) remains a tested,
documented option — not the recommendation.** It is **REJECTED as tested**
(E44's stop rule trips on Pier), and its own gain has no single-sentence
mechanism (E45, E46 both fire-specific). Any future attempt to recover
value from it would most plausibly start from E46's own split — station
wind for Ferguson-like input problems, the gene for Chimney-like
mechanism problems — rather than one bundle applied uniformly to all six
fires; that itself would be a departure from this campaign's "nothing
chosen per fire" rule and would need its own pre-registration before
being run.

**Hygiene closed this round:** crate coverage restored to ≥ 99 % lines
(measured before and after; see "Coverage," below); a read-only
bench-profile study added for the long-suite benchmarks, comparing the
release profile against fat-LTO/`panic=abort`/`target-cpu=native`
variants (`docs/performance.md`, "10. Bench profile study
(2026-09-25)") — no baseline changed, no runner's `BIN` changed;
`figures_r7.py` renders the round's own SVGs the way `figures_r6.py` did
for Round 6.
