# Round 6 — 2026-09-11/12: nulls, state correction, and a kernel that tells time

_Score family: mixed — deterministic nulls (E41), a read-only re-analysis of
existing runs (E42), five-seed forecast consensus IoU against the E33 twins
(E39, E40/E40b, E30, E30b), MAP-Elites illumination coverage (E43, E30a's
own flat-grid measurements), and one performance study (no score at all) ·
base = the E31/E25 recommended configuration unless a row says otherwise ·
all six fires, nothing chosen per fire · pre-registered TEST_PLAN v1.8 ·
terms: [GLOSSARY.md](GLOSSARY.md)_

## What we knew before

Round 5 asked what the *methods* — the ensemble, the particle filter's
operators, the offline genetic algorithm — do, and left five things open:
a kernel refit (E30) using E37's reachable-shape wedge as the acceptance
test; a gated version of the immigrant-reset repair (E39); replicating
crossover 0.5 over five seeds; the ICS-209 containment check and a fuel
term (carried from Round 4); and a fitted start for the filter on
Ferguson-like fires. Round 5 also fixed the bar every later claim is
judged against: the E33 noise floor, five seeds of the recommended
configuration, sd 0.015 (Bear) / 0.004 (Brattain) / 0.039 (Buck) / 0.012
(Chimney) / 0.007 (Ferguson) / 0.003 (Pier). A difference smaller than a
fire's own sd is a tie.

Round 6 chases three of those threads — the kernel refit, the gated
reset, the ICS-209 check — and adds two more: a third dumb forecaster (the
Ellipse) to ask whether wind *direction* carries any shape signal at all
before spending effort teaching the model to use it, and a state-correction
mechanism (rebuild an immigrant's grid from the actual observed perimeter,
not just clear a flag) that neither E38 nor E39 had tried.

## What we ran

