# Wildfire model — experiment log

One file per experiment under this folder. Each has: what we tried, why,
the numbers, and the verdict. Failed experiments stay (test plan §8 rule 5).
Start with the table; open a file only when you need its numbers.

**Ground rules** (from [../TEST_PLAN.md](../TEST_PLAN.md)):

- Calibration fires only: Bear, Brattain, Buck, Chimney. Holdout (Ferguson, Pier) never touched.
- "Mean IoU" = mean overlap over the observation series, t = 0 excluded. Higher is better.
- Every number sits next to the Circle (area-matched radial null) on the same fire — beating it is the bar.
- Scan grade = 1 seed; anything promoted gets 3 seeds before it is believed.
- Weather inputs get inspected before any comparison (TEST_PLAN §2.1).

**Tooling**: `cella_lib/examples/wildfire_experiment.rs` (hooks `EXP_P0_SCALE`, `EXP_WIND_SCALE`, `EXP_WIND_ROT_DEG`, `EXP_SEED_BASE`) and the runners in `../scripts/experiments/`. Results land in the gitignored `../results/experiments/`. **Build root trap:** runners must call `cella_lib/target/release/examples/...`, not the repo-root `target/`.

Research notes: [why fires slow down, what wind a fire feels, what suppression does](research-decline-wind-suppression.md) (2026-09-04) — sources behind E26–E30.

