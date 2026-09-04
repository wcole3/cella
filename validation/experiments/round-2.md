# Round 2 — 2026-09-01: is the wind right, and why is the model too round?


Trigger: a GUI tester saw "wind direction 0°" push the fire **east** and
expected north-to-south (the weather-report convention). Before touching
parameters again, Round 2 audits the wind path end to end against the
observed fires, then asks what actually limits the shape of the burn.

**Tooling added:** `EXP_WIND_ROT_DEG` hook in `wildfire_experiment.rs`;
runners `exp_wind_rotation.py` (E9), `exp_wind_oracle.py` (E10),
`exp_timeres.py` (E11). Shape/speed diagnostics are one-off scripts whose
outputs are kept in `results/experiments/` (`obs_front_speed.json`,
`exp12_shape.json`).

**Gotcha that cost one run:** the repo root and `cella_lib/` are separate
cargo build roots. `cargo run` inside `cella_lib/` writes to
`cella_lib/target/`; a stale `target/release/examples/wildfire_experiment`
at the repo root silently ran without the new hook (every rotation scored
identically — the tell). Runners now point at `cella_lib/target/`.

## Round 2 conclusions → what to do next


1. **No wind bug, but a home-made convention.** "Toward, 0° = +x" was
   ours alone (Alexandridis defines only a relative angle; PyTorchFire
   uses toward/east/counter-clockwise; every weather source and
   operational simulator uses the bearing the wind comes *from*). Done
   the same day: the parameter is now `wind_from_deg` in that weather
   convention everywhere — model field, panel, configs, converter,
   scenario format v2. Old `wind_dir_deg` files are rejected on load.
   Every number in this log is unchanged: the conversion is exact
   (`toward = from + 90°`) and the six-fire reports were re-run to
   confirm.
2. **Wind input, not wind math, is the weak link.** ERA5 daily domain
   means are too weak to act (E9b) and wrong on Chimney's run days (E9c).
   Codified as TEST_PLAN §2.1 + process rule 6: every source's wind
   convention, units, height and averaging get inspected and written into
   `provenance.weather` before any comparison is quoted.
   Before any more wind work, get hourly ERA5 (or a station record) into
   the scenario schedule and re-run E9b: if the rotation effect vanishes
   and Chimney improves at ×1, the kernel is vindicated.
3. **Stop scanning resolution.** E6, E11, E11b, E13 all say the same
   thing: p0 × steps is one knob. Do not spend more runs on steps/day.
4. **Time-varying drivers are the lever.** The temperature schedule is
   now a 3-seed-verified win on the fire it fits (Buck). Promote it into
   `WildfireModel` as a proper input (roadmap §10.4) and add a
   higher-range driver (hourly wind speed and RH → fuel-moisture proxy)
   so fast days and stopped days both become representable.
5. **Shape diagnostics stay in the harness.** Elongation and daily
   growth direction (E12) caught what IoU hides (Brattain: 2.7 vs 1.05).
   Worth adding to the report JSON so every run carries them.
6. **Fix the two build roots trap** in the runner scripts (done for the
   new ones) and in the older `exp_*.py`, which use `cargo run` from
   `cella_lib/` and are therefore safe — but only by accident.
