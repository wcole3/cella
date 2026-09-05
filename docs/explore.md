# Explore: ensembles, evolution and illumination for any rule or model

This page is for someone who has run one simulation and wants to know why
they should run thirty-two, or three thousand, and how. No statistics
background needed. Every number you have to pick is listed with a default
and a sentence on how to choose it.

Everything here works on **any** grid `cella` can load: a 1D Wolfram rule,
Life, a rule of your own, or a 2D grid with an external model such as the
wildfire model. Nothing in the `explore` module knows a model by name. The
wildfire model is used as the worked example because it is the one shipped
with the repo and the one with real data behind it (`validation/`).

New to the words *Monte Carlo*, *particle filter*, *genetic algorithm* or
*MAP-Elites*? Two short primers explain them from scratch:
[primer-monte-carlo.md](primer-monte-carlo.md) and
[primer-genetic-algorithms.md](primer-genetic-algorithms.md).

## 1. Why one run is not enough

A single run is one roll of the dice with one guess at the knobs. Change
the seed and the picture changes; change a knob and it changes more. Two
questions a single run cannot answer:

- **"How sure should I be?"** Where does the fire *usually* reach? Which
  cells of Life are alive in most futures? Answer: run many copies and
  count. That is an **ensemble**.
- **"Which knobs make it do X?"** Which birth count gives 30 % coverage?
  Which Wolfram code sorts a random row into all-ones or all-zeros? Answer:
  keep a population of knob settings, score them, breed the good ones.
  That is **evolution** (a genetic algorithm).

A third question turns out to be the most fun one:

- **"What *can* this rule family do?"** Not the single best setting, but
  the whole range of behaviours. Answer: the same evolution, but instead of
  keeping the fittest, keep the best example of every *kind* of behaviour.
  That is **illumination** (MAP-Elites) or **novelty search**.

## 2. Which one do I want?

| You want… | Use | Config block | GUI |
|---|---|---|---|
| a probability per cell, "where does it usually go" | Monte Carlo ensemble | `"ensemble"` | Explore → Monte Carlo → Start |
| the ensemble to learn from something you observed (a painted mask, a real perimeter) | ensemble + assimilation | `"ensemble"` + `assimilate` | Explore → Monte Carlo → Learn from grid |
| the knobs that score best on one measurement | evolution, objective search | `"evolve"` | Explore → Evolve → Start |
| a map of every different behaviour the knobs can produce | MAP-Elites | `"evolve"` with `"search": {"map_elites": …}` | Explore → Evolve → Search: MAP-Elites |
| to be surprised: behaviours unlike any seen so far | novelty search | `"evolve"` with `"search": {"novelty": …}` | Explore → Evolve → Search: Novelty |

All of them share three ingredients: **genes** (which knobs vary, §4),
**metrics** (how a run is measured, §5), and a **seed** (§3). Read those
three sections once; the rest is picking numbers.

## 3. The `seed` field

Every config now has a top-level `"seed"` (default 0). Two runs with the
same config and the same seed produce the *same cells*, step for step, on
one thread or eight. This includes rules with `randomness` and the wildfire
model: randomness is drawn from a counter (`seed`, step, cell index, stream)
rather than from a shared random generator, so no thread can "steal" another
thread's draw.

Ensembles and evolutions have their own `seed` inside their block. It
decides which knob values are drawn and which per-member seeds are used, so
"same config, same seed, same report" holds for them too.

If you want a different picture, change the seed. If you want the same
picture on another machine, do not.

## 4. Pick genes: which knobs may vary

A **gene** is one knob the search is allowed to turn. Genes are listed under
`"genes"` in both blocks, one object each, and the key names the knob:

| Key | What it is | Kind |
|---|---|---|
| `rule.subrules[i].count` | the neighbour threshold of 2D subrule `i` | integer, 0 … neighbourhood size |
| `rule.subrules[i].limit` | its upper bound (only when the subrule has one) | integer, 0 … neighbourhood size |
| `rule.subrules[i].range` | its neighbourhood radius | integer 1 … 8 |
| `rule.subrules[i].op` | `lt`, `gt` or `eq` | choice |
| `rule.subrules[i].neighborhood` | `Moore`, `VonNeumann`, `Langton`, `StraightLine`, `Knight` | choice |
| `rule.subrules[i].randomness` | skip probability (only when the subrule has one) | float 0 … 1 |
| `rule.subrules[i].wolfram_code` | the 1D transition table | bits, `2^(2n+1)` of them |
| `model.<key>` | any parameter the external model describes (`model.p0`, `model.wind_speed`, …) | whatever the model says |
| anything without a prefix | a **free gene** that only a driver reads (§13) | the driver says |

