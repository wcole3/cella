# E48 — why Brattain fails under arrival without the gene

_Round 7 (2026-09-23) · read-mostly diagnostic: seed 0, Brattain only, two
arms · new opt-in knob `SMC_DIAG=1` (per-window diagnostics, no existing
field changed) · runner `exp_r7_e48.py` → `exp48_brattain_arrival_diagnosis.json`
(+ raw `exp48_brattain_arrival_diagnosis/`) · compared against the E41
Ellipse null's own per-window series on Brattain
(`exp41_ellipse/Brattain_2020_nulls.json`, not re-run) · no score family,
this is a finding · pre-registered TEST_PLAN v1.9 · terms:
[GLOSSARY.md](GLOSSARY.md)_

**In short.** *(written after the run — see "Result" below.)*

**Question.** E30b's pilot (`48-e30b-uncapped-clock-direction-gene-pilot.md`)
found that Arm A (the arrival kernel, rear-focus wind law, 4x clock and
widened prior, with no learned wind-rotation gene) posted its single worst
result — 13.2 sd below E33 — on Brattain, the one fire E41 said had the
*right* ERA5 direction and was predicted to need the least help. Arm B
(the same plus the learned `wind_rot_deg` gene) reversed this completely,
+8.1 sd above E33. That pilot had no per-window visibility into why: it
only reported mean/final consensus IoU per fire. **Why does Brattain fail
under Arm A, and what is Arm B's gene actually correcting, window by
window?**

**What we changed.** A new opt-in knob, `SMC_DIAG=1`
(`cella_lib/examples/wildfire_smc/knobs.rs`), gates a new optional field,
`diag`, on `assim` mode's per-observation report row (`ObsScore`,
`cella_lib/examples/wildfire_smc/score.rs`) — `None`/omitted from the JSON
when the knob is unset, so every report from before this knob existed
(and every report from a run that doesn't set it) is byte-identical to
before. When set, each scored window's `diag` carries:

- the ERA5 wind vector for that window (speed, compass "from" bearing,
  and the grid "toward" bearing `wind_toward_grid_deg` converts it to —
  the same vector the Ellipse null and the driver's own `rear_focus` wind
  law use);
- the station vector mean for the same window, from `station_hourly.json`
  (`station_vector_mean`, already used by the Ellipse null's station
  variant), `None` where the field is absent (`station_speed_ms`/
  `station_toward_deg`, both present here — Brattain has a station log);
- the ensemble's per-window learned-gene medians: `model.p0`,
  `wind_scale`, and (Arm B only — absent, not zero, under Arm A, which has
  no such gene) `wind_rot_deg`. `Ensemble::genome_stats` already reports
  mean/sd/min/max per gene, but not a median, so a small new function
  (`crate::diag::median_gene`) reads it off `Ensemble::genomes()` instead;
- a head-vs-flank decomposition of the miss: every cell where the
  consensus mask (probability ≥ 0.5, the same threshold `consensus_iou`
  already scores) disagrees with the truth mask is classified by whether
  its displacement from the ignition centroid has a positive dot product
  with the window's ERA5 "toward" vector — "downwind" if so, "cross-wind"
  otherwise (this second bucket also catches ties and anything upwind;
  the pre-registered question only asks for two buckets). Four counts per
  window: `downwind_miss`, `crosswind_miss` (truth burned, consensus
  didn't call it), `downwind_false_positive`, `crosswind_false_positive`
  (consensus called it, truth didn't).

New module `cella_lib/examples/wildfire_smc/diag.rs` holds
`median_gene`, `centroid` and `head_flank_decompose`, each unit-tested;
`score.rs` carries a test that `diag: None` never serialises a `"diag"`
key. Wired into the scoring loop in
`cella_lib/examples/wildfire_smc/modes/open.rs` with about a dozen lines
(the station log is loaded once per run via a small refactor —
`crate::nulls::load_station`, factored out of `nulls::run_nulls` so both
call sites share the exact same `SMC_STATION_WIND`-or-`station_hourly.json`
lookup instead of duplicating it).

Two runs, seed 0, Brattain only: **Arm A** (arrival kernel, rear-focus
wind law, 4x clock, `arrival_x4.json` prior — Arm B's own preset minus
`SMC_WIND_ROT_GENE`) and **Arm B** (the same plus `SMC_WIND_ROT_GENE=90`),
both with `SMC_DIAG=1`, `assim` mode, 32 members, 2 workers, niced,
one batch, shared-machine rules per the Round 7 plan.

**Why we expected it to matter.** If Arm A's Brattain miss is concentrated
in a few windows rather than spread evenly across the whole run, that
would explain a large mean-IoU loss without contradicting "the daily-mean
ERA5 direction is right" (E41 scores the *whole* Ellipse series, which can
still average out a few bad windows). And if the windows Arm B's gene
diversity helps most on are the same windows the head/flank decomposition
flags as downwind-miss-heavy, that would be direct, window-level evidence
that the gene is doing more than adding noise — it is correcting the
input on exactly the days it is wrong, which the E30b pilot could only
infer indirectly from the small (8.6°–43.8°) learned median rotations at
the *end* of the run.

**How we scored it.** No score family — this experiment reads and reports
per-window diagnostics, it does not compute or compare an IoU/Brier
metric family against a noise floor. For each arm and each scored window:
`consensus_iou` (already reported, read here for context), the ERA5 and
station wind vectors, the learned p0/wind_scale (Arm B: + wind_rot_deg)
medians, and the four head/flank counts. The E41 Ellipse null's own
per-window `ellipse_era5_iou` series on Brattain (`ellipse_era5` variant —
ERA5 wind, unchanged from this campaign's default input) is read
alongside, not re-run, to see whether the null's own per-window skill
(rather than its 21-window mean) tracks the same windows.

**Prediction, written before the run.** ERA5 direction is right on the
daily mean but wrong on the two or three windows that carry most of the
burned area, and Arm B's gene diversity covers exactly those windows.

*(Result, "what it means", "questions this raises" and "verdict" are
appended below after the runs.)*