| # | Question | Answer | Verdict |
|---|---|---|---|
| [E41](41-e41-ellipse-null.md) | Does wind direction carry shape signal on these six fires? | Yes on Brattain and Ferguson — but a post-hoc control shows it is almost entirely the front/back *sign*, not the *stretch* | FINDING |
| [E42](42-e42-posterior-trajectories.md) | Does the learned p0 drift as predicted; does the model's containment curve lead ICS-209 as predicted? | Direction held (per-fire drift, always leads ICS-209); the *sizes* were 13–21 days off, not 5–10, and Pier's p0 rose instead of falling | finding, not a lever |
| [E39](43-e39-gated-immigrant-reset.md) | Does gating the immigrant reset on the area ratio keep E38's Buck gain without Pier's cost? | Protects Pier fully, but on Buck it fires only after 91–100 % of members are already contained — too late to help | REJECTED (as tested) |
| [E40 / E40b](44-e40-observed-perimeter-immigrants.md) | Does rebuilding an immigrant's grid from the observed perimeter recover the forecast? | Beats E33 by more than sd on every fire, but loses to the trivial "yesterday's map" forecast by 0.3–0.4 IoU (E40); correcting everyone (E40b) narrows that to 0.10–0.25 but still loses on 96.6 % of windows where the fire grew | KEPT as options, NOT a nowcast |
| [E43](45-e43-spotting-illumination.md) | Does spotting widen the reachable-shape wedge? | Yes, robustly, on Ferguson (1.38→2.66, confirmed by a connected-component replay); a fragile yes on Pier (1.15→1.77, 6 of 15 replays); no change on Brattain | FINDING |
| [E30a](46-e30a-arrival-time-kernel-flat.md) | Does a minimum-travel-time kernel with a rear-focus wind law match Anderson's real-fire shape formula? | After finding and fixing a diagonal-cost bug and a wrong "closed form" target (both controller errors caught in review), yes: within ~5 % of Anderson for the wind range the ensemble actually uses | FINDING (kernel validated for Task 8) |
| [E30](47-e30-arrival-time-kernel-fires.md) | Does that validated kernel actually help the six real fires? | No — the reachable-shape wedge shrinks instead of widening, and the forecast loses to its E33 twin by more than the noise floor on 5 of 6 fires | REJECTED (as tested) |
| [E30b pilot](48-e30b-uncapped-clock-direction-gene-pilot.md) | Do a faster clock, a wider speed prior, and a learned wind-direction gene fix E30? | The clock/prior fix alone is not enough (Brattain gets worse); adding the learned gene beats or ties E33 on all six fires | Arm B PASS (one-seed pilot); recommend the full run |
| Tooling — [ensemble parallelism](../../docs/performance.md#9-ensemble-stepping-parallelism-2026-09-12) | Is the default thread-scheduling heuristic right for these two grid sizes? | A single run is 24–57 % faster (bit-identical) with member-parallel stepping, but the only arm that matters for a concurrent batch of runs is 11.4 % faster — below the pre-registered 20 % bar | no default changed |
| Tooling — binary provenance stamp | Can every report be traced to the exact binary that produced it? | Yes — `binary_git` / `binary_built_utc` now written into every `wildfire_smc` report (all four modes), confirmed against `git rev-parse` and a fake-`git` fallback test | shipped |

## What we know now

1. **Wind direction carries real shape signal on Brattain and Ferguson,
   almost entirely as a *sign*, not a *stretch*** (E41). A wind-oriented
   Ellipse null beats the area-matched Circle beyond the E33 sd on
   Brattain (+0.019, sd 0.004) and Ferguson (+0.131, sd 0.007). A
   post-hoc control that removes the ellipse's ability to express a
   front/back sign at all — moving the ignition to the ellipse's centre —
   ties the Circle on four of six fires and only marginally moves on the
   other two (Brattain −0.005, Ferguson +0.009, an order of magnitude
   smaller). Chimney, whose ERA5 wind is already known to point the wrong
   way (README, E9), loses hard on the sign-carrying variant (−0.124, sd 0.012, loses) but
   just ties under the centred control — the same lesson from the other
   direction. This retargets E30: the kernel needs the wind response's
   *direction* fixed, not a generically "more anisotropic" shape.

2. **The particle filter's posterior drifts the way theory predicted on
   most, not all, fires** (E42). p0 falls on Bear and Buck and rises on
   Ferguson, as predicted; Pier rises instead of falling. The model's
   learned containment curve always reaches 50 % contained members before
   ICS-209 reports 50 % containment — but by 13.4 days on Bear and 17.9
   days on Buck, roughly double the pre-registered 5–10-day estimate. Not
   a lever test; no configuration changed as a result.

3. **Clearing a "contained" flag on evidence arrives too late to help the
   one fire that needed it** (E39). Gating the immigrant reset on the
   population's area ratio protects Pier perfectly — its E38-era loss
   disappears on all five seeds, exactly (+0.000 every time) — but on
   Buck the gate only fires after 91–100 % of members are already
   contained, nine days into a 29-window run. By then the reset changes a
   bookkeeping flag, not the forecast: Buck seed 3 ties E33 to three
   decimal places (0.527 both), not E38's repaired 0.632. Kept in the
   engine (tested, documented, off by default) but not recommended over
   plain `immigrant_reset` for repairing lock-in.

4. **Correcting the grid's actual state, not just a flag, is a much
   stronger fix — but it is not a usable nowcast** (E40/E40b). Rebuilding
   an immigrant's cells from the observed perimeter beats E33 by more
   than sd on every fire (Chimney +0.141, sd 0.012; Ferguson +0.260, sd
   0.007), and correcting the whole population instead of the usual 20 %
   (E40b) helps further on every fire and metric checked. But scored the
   honest way — against **lagged persistence**, yesterday's real map used
   unchanged as today's guess, which sees exactly what these modes see
   and nothing more — E40 loses by 0.3–0.4 IoU on every fire, and E40b
   only narrows that to 0.10–0.25 while still losing on 96.6 % (676 of
   700) of the windows where the fire actually grew. Both modes are kept
   as engine options; neither is recommended as a forecast product, and
   this file's own score-family lesson (below) exists because of it.

5. **Spotting reaches the wedge on Ferguson, not on Brattain, and Pier's
   case is fragile** (E43). Adding two spotting genes to E37's
   illumination map roughly doubles or more the reachable-shape coverage
   on every fire. Ferguson's reachable elongation at its own size rises
   from 1.38 to 2.66 (observed 1.61); a connected-component replay (does
   the shape stay one piece, or is it scattered embers?) confirms this is
   real — every one of 15 fresh replays clears the observed shape, worst
   case by +0.40. Pier's rise (1.15 → 1.77, observed 1.45) survives the
   same check only technically: 6 of 15 replays clear the bar, by at most
   +0.10. Brattain's ceiling barely moves (1.35 → 1.49) and stays well
   under its observed 1.83 on every check — it remains the one fire no
   combination of these knobs can draw.

6. **A kernel that tells time (arrival) fixes the wind kernel's shape
   mechanism — once two mistakes review caught were removed** (E30a). A
   full-diff review found `step_chunk_arrival`'s travel cost had doubled
   the diagonal-vs-cardinal step-cost ratio (`2×` instead of the correct
   `√2×`) at every wind speed, including calm wind — a plain code bug,
   not the "8-direction sampling can't track a needle-thin ellipse"
   geometric limit an earlier fix round had blamed for the same
   overshoot. Separately, and independently, the exponential wind law's
   own "closed form" target (`cosh(c2·v)`) that every earlier round
   measured against was itself wrong math — **a controller error found in
   review**: the true minimum-travel-time half-width peaks near 50°, not
   at the 90° flank the formula assumed. Both are stated plainly in
   `46-e30a-arrival-time-kernel-flat.md` rather than smoothed over. With
   both fixed, the recommended (arrival, rear_focus) kernel matches
   Anderson's real-fire length-to-breadth formula within about 5 % for
   LB ≤ 1.5 — comfortably covering the six fires' real operating wind
   (ERA5 0.5–0.7 m/s times the ensemble's own wind-multiplier gene ceiling
   gives LB ≤ ~1.3), where the overshoot is now roughly 0 %, not the
   previously reported "≤ ~20 %."

7. **That validated kernel makes the six real fires' forecasts worse, not
   better, when dropped in as-is** (E30). The reason is units, not the
   kernel's mechanism: `model.p0` is a saturating *probability* under the
   old rule and an unsaturating *rate* under the new one, and the same
   numeric gene range means something different under each. Left
   numerically unchanged (per this task's own scope), the illumination
   search cannot even reach Brattain's or Ferguson's observed day-5 size
   — there is no elite to judge shape from at all, a strictly worse
   failure than E37's "wrong shape." The forecast loses to its E33 twin
   by more than the noise floor on 5 of 6 fires, including both fires
   the pre-registered prediction expected to improve (Chimney −0.144, sd
   0.012; Brattain −0.111, sd 0.004). Only Buck gains, and only inside
   its own wide noise band (+0.035, sd 0.039).

8. **A faster clock plus a learned per-member wind-direction gene
   recovers the kernel, in a one-seed pilot** (E30b). A 4× faster clock
   and a wider p0 prior alone (Arm A) is not enough: Brattain — the fire
   E41 said had the *right* ERA5 direction — posts the pilot's single
   worst result, 13.2 (borrowed) sd below E33. Adding a free, per-member
   `wind_rot_deg` gene (Arm B) fixes essentially everything Arm A could
   not: all six fires beat or tie E33 (Brattain +8.1 sd, Chimney +4.3 sd,
   Ferguson +6.5 sd, Buck +1.2 sd; Bear and Pier tie), and Arm B beats
   Arm A outright on every fire. The learned rotations stay small
   (9°–44°) even on the biggest wins, which argues the gain comes from
   giving the *ensemble* more angular diversity to hedge a single
   noisy daily wind bearing, not from any one member learning "the true"
   correction. This is a genuine pass on the pilot's own pre-registered
   stop rule, but it rests on one seed and a noise floor borrowed from a
   different configuration — not yet a five-seed result.

9. **Two tooling findings, no forecast changed.** Stepping ensemble
   members in parallel instead of one-at-a-time is 24–57 % faster per run
   and bit-identical (`docs/performance.md` §9), but the only arm that
   matters for a batch of concurrent runs — today's process shape plus
   the parallel-stepping knob — is 11.4 % faster, below the pre-registered
   20 % bar for changing a default, so `r5_common.BASE_ENV` is unchanged.
   Separately, every `wildfire_smc` report (all four modes) now carries
   `binary_git` / `binary_built_utc`, so a stale or dirty build can no
   longer produce a number that silently lands in a table.

**Method lessons for `cella_lib::explore` users.** A rear-focus wind
template can look like it has found real shape when it has only gotten
the wind's *sign* right — build a centred control that cannot express a
sign, the way E41 did, before crediting a directional mechanism with a
shape win. A state-corrected forecast (one that gets to see yesterday's
real map) answers a different, easier question than a from-ignition
forecast, and needs its own honest baseline — the lagged null, not the
E33 twin — scored as a separate score family, never mixed into the same
table without saying so. A second-moment elongation number cannot tell a
genuinely stretched shape from a round core with a few scattered outliers
attached; check a "reachable" verdict with a connected-component replay
before trusting it, the way E43 did for Ferguson and Pier. And pre-register
a prediction, then run it — and when the run's own numbers disagree with
an earlier round's story, say so in plain language rather than filing it
away: this round's review caught two separate mistakes in the E30a
arrival-kernel work before Task 8 built a whole real-fire experiment on
top of them (a diagonal-step cost bug misdiagnosed as pure hull geometry,
and a wrong "closed form" target for the exponential law's own shape).
Both are now documented as controller errors, not swept under a later
number.

## Still open after this round

In order:

1. **The full five-seed E30b** on Arm B's configuration (4× clock,
   widened p0/burn_duration prior, the learned `wind_rot_deg` gene),
   plus **E37b** re-run at the same 4× clock — the pilot's own
   pre-registered stop rule did not trigger, so this is the recommended
   next run.
2. **The E30b mechanism ablations**: Arm B with mutation σ = 0 on
   `wind_rot_deg` (each member keeps its birth draw, never learns — tests
   whether angular *diversity* alone explains the gain), and Arm B with
   the gene's range narrowed to ±20° (tests whether a *learned*, tightly
   scoped correction explains it instead).
3. **Arrival + rear_focus + spotting genes (E43) together** — not yet
   combined; each was shown to add reach separately, and it is not known
   whether they are redundant (the same wind-aligned effect twice) or
   additive.
4. **A finer angular neighbourhood, or a fitted template-LB correction**,
   for the rear-focus hull overshoot that remains at high wind
   (LB > 1.5) — not needed for these six fires' own wind speeds, but
   named and left unfixed in E30a.
5. **A fuel term in the containment operator, and the ICS-209 check's own
   follow-up** (E42 raised, did not resolve, whether ICS-209's
   containment-line lag can be measured against fire size or fuel type)
   — carried from Round 4/5.
6. **Pre-existing gaps, unresolved this round**: crate coverage measures
   ≈ 98.3 %, below the 99 % memory gate; `make clippy` (the root command)
   does not lint `cella_lib`'s own examples, including the files this
   round changed most (`wildfire_smc.rs`, `wildfire_ros.rs`); and
   `wildfire_smc.rs` itself has grown large enough (~1,300+ lines) to be
   worth splitting into modules before the next round adds to it.

## Configuration after this round

**Unchanged** from Round 5: Bernoulli spread rule, 32 members, β 10, σ
0.2, immigrants 0.2, containment-only stopping, E25's broad prior. This is
still the recommended production configuration. Five-seed means (E33):
Bear 0.479, Brattain 0.416, Buck 0.590, Chimney 0.434, Ferguson 0.344,
Pier 0.535.

**New, opt-in, all off by default**: `spread: arrival` with
`wind_law: rear_focus` (validated on a flat grid for LB ≤ 1.5, E30a; but
**REJECTED as tested** on the six real fires without a re-tuned p0 range,
E30 — do not switch this on for a real forecast yet); the `wind_rot_deg`
gene (promising in a one-seed pilot, E30b, not yet a five-seed result);
`SMC_STEPS_SCALE` (the 4× clock the E30b pilot used); `immigrant_reset_gate`
(E39, tested, safe, but inert for its intended job); `state_correction:
Immigrants` or `All` (E40/E40b, kept as engine options, explicitly **not**
recommended as a nowcast — see finding 4 above); the lagged nulls
(`lagged_persistence_iou`, `lagged_circle_iou`) now computed in every
`assim` report; the Ellipse null (`ellipse_iou`, `brier_ellipse`) now
computed in every `open`/`assim` report; and the `binary_git` /
`binary_built_utc` provenance stamp in every report.

**The E30b Arm B pilot configuration** (arrival, rear_focus, 4× clock,
the `arrival_x4.json` prior, `wind_rot_deg` ±90°) is a **candidate** for a
future production configuration, not the recommendation of this round —
it rests on one seed and a noise floor borrowed from a different
configuration. The full five-seed E30b (item 1 above) is what would
promote it.