`rule.subrules[*].field` writes the same value into every subrule. Use it
for the 1D presets, whose two subrules share one Wolfram code, so the
fast bit-packed path keeps working.

Each gene may narrow its range. Leave the range out and the gene inherits
the knob's declared bounds:

```json
{"key": "model.p0", "range": [0.08, 0.6], "scale": "log"}
{"key": "rule.subrules[1].limit", "range": [2, 8]}
{"key": "rule.subrules[*].wolfram_code", "bits": 128, "sigma": 0.01}
{"key": "rule.subrules[2].op", "choices": ["eq", "gt"]}
{"key": "wind_scale", "range": [0.0, 1.5]}
```

| Field | Default | How to choose |
|---|---|---|
| `range` | the knob's bounds | Narrow it to what you would believe. A `p0` of 0.9 is not a fire, it is a paint bucket. Integers need whole numbers. |
| `scale` | `"linear"` | `"log"` when the range spans orders of magnitude (0.001 … 0.5): every decade then gets equal attention. Needs a positive low end. |
| `bits` | the table's width | Only for `wolfram_code`; must equal `2^(2n+1)` for the subrule's radius. |
| `choices` | all options | A subset of the knob's options. |
| `sigma` | the block's `sigma` | Per-gene mutation size when one knob should move more or less than the others. |

**How the operators treat each kind.** *Sample*: uniform in the range (or
uniform per decade for log), uniform over options, each bit a coin flip.
*Mutate* with size σ: a float moves by `σ × (hi − lo) × N(0, 1)` (log:
multiplied by `e^(σ N)`), an integer by the same amount rounded, a choice
is redrawn with probability σ, a bool flips with probability σ, and each
bit flips with probability `min(1, 8σ / bits)`, which at the default σ 0.2
flips one or two bits of a 128-bit table. Everything is clamped back into
range. *Crossover*: each gene from one parent or the other at random; a bit
string is cut at one point instead.

**The repair rule.** Turning knobs independently can produce a subrule that
does not make sense: `limit` set while `op` became `eq`, or `limit` below
`count` with `op = gt`. When a genome is written, `limit` is dropped for
`eq`, raised to `count` for `gt`, and lowered to `count` for `lt`. Anything
else the rule's own `validate()` refuses scores as the worst possible and
is counted in the report's `invalid` column. One more thing to know: a
`count` is checked against the neighbourhood *as it is when written*, so a
genome that also changes `range` or `neighborhood` can leave `count` above
the new neighbourhood size. That is legal (the subrule just never fires);
the next mutation clamps it.

**What is refused, and why** (one line each in the error):

| You wrote | The error says |
|---|---|
| a key nothing declares | `unknown key`, with the list of known keys |
| a range outside the knob's bounds | `range … outside declared bounds` |
| a read-only knob (the wildfire seed) | `read-only` |
| `bits` on a knob that is not a bit table, or the wrong width | `bits` mismatch |
| a `choices` entry the knob does not offer | `illegal choice` |
| a free gene with no driver, or one the driver does not know | `free gene … needs a driver` |
| `lo > hi`, or `log` with `lo ≤ 0` | `lo > hi` / `log needs lo > 0` |
| the same key twice | `duplicate key` |

## 5. Pick an objective: how a run is measured

A **metric** turns one simulation into one number. An **objective** is a
metric plus *when* to measure it and *which way is good*:

```json
"objective": {"metric": "fraction", "types": ["Alive"], "when": {"step": 100}, "goal": {"target": 0.3}}
```

| Field | Default | Meaning |
|---|---|---|
| `metric` | required | one of the names below |
| `types` | — | the cell types the metric counts (`fraction`, `target_mask`, `series`, and the shape metrics) |
| `when` | `"end"` | `"end"` (after the last step), `{"step": k}` (at step `k`), or `"mean"` (averaged over every step) |
| `goal` | `"maximise"` | `"maximise"`, `"minimise"`, or `{"target": v}` (score is `−|value − v|`, so 0 is perfect) |

