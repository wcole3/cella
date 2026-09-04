# Ensembles: burn probability and a fire that learns as it burns

This page is for someone who has run one wildfire simulation and wants to
know why they should run thirty-two, and how. No statistics background
needed; the numbers you need to pick are all here with defaults.

## 1. Why one run is not enough

A single run is one roll of the dice with one guess at the parameters
(how flammable the fuel is, how long a cell burns, how fast the fire
slows down over the days, how much to trust the wind). Change the seed
and the map changes; change the guess and it changes more. Nobody knows
the right guess for a fire that is burning right now.

An **ensemble** runs many members at once, each with its own random seed
and its own parameters drawn from a *prior* — a range you are willing to
believe. Two useful things fall out:

- **A burn-probability map.** For every cell, the fraction of members in
  which it burned. "70 %" means "most futures we can imagine burn this
  cell". This is what operational systems (FSim, PROPAGATOR) publish, and
  in our validation it is far better *calibrated* than any single map:
  when it says 30 % it is right about 30 % of the time.
- **Learning from observations.** When a real perimeter comes in
  (yesterday's satellite mask, a drone flight), score every member
  against it, keep the ones that did well, nudge their parameters, and let
  them all keep burning from where they are. Tomorrow's map is then made
  by members that already resemble today's fire. In the validation set
  this raised forecast skill on every one of six fires, including two the
  method never saw during development (experiments E24/E25 in
  `validation/experiments/`).

**Grown-up words:** the first is Monte Carlo sampling of a parameter
prior; the second is a particle filter (sequential Monte Carlo) whose
proposal step uses genetic-algorithm operators — selection by fitness,
mutation, immigration.

## 2. Configure it

Add an `ensemble` block to a 2D config that already has a wildfire model:

```json
{
  "dim": "2d",
  "width": 200, "height": 200, "history_limit": 0,
  "initial": ["Forest", "..."],
  "rule": { "subrules": [] },
  "model": { "wildfire": { "params": { "...": "..." }, "env": { "...": "..." } } },
  "ensemble": {
    "members": 32,
    "seed": 0,
    "prior": {
      "p0": [0.08, 0.6],
      "burn_duration": [5, 20],
      "tau_days": [2.0, 100.0],
      "wind_scale": [0.0, 1.5]
    },
    "beta": 10.0,
    "sigma": 0.2,
    "immigrants": 0.2
  }
}
```

Every field has a default (the values above), so `"ensemble": {}` works.

| Field | What it is | How to choose |
|---|---|---|
| `members` | how many simulations run together | 32 gives a smooth probability map; cost and memory are linear in members. Members share the terrain tables, so 32 members of a 1 M-cell grid need roughly 32 × (cells × 8 bytes). |
| `seed` | fixes the prior draw and every member's dice | change it to get an independent ensemble; same seed = same result, always |
| `prior.p0` | range of the base ignition probability, drawn log-uniformly | the default spans "dies out" to "burns everything"; narrow it only if you know your fuels |
| `prior.burn_duration` | ticks a cell burns, drawn uniformly | 5–20 covers the validated range |
| `prior.tau_days` | containment time-scale: p0 is multiplied by e^(−t/τ) as the fire ages; values ≥ 150 mean "no decay" | keep the wide default; the filter learns the right one per fire (validated fires wanted 5–20 days) |
| `prior.wind_scale` | multiplier on the wind you feed in | 0–1.5 lets the ensemble decide how much to trust a distant weather station |
| `beta` | selection sharpness when learning: weights are e^(β·IoU) | 10 keeps a healthy spread (≈ 25 of 32 members survive a generation); 30 is greedy and over-confident |
| `sigma` | mutation size (log-normal on p0 and τ, ±2 ticks on duration) | 0.2; 0.05 stops exploring, 0.5 forgets what it learned |
| `immigrants` | share of each new generation re-drawn from the prior | 0.2; it stops the population collapsing onto one parameter set |
| `prior.containment` | optional `{ "a": [lo, hi], "b": [lo, hi] }`: once a day each member is *contained* (stops for good) with probability sigmoid(a + b·ln g), g = its growth that day | off by default; `{"a": [-6, -1], "b": [-2, -0.3]}` is the tested prior. This is how FSim stops simulated fires (slow growth → likely contained). Call `end_of_day()` once per simulated day. |

## 3. Use it from Rust

```rust
use cella_lib::config::CellaConfig;
use cella_lib::WildfireEnsemble;

let cfg: CellaConfig = serde_json::from_str(&std::fs::read_to_string("fire.json")?)?;
let mut ens = cfg.build_ensemble().expect("config has an ensemble block")?;

// One weather window: hours since ignition, wind speed (m/s) and the
// weather-report bearing the wind comes FROM (0° = north).
ens.set_weather(0.0, 4.0, 225.0);
ens.step_n(50);                       // 50 ticks = one day at the default clock

let p = ens.burn_probability();       // f32 per cell, 0..=1
let likely = ens.consensus(0.5);      // bool per cell: majority says burned

// An observed perimeter arrives (bool per cell). Score your forecast
// against it FIRST, then let the ensemble learn from it.
let observed: Vec<bool> = load_perimeter();
let report = ens.assimilate(&observed)?;
println!("effective members {:.1}, immigrants {}", report.effective_sample_size, report.immigrants);

ens.set_weather(24.0, 6.0, 240.0);
ens.step_n(50);                       // tomorrow's forecast, from members that match today
ens.end_of_day();                     // containment draw (no-op unless the prior enables it)
println!("{:.0}% of members contained", 100.0 * ens.contained_fraction());
```

`grids_mut()` gives you every member's grid between steps, so anything
you can do to one grid — paint a fire line, drop retardant with
`WildfireModel::set_density`, load a terrain wind with
`set_wind_field` — you can do to all members.

## 4. Use it from the command line

The validation example wraps the same type with scoring:

```bash
cd cella_lib
cargo run --release --example wildfire_smc -- \
    ../validation/data/scenarios/Bear_2020 32 assim out.json
```

`open` instead of `assim` gives the plain Monte Carlo map. Environment
variables `SMC_BETA`, `SMC_SIGMA`, `SMC_IMMIGRANTS`, `SMC_SEED`,
`SMC_ASSIM_EVERY` (learn only every k-th observation) and
`SMC_PRIOR=path.json` override the config. The report JSON has, per
observation time, the consensus IoU, mean member IoU, Brier score and
the ensemble's parameter means — the same fields the experiment logs
quote.

## 5. Reading the output honestly

- **Score forecasts, not fits.** Compare the map made *before* an
  observation with that observation. If you call `assimilate` first and
  score afterwards, you are grading the answer key.
- **Look at the Brier score next to a 0/1 baseline.** A confident wrong
  map scores badly; a 30 % that is right 30 % of the time scores well.
  The validation harness prints the area-matched Circle's Brier beside
  the ensemble's for exactly this reason.
- **Watch the effective sample size.** If it drops toward 1 the
  population has collapsed on a single member; raise `immigrants` or
  lower `beta`.
- **Parameters the filter learns are diagnostic, not truth.** They are
  the values that make *this* model track *this* fire; on the validated
  fires they drifted to physically plausible ranges (τ 5–20 days, wind
  × ≈ 1), which is reassuring, not proof.

## 6. Limits (September 2026)

- Wildfire only. The ensemble type knows the wildfire model's parameters
  by name; other external models would need their own prior/mutation
  description.
- The GUI does not show ensembles yet; it is a library and CLI feature.
- Members are full grid clones. The slope table is shared, everything
  else is per member; a shared-landscape member type would cut memory
  further.
- Late-fire stalls are still under-predicted (the model cannot stop in
  place), and fast wind-driven runs are still under-predicted (see the
  validation Round 3/4 conclusions).
