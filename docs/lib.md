# Cella Library Guide (`cella_lib`)

`cella_lib` is the engine behind Cella: the data structures and logic for
simulating 1D and 2D cellular automata (CA). You would read this guide if you
want to

- run simulations from your own Rust program,
- define rules in code or load them from JSON, or
- plug in your own transition model (an `ExternalModel`) for something that
  does not fit the "count my neighbors" rule style, such as a wildfire.

If you just want to run the program, read [app.md](app.md) instead. Rust
basics are assumed; nothing else is. Terms from outside Rust are explained
where they first appear.

**Contents**

1. [Concepts](#concepts)
2. [Quick example](#quick-example)
3. [Working with grids](#working-with-grids)
4. [The rule system](#the-rule-system)
5. [Configs and serialization](#configs-and-serialization)
6. [External models](#external-models-plugin-transition-engines)
7. [Performance internals](#performance-internals)
8. [Building and testing](#building-and-testing)

---

## Concepts

A **cellular automaton** is a grid of **cells**. Each cell has a **type** (a
name such as `"Alive"` or `"Forest"`). Time moves in discrete **steps**
(generations): in each step every cell looks at its neighbors and a **rule**
decides what type it becomes. All cells update at once, using the previous
step's picture.

The library's vocabulary:

- **`CellType`**: a cell's type. Names are arbitrary strings, except that
  `"Inactive"` (constant `INACTIVE`) is built in and means background or
  "nothing here"; out-of-bounds neighbors also read as Inactive.
  Under the hood a `CellType` is *interned*: each distinct name is stored
  once in a global table and the type is just a 4-byte handle into it (a
  `lasso2::Spur`, in a `ThreadedRodeo`). That makes it `Copy` and makes
  comparing two types a single integer compare, which matters because the
  stepping loop does billions of them. Make one with `CellType::from("Alive")`
  or `CellType::new("Alive")`, read the name back with `.as_str()`, and get
  the raw handle from the public field `.0`.
- **Rule**: an ordered list of **subrules**, each of which says "a cell of
  type A whose neighbors look like B becomes type C". `Rule1D` and `Rule2D`
  (see [The rule system](#the-rule-system)).
- **`Grid1D` / `Grid2D`**: the simulation containers. They own the cells,
  each cell's **age** (how many steps it has kept its type), an optional
  bounded **history** of its recent past types, population counts, and the
  rule. Cells are indexed in flat row-major order; in 2D,
  `idx = y * width + x`.
- **Seed**: every random draw is keyed on a per-grid `seed`, so the same seed
  always gives the same run.
- **`CellState`**: a plain per-cell snapshot (type, age, history). It is used
  when saving; the live grids do not store `CellState`s (see below). Rebuild
  them with `grid.to_cell_states()`.
- **`GridState`**: an in-memory-only snapshot of a whole grid (cells, ages,
  history, counts, rule, model). It is not a file format; it is the plumbing
  that [`CellaConfig`](#saving-and-resuming-a-run) and `explore::Sim` use to
  hand a live grid's state to `Grid*::from_state` and back.
- **External model**: a plugin that replaces the subrules for a 2D grid
  (see [External models](#external-models-plugin-transition-engines)).

How the grids store data, briefly: they use a **struct-of-arrays (SoA)**
layout, meaning one flat `Vec` per field (all the types in one `Vec`, all the
ages in another, all the histories in a third) instead of a `Vec` of per-cell
structs. Stepping reads and writes each array in a straight line, which the
CPU likes. Stepping is also **double-buffered**: the step reads from one
`Vec` of cell types and writes the next generation into a second one, then
swaps them, so a cell never sees half-updated neighbors. The history is a
per-cell **circular buffer** (`history_data` + `history_heads` +
`history_counts`), and `history_limit` must be at most 255 (the heads and
counts arrays are `u8`; `new()` asserts it).

### Module map

| Module | Contents |
| --- | --- |
| `types` | `CellType`, `CellState`, `INACTIVE`, the global string `interner()` |
| `rules` | `Rule1D`/`Rule2D` + subrules, `CountOp`, `Neighborhood2D`, `neighborhood_offsets`, `RuleError`, `TypeCounter` |
| `grid1d` / `grid2d` | `Grid1D` / `Grid2D`: storage, stepping, history access |
| `state` | `GridState` (in-memory snapshot, not a file format) and `Grid*::from_state` |
| `config` | `CellaConfig`, `Config1D`, `Config2D`, `RunSnapshot`: JSON loading and saving, including resuming a run mid-simulation |
| `resize` | `ResizeError` and the helpers behind `Grid*::resize` |
| `external` | `ExternalModel` plugin trait, `ChunkCtx`, `ModelEvent`, `GridView`, `ParamDesc`/`ParamKind`/`ParamValue` |
| `rng` | `Rng` (SplitMix64), `mix`, `cell_rand(seed, step, idx, stream)`, `STREAM_RULE`, `STREAM_FILL`: the one source of randomness |
| `tunables` | One key grammar over every knob: `rule.subrules[i].field` and `model.key` as `ParamDesc`s; `Grid*::{params, get_param, set_param}` |
| `explore` | Ensembles, evolution and illumination for any grid: `Sim`, `metrics`, `genome`, `driver`, `ensemble`, `evolve`, `archive`. Guide: [explore.md](explore.md); primers: [primer-monte-carlo.md](primer-monte-carlo.md), [primer-genetic-algorithms.md](primer-genetic-algorithms.md) |
| `threads` | `thread_count()`, `MIN_WORK_PER_CHUNK`, worker pools, test overrides |
| `wildfire` | **Not part of the engine: a worked example of it.** `WildfireModel` (the first `ExternalModel`), `wildfire::driver::WildfireDriver` (the worked `MemberDriver`), `wildfire::wind_field::mass_consistent` (terrain wind downscaling) |
| `chunking` (private) | `split_chunks`: carves the output buffers into disjoint per-worker slices |

Re-exported at the crate root: `Grid1D`, `Grid2D`, `CellType`, `CellState`,
`INACTIVE`, `Rule1D`, `Rule1DSubrule`, `Rule2D`, `Rule2DSubrule`, `CountOp`,
`Neighborhood2D`, `neighborhood_contains`, `RuleError`, `GridState`,
`ResizeError`, the `ExternalModel` seam types (`ExternalModel`, `ChunkCtx`,
`GridView`, `ModelEvent`, `ModelError`, `ParamDesc`, `ParamKind`,
`ParamValue`), and from `explore`: `Sim`, `Ensemble`, `EnsembleConfig`,
`Evolution`, `EvolveConfig`, `GeneSpec`, `Metric`, `Objective`,
`MemberDriver`, `StateCorrection`.

Nothing from `wildfire` is re-exported at the crate root, on purpose:
`use cella_lib::*;` gives you the engine and nothing else, so you can tell at
a glance which types are library and which belong to one example model. Name
the module to use it: `use cella_lib::wildfire::{WildfireModel, WildfireParams};`.
Note the split around `MemberDriver`: the *trait* is engine API and is
re-exported, while `WildfireDriver`, the fire-specific implementation of it,
is not.

---

## Quick example

`cella_lib` is a path dependency of the `cella` binary; do the same in your
own `Cargo.toml` (adjust the path):

```toml
[dependencies]
cella_lib = { path = "../cella/cella_lib" }
```

Conway's Game of Life on a 5x5 grid, with a three-cell "blinker" in the middle
that flips between horizontal and vertical:

```rust
use cella_lib::{CellType, CountOp, Grid2D, Neighborhood2D, Rule2D, Rule2DSubrule};

fn main() {
    let alive = CellType::from("Alive");
    let dead = CellType::inactive();

    // One subrule: "a cell of type `current` with (op, count) `Alive`
    // neighbors in its Moore range-1 neighborhood becomes `out`".
    // Arguments: current, criteria, count, op, range, neighborhood,
    // output, randomness, limit.
    let sub = |current, count, op, out| {
        Rule2DSubrule::new(current, alive, count, op, 1, Neighborhood2D::Moore, out, None, None)
    };
    // Subrules are tried in order; the first match wins; no match means
    // the cell becomes Inactive.
    let rule = Rule2D {
        subrules: vec![
            sub(alive, 4, CountOp::Gt, dead),  // 4 or more neighbors: dies
            sub(alive, 2, CountOp::Gt, alive), // 2 or 3 neighbors: survives
            sub(dead, 3, CountOp::Eq, alive),  // exactly 3 neighbors: born
        ],
    };

    let (w, h) = (5, 5);
    let mut cells = vec![dead; w * h];
    for x in 1..=3 {
        cells[2 * w + x] = alive; // a horizontal row in the middle
    }
    let mut grid = Grid2D::new(w, h, /* history_limit */ 4, cells, rule);

    grid.step();

    // It is now a vertical column: x = 2, y = 1..=3.
    assert_eq!(grid.cell_type(1 * w + 2), alive);
    assert_eq!(grid.cell_type(2 * w + 1), dead);
    println!("alive cells: {}", grid.counts_current[&alive.0]);
}
```

(`gt` and `lt` are *inclusive*, so `Gt, 2` means "2 or more"; the second
subrule is only reached by cells that did not match "4 or more". More in
[The rule system](#the-rule-system).)

The same scenario as JSON, loaded from a file:

```rust
use cella_lib::config::CellaConfig;

let cfg = CellaConfig::from_file("configs/life.json")?;
let mut grid = cfg.build_grid2d().expect("a 2d config whose `initial` has width * height entries");
grid.step();
```

---

## Working with grids

```rust
let mut g = Grid2D::new(width, height, history_limit, initial, rule);
g.step();                              // advance one generation
g.cell_type(idx);                      // CellType at a flat index (inactive if out of bounds)
g.cell_age(idx);                       // consecutive steps in the current type
g.cell_history(idx);                   // Vec<CellType>, oldest first
g.cells();                             // &[CellType], the whole current picture
g.to_cell_states();                    // Vec<CellState>
g.transition_state_and_buffer(idx, &t) // manual paint; Some(io::Error) if idx is out of bounds
g.reset_cells(new_cells)?;             // replace the picture; step, ages, history and counts all reset
```

`Grid1D` has the same surface, indexed by cell position; its constructor is
`Grid1D::new(width, history_limit, initial, rule)`. Both grids carry a `seed`
(`with_seed` when building, `set_seed` later; `Grid2D` forwards it to its
model). In 2D, `idx = y * width + x`.

### Population counts

Each grid maintains `counts_current` and `peak_counts` as
`HashMap<Spur, u64>`, keyed by the interned type handle rather than by
`String`. To look up a count by name:

```rust
use cella_lib::CellType;
let alive = CellType::from("Alive");
let n = grid.counts_current.get(&alive.0).copied().unwrap_or(0);
```

Counts are kept up to date incrementally during stepping rather than
recounted (see [the counting trick](#stepping-interior-vs-edge)).

---

## The rule system

Cella uses a priority-based subrule system. When `step()` is called, each
cell evaluates the subrules in order. **The first subrule that matches
determines the cell's next type.** If no subrule matches, the cell becomes
`Inactive`.

So a pattern cannot spread unless you add a subrule with
`current_type = Inactive`. For 1D Rule 30 you need two subrules:
`current=X, criteria=X` (propagation) and `current=Inactive, criteria=X`
(birth). In the Life example above, "a cell with fewer than two neighbors
dies" needs no subrule at all, because not matching anything already means
Inactive.

### 1D rules (`Rule1D`)

1D rules use **Wolfram-style codes**: a number whose binary digits are a
lookup table from "what my window of cells looks like" to "do I match". A
subrule defines:

- `current_type`: the type the cell must have to match.
- `criteria_type`: the type counted as "on" when building the window pattern.
- `wolfram_code`: the lookup table, up to `u128`. Serialized as a **string**
  so JSON stays lossless above 2^53; integers are also accepted on read.
- `n`: the neighborhood radius (1 to 3). The window is `2n + 1` cells wide.
- `randomness`: optional probability in `[0, 1]` that a matching subrule is
  skipped.
- `output_type`: the type the cell becomes.

For `n = 1` the window is 3 cells, so there are 8 (2^3) possible patterns, and
the code is an 8-bit table. Rule 30 is `00011110` in binary. `n` up to 3
(128 patterns) is supported; `validate()` rejects larger radii
(`RuleError::TooManyPatterns`) and codes too large for the window
(`RuleError::InvalidWolframCode`). `Rule1D::n_max()` returns the widest
radius in the set.

### 2D rules (`Rule2D`)

2D rules use **threshold-based neighbor counting**. A subrule defines:

- `current_type`: the cell's required current type.
- `criteria_type`: the type of neighbors to count.
- `count`: the threshold value.
- `op`: the comparison (`lt`, `gt`, `eq`). `gt` and `lt` are **inclusive**:
  `gt` matches `neighbors >= count` and `lt` matches `neighbors <= count`
  ("at least" / "at most").
- `limit`: optional second bound turning the comparison into an inclusive
  between-range:
  - with `op = gt`: matches `count <= neighbors <= limit`
  - with `op = lt`: matches `limit <= neighbors <= count`
  - with `op = eq`: `limit` must be `None` (rejected by `validate()`)
- `range`: the radius of the neighborhood (at least 1).
- `neighborhood`: the shape (`Moore`, `VonNeumann`, `Langton`,
  `StraightLine`, `Knight`).
- `randomness`: optional probability in `[0, 1]` that a matching subrule is
  skipped.
- `output_type`: the type the cell becomes.

Construct subrules with
`Rule2DSubrule::new(current, criteria, count, op, range, neighborhood, output, randomness, limit)`.
The constructor precomputes three derived fields that the stepper depends on:

- `offsets`: the sorted `(dx, dy)` neighbor list for the shape and range.
- `pad`: the Chebyshev radius (`max(|dx|, |dy|)` over the offsets), i.e. how
  far from the grid edge a cell must be for all its neighbors to exist.
- `early_exit`: `op == Gt && limit.is_none()`, meaning neighbor counting can
  stop the moment `count` is reached.

All three are `#[serde(skip)]` and rederived on deserialization, which routes
through `new`, so JSON round-trips are safe. **Building a `Rule2DSubrule` with
struct-literal syntax skips the precomputation, so use `new`.** (Validate a
rule with `rule.validate()`; `RuleError` says what is wrong.)

#### Neighborhood shapes

- **Moore**: a square area around the cell (3x3 for range 1).
- **VonNeumann**: a diamond (Manhattan distance at most range).
- **Langton**: diagonal neighbors only (`|dx| = |dy|` at most range).
- **StraightLine**: the four cardinal lines out to `range` distance.
- **Knight**: every cell reachable in at most `range` chess-knight hops (each
  hop is 1 and 2 cells apart in perpendicular directions). `range = 1` gives
  exactly the 8 classic knight squares.

`neighborhood_offsets(shape, range)` and
`neighborhood_contains(dx, dy, range, shape)` are both *memoized* (results
are cached per argument, via `memoize::SharedCache`).
`neighborhood_contains` does a linear scan of the offset list and is meant for
tests and tooling, not for stepping loops.

### Randomness is reproducible

When subrule `i` has a skip probability, the stepper draws
`rng::cell_rand(grid.seed, step, cell_index, STREAM_RULE + i)`. That is a
*stateless* hash: the number depends only on those four inputs, with no shared
generator to advance. Two consequences:

- A rule with `randomness` is exactly reproducible for a given `seed`, on any
  thread count. (The `1d_randomness_512` / `2d_randomness_128` snapshot tests
  pin this by comparing FNV hashes of the output.)
- Deterministic rules pay nothing, because the draw only happens for subrules
  that ask for it.

The crate has no `rand` dependency.

---

## Configs and serialization

### JSON configuration

The `CellaConfig` enum (in `config.rs`) loads 1D and 2D setups from JSON.
The `"dim"` field (`"1d"` or `"2d"`) picks the variant.

```json
{
  "dim": "2d",
  "width": 100,
  "height": 100,
  "history_limit": 2,
  "initial": ["Inactive", "Alive", "..."],
  "rule": {
    "subrules": [
      {"current_type":"Alive","criteria_type":"Alive","count":4,"op":"gt","range":1,
       "neighborhood":"Moore","randomness":null,"limit":null,"output_type":"Inactive"}
    ]
  }
}
```

`initial` is a flat array of type *names* with length `width` (1D) or
`width * height` (2D). (The `"..."` above stands for the rest of the array.)

```rust
use cella_lib::config::CellaConfig;
let cfg = CellaConfig::from_file("configs/life.json")?;
let mut grid = cfg.build_grid2d().expect("2d config with matching initial length");
cfg.to_file_pretty("out.json")?;
```

`build_grid1d` / `build_grid2d` return `None` on a mismatch (wrong
dimension, or `initial.len()` not matching the declared size).

Optional fields, on both variants:

- `"seed"` (default 0): the grid's random seed.
- `"colors"`: a map `BTreeMap<String, String>` from type name to `#rrggbb`,
  read back with `CellaConfig::colors()`. The library only stores it (missing
  means empty; an empty map is not written out); the GUI applies it on load.
  It never influences a simulation.
- `"model"` (2D only): an [external model](#external-models-plugin-transition-engines).
- `"ensemble"` and `"evolve"`: settings for the `explore` engines, documented
  in [explore.md](explore.md) sections 7-8. `build_sim()` returns the grid as
  an `explore::Sim`; `build_ensemble()` / `build_evolution()` return
  `Option<Result<_, ModelError>>`: `None` when the block is absent, `Err`
  when it does not fit the grid (an unknown gene key, a `track` type nothing
  declares). These blocks reject unknown fields, so a config still using
  the older `prior` form fails to load with ``unknown field `prior` ``; the
  translation table is in [explore.md](explore.md) section 16.
- `"snapshot"`: a run in progress; see the next section.

### Saving and resuming a run

A saved file **is** a `CellaConfig`, the same shape as any other config
(dimensions, rule, model, seed, colors), plus one optional block,
`"snapshot"`, that holds only the run-time state a config cannot otherwise
express. `initial` always stays the scenario's *starting* cells (what Reset
goes back to), never wherever the run happened to be saved.

```rust
use cella_lib::config::CellaConfig;
use cella_lib::GridState;

let cfg = CellaConfig::from_file("configs/life.json")?;
let mut grid = cfg.build_grid2d().expect("2d config with matching initial length");
let initial = GridState::from_grid2d(&grid); // the Reset target, before stepping
for _ in 0..50 { grid.step(); }

// Save: at step 0 this writes no `snapshot` and `initial` becomes the grid's
// own cells; past step 0, `initial` stays the scenario's start and a
// `snapshot` of the run in progress goes in alongside it.
let saved = CellaConfig::save_2d(&initial, &grid, Default::default());
saved.to_file_pretty("out.json")?;

// Load back: `build_grid2d`/`build_grid1d` always give the initial state and
// ignore any `snapshot`; the `_resumed` twins give the state the snapshot
// describes, or `None` if there isn't one, or its lengths don't match the
// config (a hand-edited or truncated file).
let at_start = saved.build_grid2d().unwrap();
let mid_run = saved.build_grid2d_resumed().unwrap();
```

A file's `snapshot` block, when present, looks like:

```json
"snapshot": {
  "step": 120,
  "cells": ["Burning", "..."],
  "ages": [3, 0, "..."],
  "history": [["Forest"], "..."],
  "peak_counts": {"Burning": 812}
}
```

Nothing here duplicates the rest of the file:
`width`/`height`/`history_limit`/`rule`/`model`/`seed` each live once, at the
top level, and `cells`/`ages`/`history` have one entry per grid cell, in the
same order and length as `initial`. The live grid's `counts_current` is not
saved at all (it is cheap to recount from `cells`), but `peak_counts` is,
since a peak from earlier in the run cannot be recovered from where the cells
ended up.

`build_grid1d_resumed` / `build_grid2d_resumed` convert the config and its
snapshot into an in-memory `GridState` and hand it to
`Grid1D::from_state` / `Grid2D::from_state`, the same restore path
`explore::Sim::from_state` uses, so the history rebuild, count recompute, and
model `attach` all happen in one place. A model's own derived state (the
wildfire model's `arrival` table, for one) is never saved; `attach` rebuilds
it every time, whether this is the first load or a resume. For the arrival
spread rule this has one visible effect: every cell that is Burning at the
resume step restarts with arrival time equal to that step, not its true
earlier ignition time, so a resumed fire lags an uninterrupted one by about
one cell.

`GridState` is in-memory only. If you need a grid's raw per-cell state with no
scenario context, `grid.to_cell_states()` gives a `Vec<CellState>`, but
nothing in the library serializes it directly; `CellaConfig` is the one
save/load path.

### Resizing a grid

You can change a grid's size in the middle of a run:

```rust
grid.resize(80, 50)?;            // Grid2D: new width, new height
row.resize(120)?;                // Grid1D: new width
sim.resize(80, 50)?;             // Sim: height is ignored for 1D
```

The top-left corner stays put. Any cell that exists in both the old and the
new size keeps its type, its age and its history. New cells start Inactive,
with age 0. The step count, seed, rule and peak counts carry on, so the run
continues rather than restarting.

If the grid has a model, the model is asked to fit itself to the new size
(`ExternalModel::resize`). The wildfire model crops or pads its terrain, wind
and density layers; a new cell copies the nearest old edge cell, so the
terrain continues instead of dropping off a cliff. It also keeps the arrival
times it has already worked out. If the model refuses, `resize` returns an
error and the grid is left exactly as it was.

**Watch out: resizing the width changes the random numbers.** Every random
draw is keyed by the cell's flat index, `y * width + x`. When the width
changes, every cell below the first row gets a new index, so from then on it
draws different random numbers than it would have. The run is still
repeatable (the same resize at the same step gives the same result), but it is
no longer the run you would have got without the resize. Changing only the
height, or resizing a 1D grid, keeps every surviving cell's index and
therefore its random numbers.

---

## External models (plugin transition engines)

Some processes do not fit "count neighbors, compare to a threshold": wind
blowing fire uphill, for instance. For those, `Grid2D` carries an optional
`model: Option<Box<dyn ExternalModel>>`. When present, `step()` hands each
chunk of the grid to the model instead of evaluating `Rule2D` subrules. The
engine keeps ownership of ages, history, population counts, double buffering,
and parallelism, so a model cannot corrupt those invariants. A model-driven
grid simply carries `rule: { subrules: [] }`.

### What happens in one step

1. The engine splits the grid into **chunks** (contiguous runs of cells), one
   per worker thread when the grid is big enough (see
   [Threading](#parallelism-and-threading)).
2. Each chunk calls `model.step_chunk(&ChunkCtx, next)` to fill in the next
   types for its cells; the engine then runs its bookkeeping (ages, history,
   counts).
3. Chunk results (a `TypeCounter` and any `ModelEvent`s) are merged.
4. Events are sorted and applied serially before the buffers swap, gated by
   `model.event_applies` so that applying them is idempotent (doing it twice
   changes nothing) and independent of how many chunks there were.

### The determinism contract

`step_chunk` runs concurrently, under any partition of the grid. A model must
therefore get its randomness from per-cell counters, not from a shared
generator: a stateless hash of `(seed, ctx.step, index, stream)`, i.e.
`cella_lib::rng::cell_rand`, the same function the rule stepper uses. Pick a
`stream` number below `STREAM_RULE` (16); subrules use `16 + i` and random
fill uses 64. That makes stochastic runs exactly reproducible,
thread-count-independent, and snapshot-testable. Also implement
`set_seed(&mut self, seed)` (default: does nothing) so `Grid2D::set_seed`
reaches the model; ensembles call it once per member.

### Writing a model in your own crate

```rust
use cella_lib::external::{ChunkCtx, ExternalModel, GridView, ModelError, ModelEvent};
use cella_lib::CellType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
struct MyModel { /* params (serialized) + #[serde(skip)] derived state */ }

#[typetag::serde(name = "my_model")]   // JSON tag inside configs/snapshots
impl ExternalModel for MyModel {
    fn attach(&mut self, view: &GridView) -> Result<(), ModelError> { /* validate + build derived */ Ok(()) }
    fn step_chunk(&self, ctx: &ChunkCtx, next: &mut [CellType]) -> Vec<ModelEvent> {
        for local in 0..next.len() {
            let idx = ctx.start + local;
            next[local] = transition_of(ctx.cells[idx]); // your transition logic
        }
        Vec::new()   // long-range writes go here (see ModelEvent)
    }
    fn boxed_clone(&self) -> Box<dyn ExternalModel> { Box::new(self.clone()) }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
```

`#[typetag::serde]` (from the `typetag` crate) registers the type under a
name so that `Box<dyn ExternalModel>` can be saved to and loaded from JSON;
your crate needs `typetag` and `serde` as dependencies. The four methods
above are the only required ones. Attach a model with
`grid.attach_model(Box::new(model))?`, or in a config as
`"model": {"my_model": {...}}` alongside an empty rule. The model rides along
in a saved config's `model` block as-is (see
[Saving and resuming a run](#saving-and-resuming-a-run)); its derived state is
rebuilt via `attach` when the config is loaded back, not saved.

Optional hooks (all have defaults):

| Hook | Use it to |
| --- | --- |
| `resize` | re-fit per-cell layers when the grid changes size (default re-runs `attach`; override if you store per-cell data) |
| `event_applies` | gate long-range writes |
| `on_paint` | refresh derived state when the user paints a cell |
| `declared_types` | list the cell types a UI should offer for painting |
| `work_per_cell` | tell the engine how costly a cell is, for splitting work |
| `set_seed` | reseed your randomness (ensembles do this per member) |
| `params`, `get_param`, `set_param` | describe tunable values so a UI can build controls (next section) |

### Model parameters

A model can describe its own tunable values so a generic UI can build
controls without knowing anything about the model. Three types in
`external.rs` carry the description:

- **`ParamValue`**: the value itself, as read from or written to a control:
  `Float(f64)`, `Int(i64)`, `Bool(bool)`, `Choice(String)`, or `Bits(u128)`
  (serialized as a decimal string so JSON stays lossless).
- **`ParamKind`**: what kind of control the value wants, and its legal range:
  `Float { min, max, step }`, `Int { min, max }`, `Bool`,
  `Choice { options }`, or `Bits { len }` (`len` independent on/off bits; a 1D
  rule table is `2^(2n+1)` of them).
- **`ParamDesc`**: one row of self-description: `key` (the stable name used
  with `get_param`/`set_param`), `label`, an optional `group` (a panel heading
  for related controls), an optional `help` tooltip, an optional `unit` suffix
  (`"m/s"`, `"°"`), the `kind`, whether changing it needs `reattach` (see
  below), and whether it is `read_only`.

Three `ExternalModel` methods carry these around, and **all three have
default implementations**: `params() -> Vec<ParamDesc>` defaults to an empty
list, `get_param(&self, key) -> Option<ParamValue>` to `None`, and
`set_param(&mut self, key, value) -> Result<(), ModelError>` to rejecting
every key. A model that implements none of them still compiles and simply
shows no controls.

Two rules come with those methods, and both are easy to trip over:

1. **`get_param` must return `Some` for every key `params()` lists.** The
   engine's rollback puts back the value `get_param` reported, so a
   `reattach: true` key it will not answer has no way home, and
   `set_model_param` refuses to write such a key at all rather than strand the
   model.
2. **`set_param` validates nothing and rebuilds nothing on its own.** It is
   the raw write. Library callers should go through
   `Grid2D::set_model_param(&mut self, key: &str, value: ParamValue) -> Result<(), ModelError>`,
   which does the generic work so a model author does not have to. It:
   1. looks up `key` in `model.params()` (unknown key or `read_only` is an
      error);
   2. checks `value` against the descriptor's `ParamKind` bounds;
   3. reads the old value with `get_param` and refuses the whole write if a
      `reattach` parameter has none;
   4. calls `model.set_param(key, value)`;
   5. only when the descriptor says `reattach: true`, calls `model.attach`
      again to rebuild derived state. If that fails, it writes the old value
      back, calls `attach` once more so the model ends up exactly as it was
      (rollback), and returns the original error.

   Calling `set_param` directly is for a model that is not attached to a grid,
   such as the clone inside a saved config, which is re-attached when the
   config is loaded back.

A one-parameter model implementing all three methods:

```rust
fn params(&self) -> Vec<ParamDesc> {
    vec![ParamDesc {
        key: "speed".into(),
        label: "Speed".into(),
        group: None,
        help: None,
        unit: Some("m/s".into()),
        kind: ParamKind::Float { min: 0.0, max: 10.0, step: 0.1 },
        reattach: false,
        read_only: false,
    }]
}
fn get_param(&self, key: &str) -> Option<ParamValue> {
    (key == "speed").then(|| ParamValue::Float(self.speed))
}
fn set_param(&mut self, key: &str, v: ParamValue) -> Result<(), ModelError> {
    match (key, v) {
        ("speed", ParamValue::Float(v)) => { self.speed = v; Ok(()) }
        _ => Err(ModelError::InvalidParam(format!("unknown '{key}'"))),
    }
}
```

The same description covers a grid's *rule*: `tunables::rule2d_params` /
`rule1d_params` list `rule.subrules[i].count`, `.limit`, `.range`, `.op`,
`.neighborhood`, `.randomness` (2D) and `.wolfram_code` (1D, as `Bits`) as
`ParamDesc`s. `Grid1D::params` / `Grid2D::params` return rule and `model.*`
knobs together, with `get_param` / `set_param` (a rule write rebuilds the
subrule through `Rule2DSubrule::new` and runs `validate()`; a refusal leaves
the rule untouched). This is the key grammar the `explore` genes use.

### Drivers: what a model adds to an ensemble

An ensemble runs many copies ("members") of one grid, each with its own seed
(see [explore.md](explore.md)). Turning knobs is generic. Anything else a
model needs per member (a weather schedule, a daily containment roll) is a
`MemberDriver` in the model's own crate, registered with
`#[typetag::serde(name = "...")]` and named in JSON as
`"driver": {"name": {...}}`. It gets `apply` (every member, every forcing
change), an optional `period_end` every `period_steps`, `free_genes` for
genes only it reads, and `owned_keys` for prefixed knobs it writes itself.
`wildfire::driver::WildfireDriver` is the worked example, explained line by
line in [explore.md](explore.md) section 13.

### The wildfire model (a worked example)

Everything in this section lives under `cella_lib::wildfire`, and none of it
is re-exported at the crate root. The module demonstrates the `ExternalModel`
and `MemberDriver` seams; it is not a feature of the engine, which could drop
it and lose nothing. Read it as the answer to "what does a real model plugged
into this library look like?"

`wildfire::WildfireModel` implements Alexandridis-style stochastic spread:
per-cell base probability `p0 * veg_factor * density`, exponential wind
(`c1`, `c2`; direction as the meteorological bearing the wind comes *from*,
`wind_from_deg`, 0 degrees = north, clockwise, grid north-up; check any
external weather feed against `validation/TEST_PLAN.md` section 2.1 before
comparing results) and slope (`slope_a`) modifiers with a `1/sqrt(2)` diagonal
correction, burn duration tracked through cell ages, and lognormal firebrand
spotting delivered as `ModelEvent`s. Slope factors are precomputed per cell
at attach; wind factors once per chunk; the per-cell loop is two multiplies
per burning neighbor plus one hash draw. See `configs/2d_wildfire_demo.json`
and the module docs for parameters (defaults follow Alexandridis et al. 2008).

Around it:

- `wildfire::WildfireDriver` applies the wind schedule, optional `tau_days`
  decay and the FSim-style daily containment roll to each ensemble member.
  `configs/2d_wildfire_ensemble.json` shows the `"ensemble"` block.
- `wildfire::wind_field::mass_consistent` / `MassConsistentBasis` downscale one
  wind over the elevation layer (WindNinja-style mass conservation: ridges
  speed up, valleys channel) into a per-cell field for
  `WildfireModel::set_wind_field`.
- `set_density` paints a per-cell multiplier (retardant, wet line) that can be
  restored.
- Members share the slope table (`Arc`), so an ensemble costs roughly
  cells x members x 8 bytes.
- `cella_lib/examples/wildfire_smc/` is the validation runner built on all of
  this (its module docs describe the run modes; the experiments that use it
  are in [validation/README.md](../validation/README.md)).

**Two spread rules.** `params.spread` picks between them (default
`"bernoulli"`, so nothing above changes unless you opt in).

- `"bernoulli"` is the rule just described: every tick, every unburned
  neighbor of a burning cell rolls independent dice, one per burning
  neighbor, at a *probability*. A probability tops out at 1, so once a cell
  has enough burning neighbors it catches almost immediately no matter which
  direction they came from, which is why a big fire under this rule tends to
  come out round rather than stretched.
- `"arrival"` fixes that with the standard fire-CA **minimum-travel-time**
  idea instead. Every fuel cell keeps an *arrival time* in ticks (starting at
  "never" until a path to it exists). Each tick, a still-unburned cell with a
  burning-or-already-burned neighbor asks that neighbor "how soon could I have
  caught, coming from you?": the neighbor's own arrival time plus the *cost*
  (in ticks) of crossing the one cell between you, where cost is distance over
  speed and speed is the same per-direction wind/slope number the Bernoulli
  rule uses as a probability. It keeps the smallest answer found across every
  such neighbor, and catches fire the first tick its own tick counter reaches
  that number.

  Worked example: a rate of 0.5 cells/tick costs 2 ticks to cross, so a cell
  one cardinal step from a source that ignited at tick 0 catches at tick 2; a
  diagonal step costs `sqrt(2)` times more (it is physically farther away) than
  a cardinal one at the same rate. A slow direction simply costs more ticks
  per cell; it never "catches up" to a fast one the way a saturating
  probability does, so the fast-direction-vs-slow-direction shape survives no
  matter how big the fire gets. Burn duration no longer has any say in *when*
  a cell catches (only how long it keeps burning, and so stays eligible to
  spot, once it does). A small per-cell `arrival_jitter` (log-normal, one draw
  per cell for the whole run, multiplying that cell's own cost) keeps
  otherwise-identical cells from igniting in perfect lockstep.

Independently, `params.wind_law` picks which formula produces the eight
direction numbers either rule reads: `"exponential"` (default, the wind kernel
above) or `"rear_focus"`, an Anderson (1983) fire-ellipse template that gives
a much bigger head-to-tail difference at low wind speeds than the exponential
kernel can.

---

## Performance internals

You do not need this section to use the library; it explains why stepping is
fast, for anyone reading or changing the engine. Every optimization below is
invisible from outside except for speed.

### Stepping: interior vs. edge

Both steppers split each chunk's cells into an **interior** fast path and an
**edge** slow path, using the rule's `pad` (the widest neighborhood radius) as
the margin:

- **Interior**: every neighbor is guaranteed to be in bounds. 2D reaches
  neighbors by adding a precomputed *linear* offset (`dy * width + dx`) to the
  cell's own index: no per-neighbor bounds comparison and no `y * width + x`
  multiply. 1D writes out the `n = 1|2|3` windows as straight-line code
  instead of a loop whose bound is only known at runtime, which the compiler
  does not unroll.
- **Edge**: within `pad` of a border. Out-of-bounds neighbors read as
  Inactive.

2D hoists the y-component of the interior test to the row level
(`row_interior`), so the per-cell check only bounds `x`.

`Rule2DPlan` holds the per-step precomputation shared by every chunk: linear
offsets per subrule, the max `pad`, and `work_per_cell`. It is rebuilt on each
`step()` rather than cached on the grid, so changing `grid.rule` between steps
can never leave a stale plan. Its cost is O(subrules x neighbors) against tens
of thousands of cells.

**The counting trick.** The stepping loop skips the current `dominant_type`
(the most populous type) when counting, and back-fills its population by
subtracting every counted type from the cell total, so the common case costs
no per-cell counter update. `apply_counts` re-elects the dominant type each
step: if a counted type now outnumbers the back-filled remainder, it takes
over. Without that, the skip would stop saving anything once the majority
flipped.

### Fast paths (bit-parallel stepping)

**Bit-parallel** means packing one cell into one *bit* and operating on 64
cells at a time with ordinary integer instructions. Three optimizations kick
in automatically when a rule or model has the right shape. Each checks its
preconditions every step and falls back to the normal ("scalar", one cell at a
time) path when they do not hold, and property tests pin their output to be
exactly identical to the scalar path: cells, ages, history, and counts.

**1D packed Wolfram** (`Grid1D::step_packed`, chosen by the internal
`Rule1DPlan`). When a 1D rule is a classic two-state Wolfram automaton (two
subrules sharing one transition table; see `PackedWolfram` in `rules.rs`), the
row is stored one bit per cell inside 64-bit integers. Shifting a word
left/right hands all 64 cells their left/right neighbors at once, and the
8-entry transition table becomes a handful of AND/OR operations per word.
Roughly 40 % faster on the rule-30 benchmarks.

**2D bit-plane threshold** (`Grid2D::step_packed`, chosen by the internal
`Rule2DPlan`). When a 2D rule is "life-like" (two cell types, every subrule
counting the same type over one shared radius-1 neighborhood; see
`PackedThreshold2D`), the next state only depends on (current state, neighbor
count): 18 possible situations, precomputed into a table. The grid is packed
one bit per cell; neighbor counts for 64 cells at a time are built by adding
eight shifted words with schoolbook binary carries, then the table is applied
with bitwise masks. Cuts the 256x256 Life benchmark by about 59 %, and is
faster single-threaded than the old 8-thread scalar path.

**Wildfire fire-front mask** (`WildfireModel::step_chunk`). The wildfire model
cannot use a count table (its ignition math depends on *which* neighbors burn,
with per-direction wind and per-cell slope factors), but fire only moves at
its edges: a cell can only change if it is Burning or touches a Burning cell.
The step copies the grid through as the default, builds a Burning bitmap, ORs
its eight shifts into a "might change" mask, and runs the full per-cell math
only for those cells: a thin front line instead of the whole grid. Roughly
halves the wildfire benchmarks. Skipped cells never consumed randomness, so the
stochastic output is bit-identical.

The performance history and the experiment log behind these (including the
approaches that were tried and made things *worse*) live in
[performance.md](performance.md) section 8. That document also reviews the
other engine optimizations (SoA layout, double buffering, interned types,
incremental counting, the interior/edge split, work-sized chunking on a
persistent pool), known issues, and a discussion of the Hashlife algorithm as
a possible future direction.

### Parallelism and threading

For user-level rules of thumb on run time (which settings cost what, and when
threads help), see [runtime-guide.md](runtime-guide.md).

Both `Grid1D` and `Grid2D` stepping use multiple cores through `rayon`, a Rust
library that runs work on a pool of worker threads.

#### Thread configuration

The library looks for a `cella.properties` file in the current working
directory and up to four parent directories (the nearest one wins):

```properties
threads=8
```

- If the file is missing, the key is not set, or its value is not a whole
  number of at least 1, the count defaults to
  `std::thread::available_parallelism()` (or 1 on error). The key name is
  case-insensitive, and lines starting with `#` or `//` are comments. The
  resolved value is cached for the process.
- Set `threads=1` to force single-threaded execution (useful for debugging).
- Tests and benchmarks can use `threads::set_thread_override(n)` /
  `clear_thread_override()` for a process-local override.
- Two environment variables are read once per process and do nothing unless
  set (see [performance.md](performance.md) section 9): `CELLA_MIN_WORK=<n>` changes
  the work-per-chunk threshold below, and `CELLA_MEMBER_PAR=<n>` caps how many
  ensemble members step concurrently.

#### Chunk sizing is driven by work, not grid size

Waking a parked worker costs real time (tens of microseconds on some
platforms), so the split is sized by *estimated work*, not by cell count:

```
nchunks = clamp(total_work / MIN_WORK_PER_CHUNK, 1, thread_count())
```

with `threads::MIN_WORK_PER_CHUNK = 400_000` nominal neighbor visits.
`total_work` is `cells x work_per_cell`, where `work_per_cell` is the sum of
neighborhood sizes over subrules (2D) or of `2n + 1` window widths (1D). It is
an upper bound (it ignores early exit and non-matching subrules), which is the
safe direction: it never promotes a grid that is too small to parallelize.

`nchunks <= 1` runs the step serially on the calling thread with no pool
involvement. `threads::set_min_work_per_chunk_override(n)` /
`clear_min_work_per_chunk_override()` lower the threshold so tests can force
the multi-threaded path on grids small enough to check exhaustively.

#### Implementation

`step()` calls `chunking::split_chunks` to carve the output buffers
(`next_cells`, `ages`, and the three history arrays) into disjoint per-worker
`OutChunk`s, then runs `step_chunk` over them with `par_iter_mut` inside a
**persistent, size-keyed rayon pool** (`threads::pool(n)`). Workers read the
shared `cells` buffer and write only their own slices, so there is no locking
on cell data. Each chunk returns a `TypeCounter` (a small `Vec`-backed
counter, no `HashMap` allocation) and the results are `reduce`d by merging.

Pools are built on first use per thread count and live for the process, so
stepping only pays fork/join on already-parked workers rather than spawning
fresh threads every step.

---

## Building and testing

`cella_lib` is **not** a workspace member of the root `Cargo.toml`: it is its
own build root with its own `cella_lib/target/` directory. That is why the
`Makefile` test targets start with `cd cella_lib &&`, and why running plain
`cargo test` at the repository root tests only the `cella` binary. Use the
`Makefile` (or `cd cella_lib` first):

| Target | Does |
| --- | --- |
| `make build` / `make build-release` | build the `cella` binary |
| `make run` / `make run-gui` | run CLI mode / GUI at 1024x768 |
| `make check` / `make fmt` / `make fmt-check` | `cargo check` / `cargo fmt` from the repository root |
| `make clippy` | clippy with `-D warnings` at the root, then again for `cella_lib`'s examples (`cd cella_lib && cargo clippy --examples`) |
| `make doc` | `cargo doc --workspace --no-deps --open` at the root (see [Rustdoc](#rustdoc) for the library's own docs) |
| `make test` | fast pass: non-ignored `cella_lib` tests, plus the `wildfire_smc` example's tests |
| `make test-all` | the same including `#[ignore]`d long-running tests |
| `make test-all-single` | `cella_lib` tests including ignored ones, with `--test-threads=1` (needed for the benchmark summary ordering) |
| `make test-all-single-run` | as above with `CELLA_BENCH_RUNS=1` and `CELLA_EXPORT_CONFIGS=1` |
| `make coverage` / `make coverage-all` | `cargo llvm-cov --html` for `cella_lib` (the `-all` form includes ignored tests; needs `cargo-llvm-cov`) |
| `make test-create-snapshots` | regenerate `cella_lib/tests/snapshots/*.txt` |
| `make test-update-benchmarks` | rewrite `cella_lib/tests/benchmarks_last.json` |
| `make bench-ab A=<git ref> FILTER='<name>'` | interleaved A/B timing of a git ref against the working tree (see [performance.md](performance.md)) |
| `make clean` | `cargo clean` at the repository root |

Environment variables the test suite reads:

- `CELLA_ASCII=1`: write ASCII renders of the initial and final grids (off by
  default; `make test`, `test-all`, `coverage` and `coverage-all` pin it to `0`).
- `CELLA_UPDATE_SNAPSHOTS=1`: write snapshot hashes instead of asserting them.
- `CELLA_UPDATE_BENCH=1`: write benchmark baselines instead of comparing.
- `CELLA_BENCH=1`: print per-test timings as they complete.
- `CELLA_BENCH_RUNS=N`: timed repeats per benchmark (default 10). Timings are
  recorded per thread count (`<name>_t<threads>`).
- `CELLA_BENCH_WARMUP=N`: untimed warm-up runs before the timed ones
  (default 1).
- `CELLA_EXPORT_CONFIGS=1`: write a JSON config file for each scenario the
  tests build.

Integration tests live in `cella_lib/tests/`: `config_tests.rs` (JSON
round-trips; every shipped config loads and builds its blocks),
`edge_cases.rs` (validation boundaries), `randomness.rs`, `soa_robust.rs`
(history circular buffer and serial/parallel agreement), `external_model.rs`,
`explore_generic.rs` (an out-of-tree model and driver run through
`CellaConfig` JSON, proving the engines need nothing from the `wildfire`
module), `wildfire_stats.rs`, and `long_suite.rs` (ignored-by-default stress
tests, snapshots, and benchmarks).

### Rustdoc

The library is documented with Rustdoc, including usage examples on the
public types. Because `cella_lib` is its own build root, build its docs from
inside it (`make doc` only covers the root package):

```bash
cd cella_lib && cargo doc --no-deps --open
```