The metrics, and when to reach for each:

- **`fraction`** — share of cells in `types`. *Use it when* you want "about
  30 % alive" or "as much fire as possible"; the simplest thing to tune.
- **`activity`** — share of cells that changed on the last step (1.0 at
  step 0). *Use it when* you want something that keeps moving, or a
  still-life (`minimise`).
- **`entropy`** — Shannon entropy of the type mix, in bits. *Use it when*
  you want a balanced mixture rather than one type winning.
- **`lifetime`** — steps until the grid stopped changing (the run length
  if it never did). *Use it when* you want long transients; this is the
  Wolfram "how long until boring" measure. Ignores `when`.
- **`target_mask`** — how well the cells in `types` match a 0/1 mask you
  supply (`"score": "iou"` (default), `"sorensen"`, or `"agreement"`). *Use
  it when* you have an observed picture: a satellite perimeter, or the grid
  you painted in the GUI ("Match the current grid").
- **`series`** — root-mean-square error between the fraction of `types`
  per step and a `target` list. *Use it when* you know the curve, not just
  the end state.
- **`density_classification`** — the classic 1D task: start from a random
  row at a random density, and score 1 if the row ends all-majority. Draws
  its own initial rows, fresh every generation. *Use it when* you want to
  evolve a rule that *computes* something.
- **`bbox_fraction`**, **`elongation`**, **`centroid_speed`**, **`growth`**,
  **`period`** — shape measures described in §6. They also work as objectives
  (`centroid_speed` maximised finds gliders).

A metric can be measured every step (`"when": "mean"`, `series`,
`centroid_speed`, `period`) or once. Measuring every step costs a scan of
the grid per step; on a 256×256 grid that doubles the run time, so leave
`when` at `"end"` unless the curve matters.

**Rust users** are not limited to this list: anything implementing
`explore::Fitness` (`sample`, `every_step`, `score`) can be passed to
`Evolution::with_fitness`.

## 6. Descriptors: what makes two rules different

Illumination and novelty need a way to say "these two behaviours are
different" without saying which is better. A **descriptor** is a list of
one to three metrics, each with a range and a number of bins:

```json
"descriptors": [
  {"metric": "activity", "when": "mean", "bins": 20},
  {"metric": "entropy",  "when": "end",  "bins": 20}
]
```

| Field | Default | How to choose |
|---|---|---|
| `metric` | required | any metric except `target_mask`, `series`, `density_classification` (those measure closeness to a target, not a behaviour) |
| `when` | `"end"` | `"mean"` for "what it did on average", `"end"` for "where it ended up" |
| `range` | the metric's natural range | set it when you know the interesting band (activity rarely exceeds 0.5 in Life-likes) |
| `bins` | 20 | more bins = finer map, more cells to fill; 10–20 per axis is the useful band, 3 axes at 20 bins is 8000 cells |

The shape metrics exist for this:

- **`bbox_fraction`** — bounding box of the tracked cells over grid area.
  Small = a compact object; 1 = it reached every edge.
- **`elongation`** — how stretched the tracked cells are: `√(λ₁/λ₂)` of
  their second-moment matrix, 1 for a blob, 2 for something twice as long
  as it is wide, whichever way it points (clamped to 10). The validation
  log's E12 shape measure.
- **`centroid_speed`** — how far the centre of mass moved per step,
  averaged over the run (cells/step, clamped to 1). Non-zero = something
  travels: gliders, wind-driven fires.
- **`growth`** — final fraction minus initial fraction, −1 … 1. Positive
  = expanding, negative = dying out.
- **`period`** — the smallest cycle length in the last `window` states (0
  when none repeats). Oscillators show up here.

The classic picture: **activity × entropy** separates Wolfram's four
classes. Class 1 (dies) sits at low activity, low entropy; class 2
(periodic) at low activity, medium entropy; class 3 (chaos) at high
activity, high entropy; class 4 (complex) in between with long lifetime.
`configs/2d_map_elites_life_classes.json` draws that map for Life-like
rules.

## 7. Configure an ensemble