| # | Experiment | Verdict | Fires | Key number | Round |
|---|---|---|---|---|---|
| — | **Round 1 — 2026-08-15** — intro, verified results, conclusions | | | | [round-1.md](round-1.md) |
| E1 | [p0 × burn_duration scan](01-e1-p0-burn-duration-scan.md) | KEPT (the big lever) | Bear, Brattain, Buck, Chimney | best per-fire mean IoU 0.32–0.44; global 0.334 | Round 1 |
| E2 | [canopy-cover density layer](02-e2-canopy-cover-density-layer.md) | REJECTED | Bear, Brattain, Buck, Chimney | −0.045 Buck, +0.018 Bear | Round 1 |
| E3 | [weather-driven daily p0 schedule](03-e3-weather-driven-daily-p0-schedule.md) | KEPT (first physics win) | Bear, Brattain, Buck, Chimney | Buck +0.05 (temp); rain collapses Bear ×3.3→×0.3 | Round 1 |
| E4 | [wider veg_factor spread](04-e4-wider-veg-factor-spread.md) | REJECTED | Bear, Brattain, Buck, Chimney | Bear −0.061, Buck −0.064 | Round 1 |
| E5 | [wind gust multiplier ×2 / ×4](05-e5-wind-gust-multiplier-2-4.md) | REJECTED | Bear, Brattain, Buck, Chimney | Chimney +0.013 at ×2, Bear −0.04 at ×4 | Round 1 |
| E6 | [4× time resolution](06-e6-4-time-resolution.md) | REJECTED (as tested) | Bear, Brattain, Buck, Chimney | Bear −0.05 (p0/4 too crude) | Round 1 |
| E7 | [spotting on](07-e7-spotting-on.md) | REJECTED | Bear, Brattain, Buck, Chimney | −0.1 to −0.2 everywhere | Round 1 |
| E8 | [ensemble burn-probability threshold](08-e8-ensemble-burn-probability-threshold.md) | finding, not a lever | Bear, Brattain, Buck, Chimney | union of seeds best; halo is deterministic | Round 1 |
| — | **Round 2 — 2026-09-01: is the wind right, and why is the model too round?** — intro, verified results, conclusions | | | | [round-2.md](round-2.md) |
| E9a | [wind convention audit (code + converter + raster)](09-e9a-wind-convention-audit-code-converter-raster.md) | NO BUG | Bear, Brattain, Buck, Chimney | cos +0.96 row 0 = north; no bug | Round 2 |
| E9b | [rotate the whole wind schedule](10-e9b-rotate-the-whole-wind-schedule.md) | finding, not a lever | Bear, Brattain, Buck, Chimney | rotations ±0.02 at ERA5 speed; Chimney 0.571 at ×5 rot 270 | Round 2 |
| E9c | [is Chimney's 270° a lucky angle?](11-e9c-is-chimney-s-270-a-lucky-angle.md) | NO (robust plateau) | Bear, Brattain, Buck, Chimney | 0.50–0.57 plateau for rot 225–300 | Round 2 |
| E10 | [wind-direction ORACLE (upper bound, cheats)](12-e10-wind-direction-oracle-upper-bound-cheats.md) | finding | Bear, Brattain, Buck, Chimney | oracle ≤ +0.08 (Chimney); hurts Bear/Buck | Round 2 |
| — | [Observed front speed vs the model's hard cap](13-observed-front-speed-vs-the-model-s-hard-cap.md) | finding (data) | all six (data only) |  | Round 2 |
| E12 | [shape: how round is the model?](14-e12-shape-how-round-is-the-model.md) | finding | Bear, Brattain, Buck, Chimney | Brattain elongation 2.70 truth vs 1.05 model | Round 2 |
| E11/E11b | [joint (steps/day, p0, burn_duration) scan](15-e11-e11b-joint-steps-day-p0-burn-duration-scan.md) | REJECTED (cap is not the binding limit) | Bear, Brattain, Buck, Chimney | ±0.03 for 50→400 steps/day; best p0 ∝ 1/steps | Round 2 |
| E13 | [daily temperature schedule × tick rate](16-e13-daily-temperature-schedule-tick-rate.md) | schedule KEPT, rate REJECTED | Bear, Brattain, Buck, Chimney | Buck +0.06 (0.485) at either rate | Round 2 |
| — | **Round 3 — 2026-09-02: real weather in, explosion out** — intro, engine fix (`set_p0`), conclusions | | | | [round-3.md](round-3.md) |
| E14 | [hourly station wind (NOAA ISD) instead of ERA5 daily](17-e14-hourly-station-wind.md) | REJECTED as drop-in; loader KEPT | Bear, Brattain, Buck, Chimney | flat Chimney, −0.02…−0.04 elsewhere; stations 41–66 km away | Round 3 |
| E15 | [hourly fuel-moisture damping from station RH/T](18-e15-hourly-fuel-moisture-damping.md) | REJECTED (as tested) | Bear, Brattain, Buck, Chimney | damped fire dies (×0.1) or, re-tuned, explodes again | Round 3 |
| E16 | [heterogeneity (a) and containment decay (b)](19-e16-heterogeneity-and-containment-decay.md) | (a) REJECTED, (b) KEPT | Bear, Brattain, Buck, Chimney | τ5 ×2: Bear +0.12, Buck +0.14, area ×0.7–1.0 | Round 3 |
| E16c | [containment decay as one global recipe, all six fires](20-e16c-containment-decay-global-recipe-holdout.md) | KEPT (suppression proxy) | all six incl. holdout | Pier (holdout) 0.32→0.46; Ferguson flat (under-burns) | Round 3 |
| E19 | [what is one tick in real time? model rate of spread](21-e19-what-is-one-tick-in-real-time.md) | finding (data) | synthetic grid | wind changes speed 5–10 %; E1 recipes need 100–210 ticks/day on run days | Round 3 |
| E17 | [moisture × containment decay](22-e17-moisture-times-decay.md) | KEPT (physics retained, no cost) | Bear, Brattain, Buck, Chimney | equals decay-alone on mean IoU, +0.01…+0.09 final IoU | Round 3 |
| E18 | [dynamic fire-line agent](23-e18-dynamic-fire-line-agent.md) | REJECTED as tested; direction KEPT | Bear, Brattain, Buck, Chimney | ring closes day 2 (×0.1) or never (×3.6); no middle | Round 3 |
| E20 | [evolutionary per-fire calibration + transfer](24-e20-evolutionary-per-fire-calibration-and-transfer.md) | finding: τ, dur, wind× transfer; fuel ratio does not | all six incl. holdout | transfer recipe: Pier (holdout) 0.32→0.51; Bear 0.45 | Round 3 |
| E21 | [observed percent-contained (ICS-209) replaces the decay](25-e21-observed-containment-replaces-decay.md) | REJECTED as p0 schedule; data KEPT; decay relabelled | Bear, Brattain, Buck, Chimney | real containment 0–13 % by day 4 → area ×3–8 vs decay ×0.7–1.0 | Round 3 |
| E22 | [rate-of-spread-driven clock (normalised / un-normalised)](27-e22-rate-of-spread-driven-clock.md) | REJECTED at daily truth | Bear, Brattain, Buck, Chimney | normalised ±0.02; extra ticks −0.08…−0.18 | Round 3 |
| E23 | [ramped, breachable, anchor-and-flank line agent](26-e23-ramped-breachable-line-agent.md) | REJECTED (parked) | Bear, Brattain, Buck, Chimney | perfect line strangles, veg 0.1 line ignored; Bear ×0.9 area but IoU 0.27 < 0.31 | Round 3 |
| — | **Round 4 — 2026-09-04: ensembles** — Monte Carlo maps and a fire that learns as it burns | | | | [round-4.md](round-4.md) |
| E24 | [Monte Carlo burn probability from one untuned prior](28-e24-monte-carlo-burn-probability.md) | KEPT (default product) | all six | consensus ties/beats tuned single runs; Brier 40–60 % below Circle on 3 fires | Round 4 |
| E25 | [particle filter with GA operators (learns as it burns)](29-e25-generational-ensemble-particle-filter.md) | KEPT (headline mode) | all six incl. holdout | forecast IoU +0.05…+0.21 over open; Ferguson 0.13→0.34; Bear beats Circle days 2–4 | Round 4 |
| E26 | [terrain-adjusted wind field (mass-consistent downscaling)](30-e26-terrain-wind-field.md) | NULL at this kernel; infrastructure KEPT | Bear, Brattain, Buck, Chimney | ±0.01 at ×1, ±0.02 at ×3; run cost 6 s | Round 4 |
| E27 | [painted retardant multiplier that dries out](31-e27-painted-retardant-multiplier.md) | REJECTED (tactics, not material) | Bear, Brattain, Buck, Chimney | 0.02 = fence, 0.1 = nothing; recovery time irrelevant | Round 4 |
| E28 | [containment-probability operator replaces the decay](32-e28-containment-probability-operator.md) | KEPT (recommended over τ) | all six incl. holdout | containment-only: Bear 0.482 vs decay 0.456; others within noise | Round 4 |
| E31 | [replicate E25/E28 through the generic `explore` engine](33-e31-generic-engine-replication.md) | PASS (replication); E28 Bear gain = noise | all six incl. holdout | containment-only within 0.009 IoU of record on every fire; containment vs decay a tie everywhere | Round 4 |
| — | **Round 5 — 2026-09-05: the methods themselves** — noise floor, ensemble size, operators, priors, offline GA vs filter, illumination | | | | [round-5.md](round-5.md) |
| E33 | [noise floor: five seeds of the recommended ensemble](34-e33-noise-floor.md) | finding (the bar for the round) | all six incl. holdout | sd ≤ 0.015 on five fires, 0.039 on Buck (one seed locks in contained) | Round 5 |
| E32 | [ensemble size 8–128](35-e32-ensemble-size.md) | finding: keep 32; 64–128 for Brier | all six incl. holdout | IoU flat past 32 on five fires; 8 members −0.03…−0.07; Brier keeps improving to 128 | Round 5 |
| E34 | [operator ablation: immigrants, σ, β, crossover](36-e34-operator-ablation.md) | finding: plateau; keep defaults | all six incl. holdout | 38 of 54 cells ties; all runs end 100 % contained; crossover 0.5 +0.027 Bear (one seed) | Round 5 |
| E35 | [prior width: narrow / broad / very broad](37-e35-prior-width.md) | finding: never narrow the prior | all six incl. holdout | narrow prior −0.037 on Bear; very broad a tie on five fires, better Brier on four | Round 5 |
| E36 | [fit the first three days with a GA, then forecast](38-e36-offline-fit-versus-filter.md) | finding: the filter wins everywhere | all six incl. holdout | filter beats the fitted genome by 0.025–0.104 on forecast days; fits hit the box edges | Round 5 |
| E37 | [illuminate the fire model: growth × elongation reachability](39-e37-illuminate-the-fire-model.md) | finding: wedge; 3 fires unreachable | all six incl. holdout | model elongated only while small; Brattain/Ferguson/Pier shapes outside the reachable set; wind = speed knob not shape knob | Round 5 |
| E38 | [immigrants start with a fresh driver state](40-e38-immigrant-reset.md) | KEPT as option (default off) | all six incl. holdout, 5 seeds | Buck seed 3 +0.105, its sd 0.039 → 0.021; ties elsewhere; Pier −0.008, Brier +0.003…0.006 | Round 5 |