```json
"ensemble": {
  "members": 32, "seed": 0,
  "genes": [ … ],
  "track": ["Burning", "BurnedOut"],
  "beta": 10.0, "sigma": 0.2, "immigrants": 0.2,
  "driver": {"wildfire": {"steps_per_day": 50}}
}
```

| Field | Default | How to choose |
|---|---|---|
| `members` | 32 | Each member is a full copy of the grid. 32 gives a probability map in steps of 3 %; 128 is smooth. Memory is roughly members × cells × 16 bytes plus the history. |
| `seed` | 0 | Change for a different draw; keep for a reproducible one. |
| `genes` | none | §4. With no genes, members differ only by seed, which is still useful for a rule with `randomness` or a stochastic model. |
| `track` | every declared type except Inactive | The types the probability map counts. For a fire: burning and burned. For Life: alive. |
| `beta` | 10 | How sharply learning favours good members: weight = `e^(β × (score − best))`. 10 is gentle; 30 collapses the population onto a couple of members (experiment E24 shows it). |
| `sigma` | 0.2 | Mutation size after learning (§4). |
| `immigrants` | 0.2 | Share of the population re-drawn from scratch after each learning step. Keeps diversity: with 0 the population can converge on one wrong idea and never recover. |
| `crossover` | 0 | Chance a resampled child takes each gene from either of two parents before mutation. Off by default: the classic particle filter copies one parent. Experiment E34 measures whether it helps. |
| `immigrant_reset` | false | Give immigrants a fresh driver state instead of their parent's. For the wildfire driver that means an immigrant is uncontained and takes `p0` from its own genome, so a population in which every member has stopped can start again (E33 found the lock-in; E38 tests the fix). Needs the `model.p0` gene. |
| `driver` | none | A model-specific helper (§13); the wildfire one applies wind schedules and decides when a member is contained. Leave it out for rules. |

What you get back (see the CLI report and the Rust API in §11):

- **State probability** per cell: the share of members whose cell is in
  a tracked type. This is the map.
- **Consensus mask**: probability ≥ threshold, the map as a yes/no picture.
- **Metric mean ± sd** over members, and **genome statistics** (mean, sd,
  min, max of each gene) which tell you what the population believes.

**Learning from an observation** (`assimilate`): give the ensemble a 0/1
mask of what you actually saw (which cells burned; which cells you painted
alive). Every member is scored by IoU against it, the weights above decide
who breeds, and the population is resampled: good members are copied,
their copies mutated, `immigrants` fresh members added, and everyone keeps
simulating *from where they are*. The report gives the **effective sample
size**, `(Σw)² / Σw²`: how many members really carried weight. Near 1 means
collapse; raise `immigrants` or lower `beta`.

## 8. Configure evolution

```json
"evolve": {
  "population": 24, "generations": 30, "seed": 0,
  "genes": [ … ],
  "objective": { … },
  "steps": 100, "repeats": 3,
  "elite": 2, "crossover": 0.5, "mutation": 0.3, "sigma": 0.2, "immigrants": 0.1,
  "selection": {"tournament": {"k": 3}},
  "initial": "fixed"
}
```

| Field | Default | How to choose |
|---|---|---|
| `population` | 24 | More = broader search per generation, more cost. 24–48 is plenty for a handful of genes; the 1D 128-bit table wants 40+. |
| `generations` | 30 | When the best score stops moving for ten generations you are done. |
| `steps` | 100 | How long each candidate is simulated. Long enough for the behaviour you score to appear. |
| `repeats` | 3 | Seeds averaged per candidate. 1 for a deterministic rule with a fixed start; 3–5 for anything random; 20 for `density_classification`, where each repeat is a new random row. |
| `elite` | 2 | Best genomes copied unchanged into the next generation, so the best never gets worse. |
| `crossover` | 0.5 | Chance a child has two parents rather than being a copy of one. |
| `mutation` | 0.3 | Chance *each gene* of a child is nudged. |
| `sigma` | 0.2 | Size of a nudge (§4). |
| `immigrants` | 0.1 | Share of fresh random genomes per generation. |
| `selection` | tournament of 3 | `{"tournament": {"k": n}}` picks the best of `n` random genomes; `{"boltzmann": {"beta": b}}` weights by `e^(β score)`. Tournament is robust to the scale of the score; use it. |
| `initial` | `"fixed"` | `"fixed"` starts every run from the config's `initial` cells; `{"random": {"types": [...], "weights": [...]}}` draws a fresh random grid per generation (the same one for every candidate in that generation, so they compete fairly). |
| `search` | `"objective"` | §9 and §10. |
| `descriptors` | none | §6; required for `map_elites` and `novelty`. |
| `thumbnails` | true | Keep a small picture of each elite for the GUI gallery and `EXPLORE_THUMBS=1`. |
| `driver`, `forcing` | none | §13. |

**Cost.** One generation costs `population × repeats × steps` grid steps.
The defaults on a 64×64 grid are 24 × 3 × 100 = 7 200 steps per generation,
a couple of seconds; on 256×256 with `"when": "mean"` it is minutes. Scale
the grid down first. Small grids evolve rules that overfit to the small
grid (a glider that fits in 8×8 may crash into itself at 64×64); check the
winner at the real size before believing it.

**The loop, one generation.** Evaluate every genome (in parallel, results
kept in population order so the report is thread-count independent) →
report best/mean/sd and update the hall of fame (top five distinct genomes
ever seen) → copy the `elite` → add `immigrants` → fill the rest with
children: select two parents, cross them with probability `crossover`,
nudge each gene with probability `mutation`.

## 9. Illuminate instead of optimise (MAP-Elites)

```json
"search": {"map_elites": {"batch": 32, "iso_line": true}},
"objective": {"metric": "lifetime", "goal": "maximise"},
"descriptors": [ … two or three … ]
```

The **archive** is a grid with one cell per combination of descriptor bins.
Each cell keeps the single best genome (by the objective) whose behaviour
landed in it. Every generation: pick `batch` random elites, make one child
each, run them, and put each child in its cell if that cell is empty or the
child scores higher. The population *is* the archive.

| Field | Default | How to choose |
|---|---|---|
| `batch` | 32 | Children per generation. |
| `iso_line` | true | Children are made from *two* elites: a Gaussian nudge plus a step along the line between them. This follows the archive's shape; plain mutation (`false`) is fine for one or two genes. |
| `objective` | optional | Leave it out and every elite scores 0: pure **illumination**, "show me one example of everything". |

Readouts:

- **`coverage`** — filled cells / all cells. Rises fast, then crawls;
  when it stops, the reachable behaviours are found.
- **`qd_score`** — sum over elites of (fitness − the objective's lower
  bound). Rises when new cells fill *and* when existing cells improve. With
  no objective it equals the elite count.
- **`obj_max`, `obj_mean`** — the best and average elite fitness.

**Walk-through: the Wolfram-class map.** Run

```bash
cd cella_lib && cargo run --release --example explore -- ../configs/2d_map_elites_life_classes.json evolve map.json
```

Genes: the three neighbour counts of a Life-like rule. Objective: lifetime.
Descriptors: mean activity × final entropy, 20 × 20. Random 30 % start each
generation, so no rule can memorise a picture. After 60 generations
coverage is around 0.3, and reading the archive top to bottom: the low
activity row is rules that die or freeze (class 1/2), the high activity,
high entropy corner is noise (class 3), and the long-lifetime elites sit in
the middle band, where Life itself lives (class 4). Click one in the GUI
gallery and press Play to see it.

## 10. Novelty: reward being different

```json
"search": {"novelty": {"k": 15}}
```

Fitness is replaced by **novelty**: the mean distance in descriptor space
to the `k` nearest behaviours seen so far (the current generation plus an
archive). Anything far from everything is kept in the archive. The
threshold to enter adapts: it rises by 20 % when more than a tenth of a
generation gets in, and drops by 5 % when nobody does, so the archive grows
at a steady rate whatever the scale of your descriptors. The archive caps
at 2 000 entries (older ones are replaced at random).

| Field | Default | How to choose |
|---|---|---|
| `k` | 15 | Neighbours averaged. Smaller = spikier, rewards isolated oddities; larger = smoother. |
| `threshold` | adaptive | Set a number to freeze it; leave it out. |

Novelty search still needs an `objective` field to report *something* as
"best" in the log, but it does not steer. `configs/1d_novelty_rule_space.json`
walks the 256 elementary rules by final density × mean activity × period,
and its archive is a catalogue of what radius-1 rules can look like.

## 11. Run it from Rust and the command line

**Rust.** A config builds either engine:

```rust
use cella_lib::config::CellaConfig;
use cella_lib::explore::Sim;

let cfg = CellaConfig::from_file("configs/2d_wildfire_ensemble.json")?;
let mut ens = cfg.build_ensemble().expect("config has an ensemble block")?;
ens.step_n(200)?;
let prob: Vec<f32> = ens.state_probability(ens.track());   // one value per cell
let observed: Vec<bool> = /* what you saw */;
let report = ens.assimilate(&observed, ens.track())?;       // scores, effective_sample_size, immigrants
println!("p0 now {:?}", ens.genome_stats("model.p0"));

let mut evo = cfg.build_evolution().expect("config has an evolve block")?;
let reports = evo.run(30, |r| eprintln!("gen {} best {:.3}", r.generation, r.best));
let mut sim: Sim = cfg.build_sim().unwrap();
evo.apply_best(&mut sim)?;                                   // write the winner into a grid
```

Without a config: `Ensemble::new(sim, &EnsembleConfig { .. })`,
`Evolution::new(sim, &EvolveConfig { .. })`, or `Evolution::with_fitness`
for your own `Fitness`. `Sim` wraps a `Grid1D` or `Grid2D` and exposes
`step`, `cells`, `params`/`get_param`/`set_param`, `mask(types)`,
`thumbnail(max_side)`.

**Command line.** `cella_lib/examples/explore.rs` runs either block of any
config:

```bash
cd cella_lib
cargo run --release --example explore -- <config.json> ensemble <steps> [report.json]
cargo run --release --example explore -- <config.json> evolve [report.json] [--cell i[,j[,k]]]
```

| Env | Does |
|---|---|
| `EXPLORE_SEED` | replace the block's seed (and the grid seed) |
| `EXPLORE_GENES=path.json` | a JSON array of genes replacing the block's list |
| `EXPLORE_EVERY` | ensemble: report every N steps (default 10) |
| `EXPLORE_MASKS=path.json` | ensemble: `[{"step": k, "mask": [0/1 per cell]}]`; at each step the forecast is scored against the mask *before* learning from it |
| `EXPLORE_GENERATIONS` | evolve: override `generations` |
| `EXPLORE_THUMBS=1` | evolve: include archive thumbnails in the report |
| `EXPLORE_APPLY=out.json` | evolve: write the config with the best genome (or the `--cell` elite) applied |

The ensemble report has one entry per batch: tracked fractions (mean, sd),
consensus cell count, activity and entropy, genome statistics, and, when a
mask was given, `assimilation` with consensus IoU, mean and best member IoU,
Brier score, effective sample size and immigrant count. The evolve report
has the generation log, the hall of fame, `best_config`, and for
MAP-Elites/novelty the archive (dims, ranges, labels, per-generation
statistics, final cells).

The wildfire validation runner `cella_lib/examples/wildfire_smc.rs` is the
same engine with the fire's weather schedule and observation series wired
in; its command line and report fields did not change when the engine was
generalised (experiment E31 checks that).

Two more modes of that runner show what the engines are good for beyond
forecasting. `wildfire_smc <dir> 32 evolve out.json` fits the genes to
the first observed days with a GA and then forecasts forward
(experiment E36: the day-by-day filter beat it on every fire).
`wildfire_smc <dir> 32 map out.json` runs MAP-Elites with no objective over
the spread knobs, with growth × elongation as the axes, and plots the real
fire in the same coordinates (E37): a **reachability test**. If the
observed fire sits outside the shaded region, no calibration can reach it
and the model itself has to change. That is a use of illumination worth
copying for any model: before tuning, ask what the knobs can produce at all.

## 12. Read results honestly

- **Score forecasts, not fits.** Compare the map made *before* an
  observation with that observation. Assimilate first and score afterwards
  and you are grading the answer key.
- **Put a Brier score next to a dumb baseline.** A probability map is
  judged by `mean((p − outcome)²)`; the validation harness prints the
  area-matched circle's Brier beside the ensemble's. A confident wrong map
  scores badly; a 30 % that is right 30 % of the time scores well.
- **Watch the effective sample size** after each assimilation. Near 1
  means the population collapsed on one member; raise `immigrants` or
  lower `beta`.
- **`repeats` is not a free lunch.** A genome evolved against three seeds
  can be tuned to those three seeds. Re-score the winner with a different
  `seed` before believing the number.
- **Trivial targets.** "Maximise `fraction` of Alive" is won by a rule
  that fills the grid on step 1. Add a `lifetime` objective, use a
  `target`, or use MAP-Elites, whose descriptors keep the boring winners
  in their own corner.
- **Complexity measures reward noise.** Entropy is maximal for a random
  grid. Pair it with activity or lifetime, or use it as a descriptor rather
  than an objective.
- **Learned knobs are diagnostic, not truth.** They are the values that
  make *this* model track *this* observation; when they land in physically
  plausible ranges that is reassuring, not proof.
- **The start matters.** What evolves under `"initial": "fixed"` is a rule
  that does X *from that picture*. Use `random` starts when the behaviour
  should be general.

## 13. Write a driver for your model

A **driver** is the one piece a model author writes when knob-turning is
not enough: something must happen to each member *at every step* (apply a
weather schedule) or *once a period* (roll a die to decide the fire is
contained). Drivers live in the model's crate, are registered with
`typetag` like the model itself, and appear in JSON as
`"driver": {"<name>": {...}}`.

```rust
#[typetag::serde]
pub trait MemberDriver: Send + Sync + Debug {
    fn apply(&self, sim: &mut Sim, genome: &Genome, space: &GeneSpace,
             forcing: &Forcing, state: &mut MemberState) -> Result<(), ModelError>;
    fn period_steps(&self) -> Option<u64> { None }
    fn period_end(&self, sim: &mut Sim, genome: &Genome, space: &GeneSpace,
                  state: &mut MemberState, rng: &mut Rng) -> Result<(), ModelError> { Ok(()) }
    fn free_genes(&self) -> Vec<Gene> { vec![] }
    fn owned_keys(&self) -> Vec<String> { vec![] }
    fn boxed_clone(&self) -> Box<dyn MemberDriver>;
}
```

- `apply` runs on every member at construction, after each learning step,
  and whenever the **forcing** (a `name → number` map, e.g. the wind right
  now) changes.
- `period_steps` = `Some(n)` makes the engine call `period_end` every `n`
  steps with the engine's sequential random generator (so results stay
  thread-count independent).
- `free_genes` declares genes only the driver reads (`wind_scale`), with
  default kinds, so a config can list them without a range.
- `owned_keys` names prefixed knobs the driver writes itself, so the engine
  does not write them too (the wildfire driver owns `model.p0` because it
  applies decay on top of the genome's value).
- `state` is a per-member scratch map (`MemberState`) that survives steps
  and resampling copies.

**`WildfireDriver`, line by line** (`cella_lib/src/wildfire/driver.rs`):

1. *Fields.* `steps_per_day` (50) and an optional `weather` list of
   `{hours, speed_ms, from_deg}` windows. Empty = the model's constant
   wind.
2. *`free_genes`.* `wind_scale` 0…1.5, `tau_days` 2…100 (log),
   `contain_a` −6…−1, `contain_b` −2…−0.3. A config that lists
   `{"key": "wind_scale"}` gets these bounds.
3. *`owned_keys`.* `["model.p0"]`.
4. *`apply`.* Works out the hour: from `forcing["hours"]` if present, else
   `step / steps_per_day × 24`. Picks the wind: forcing wins, then the
   schedule window whose `hours` is the latest not after now, then the
   model's own. Multiplies the speed by the `wind_scale` gene. Captures the
   member's base `p0` once into `state["p0_base"]` (the genome's `model.p0`
   if it has one). Computes `decay = e^(−hours / (24 τ))` when the
   `tau_days` gene is present, 1 otherwise. Sets
   `p0 = contained ? 0 : p0_base × decay` through `set_p0`, the exact
   O(cells) write that avoids a re-attach.
5. *`period_steps`.* `Some(steps_per_day)`: once a simulated day.
6. *`period_end`.* Only when both `contain_*` genes exist. Counts burning
   plus burned cells, compares with `state["burned_at_day_start"]`, and
   contains the member with probability `1 / (1 + e^(−(a + b ln growth)))`
   — slow days are likely to be the last (FSim's rule, experiment E28). A
   contained member sets `state["contained"] = 1` and `p0 = 0`, and keeps
   its cells. Then stores today's count for tomorrow.
7. *`boxed_clone`.* `Box::new(self.clone())`.

Downcasting `sim.model_mut()` to the concrete model is allowed inside a
driver (it is the model's own crate); it is never done inside `explore/`.
The test module at the bottom of the file shows an ensemble running with
the driver, a forced wind bending the map, and containment stopping members.

Your own driver needs: a struct with `Serialize`/`Deserialize`,
`#[typetag::serde(name = "yours")] impl MemberDriver for Yours`, and the
methods above. Everything else — resampling, mutation, reports, the GUI —
comes for free.

## 14. The GUI Explore tab

Open the right-hand workbench and pick **Explore** (see
[app.md](app.md) for the workbench tour). The tab is model-agnostic in the
same way the engines are: the **Genes** table lists every knob the loaded
grid declares (rule fields for a rule, `model.*` for a model, both for a
model grid with a rule), and **Tracked types** offers the grid's declared
types.

- Tick a gene to let it vary and edit its range; `log` samples per decade.
- **Monte Carlo**: members, seed, β, σ, immigrants; **Start** builds the
  ensemble from the grid *as it is now* on a background thread, turns on
  the probability layer (P), and the viewport tints cells blue → red by the
  share of members with a tracked cell there. **Run +N** steps the ensemble;
  **Run to grid step** catches it up with the main grid; **Learn from grid**
  scores every member against the tracked cells you painted and resamples
  (the effective sample size and mean IoU appear below). The main grid
  stays paintable and playable throughout; the members are private copies.
- **Evolve**: population, steps, repeats, elite, crossover, mutation, σ,
  immigrants, a **Search** combo (Best score / Novelty / MAP-Elites), the
  objective picker (metric, goal, target, when), and for the two
  illumination modes one to three **Behaviour axes**. **Start** then **Run
  +G** generations; the fitness chart plots best and mean; **Apply best**
  writes the best genome into the grid (pause first; **Undo rule** takes it
  back).
- **Archive** (MAP-Elites): coverage, QD score, a heat map of the first
  two axes coloured by fitness (hover for the bin midpoints and genome;
  click to apply), and a strip of the fittest elites' thumbnails.

Replacing the grid (load, resize, random fill with "clear first") drops
the worker and its map; the best genome and fitness history survive so
Reset → Apply → Play works.

## 15. Migrating from the September 2026 `ensemble` block

The first ensemble block (`prior`, wildfire only) is gone. Old files fail
with an unknown-field error pointing here. Translate the prior into genes:

| Old (`prior`) | New (`genes`) |
|---|---|
| `"p0": [a, b]` | `{"key": "model.p0", "range": [a, b], "scale": "log"}` |
| `"burn_duration": [a, b]` | `{"key": "model.burn_duration", "range": [a, b]}` |
| `"tau_days": [a, b]` | `{"key": "tau_days", "range": [a, b], "scale": "log"}`; leave it out for "no decay" |
| `"wind_scale": [a, b]` | `{"key": "wind_scale", "range": [a, b]}` |
| `"containment": {"a": [..], "b": [..]}` | `{"key": "contain_a", "range": [..]}` and `{"key": "contain_b", "range": [..]}` |
| (implicit) | add `"driver": {"wildfire": {"steps_per_day": 50}}` and `"track": ["Burning", "BurnedOut"]` |

`configs/2d_wildfire_ensemble.json` is the translated demo.

## 16. Limits (September 2026)

- Members are full grid clones (the wildfire slope table is shared). A
  shared-landscape member type would cut memory further.
- Only the first two descriptor axes are drawn in the GUI heat map; a
  third axis collapses onto the best cell.
- Initial conditions are not evolved; only knobs are. `"initial": "random"`
  varies the start per generation but does not search over it.
- One objective at a time. Multi-objective (Pareto) search is a natural
  next step on the same `Fitness` seam.
- The GUI Explore tab has been exercised headless (widgets draw, workers
  round-trip); a click-through on a machine with working GL is still owed
  (roadmap Phase 5).
