# Cella Library Documentation (`cella_lib`)

`cella_lib` is the core engine for the Cella cellular automata project. It provides the data structures and logic for simulating 1D and 2D automata, handling rules, and managing grid state.

## Architecture Overview

The library is designed with a focus on:
- **Composability**: Rules are built from multiple "subrules" that are evaluated in order.
- **Performance**: Grids store cell data as struct-of-arrays with double-buffered stepping; each step is split into chunks that run on a persistent rayon worker pool when the estimated work justifies it. See [performance.md](performance.md) for a detailed review.
- **Serializability**: Grids and rules can be easily converted to and from JSON using `serde`.

### Module map

| Module | Contents |
| --- | --- |
| `types` | `CellType`, `CellState`, `INACTIVE`, the global string `interner()` |
| `rules` | `Rule1D`/`Rule2D` + subrules, `CountOp`, `Neighborhood2D`, `neighborhood_offsets`, `RuleError`, `TypeCounter` |
| `grid1d` / `grid2d` | `Grid1D` / `Grid2D`: SoA storage, stepping, history access |
| `state` | `GridState` snapshots and `Grid*::from_state` |
| `config` | `CellaConfig`, `Config1D`, `Config2D` — JSON scenario loading |
| `threads` | `thread_count()`, `MIN_WORK_PER_CHUNK`, worker pools, test overrides |
| `external` | `ExternalModel` plugin trait, `ChunkCtx`, `ModelEvent`, `GridView` — pluggable transition models |
| `wildfire` | `WildfireModel` — stochastic Alexandridis-style wildfire spread, the first `ExternalModel` |
| `chunking` (private) | `split_chunks` — carves the output buffers into disjoint per-worker slices |

Re-exported at the crate root: `Grid1D`, `Grid2D`, `CellType`, `CellState`, `INACTIVE`, `Rule1D`, `Rule1DSubrule`, `Rule2D`, `Rule2DSubrule`, `CountOp`, `Neighborhood2D`, `neighborhood_contains`, `RuleError`, `GridState`, `grid2d_to_json`.

### Core Components

- **`CellType`**: An interned symbol identifying a semantic state (e.g., "Alive", "Dead", "X", "Inactive"). It is a newtype over a `lasso2::Spur` — a 4-byte handle into a global `ThreadedRodeo` interner — so it is `Copy` and comparisons in the stepping hot path are single integer compares. Create one with `CellType::from("Alive")` or `CellType::new("Alive")`; get the name back with `.as_str()`; the raw handle is the public field `.0`.
- **`CellState`**: A per-cell snapshot (current type, `age_in_state`, `history_limit`, bounded history). Used as a serialization intermediate; the live grids do not store `CellState`s. Rebuild them on demand with `grid.to_cell_states()`.
- **`Grid1D` / `Grid2D`**: The primary simulation containers. Internally they use a struct-of-arrays layout: flat `Vec`s for current types, ages, and a per-cell circular history buffer (`history_data` + `history_heads` + `history_counts`), plus a second cell buffer for double-buffered stepping. `history_limit` must be ≤ 255 — the heads/counts arrays are `u8`, and `new()` asserts it.
- **`Rule1D` / `Rule2D`**: Contain lists of subrules that define how cells transition between states.
- **`GridState`**: A flat, serializable representation of a grid's current configuration, useful for snapshots.

### Grid API

```rust
let mut g = Grid2D::new(width, height, history_limit, initial, rule);
g.step();                              // advance one generation
g.cell_type(idx);                      // CellType at a flat index (inactive if OOB)
g.cell_age(idx);                       // consecutive steps in the current state
g.cell_history(idx);                   // Vec<CellType>, FIFO, oldest first
g.to_cell_states();                    // Vec<CellState> for serialization
g.transition_state_and_buffer(idx, &t) // manual paint; Some(Error) if idx is OOB
```

`Grid1D` exposes the same surface, indexed by cell position. Both index cells row-major and flat: for 2D, `idx = y * width + x`.

### Population counts

Each grid maintains `counts_current` and `peak_counts` as `HashMap<Spur, u64>` — keyed by the interned type handle, not by `String`. To look up a count by name:

```rust
use cella_lib::CellType;
let alive = CellType::from("Alive");
let n = grid.counts_current.get(&alive.0).copied().unwrap_or(0);
```

Counts are maintained incrementally during stepping. The stepping loop **skips** the current `dominant_type` (the most populous type) and its population is back-filled by subtracting every counted type from the cell total, so the common case costs no per-cell counter update. `apply_counts` re-elects the dominant type each step: if a counted type now outnumbers the back-filled remainder, it takes over — otherwise the skip would stop saving anything once the majority flipped.

---

## The Rule System

Cella uses a priority-based subrule system. When `step()` is called, each cell evaluates the subrules in its assigned `Rule` set one by one. The first subrule that matches determines the cell's next state. If no subrule matches, the cell becomes `Inactive`.

This means a seed cannot spread unless you add an explicit subrule with `current_type = Inactive`. For 1D Rule 30 you typically need two subrules: `current=X, criteria=X` (propagation) and `current=Inactive, criteria=X` (birth).

### 1D Rules (`Rule1D`)

1D rules use **Wolfram-style codes**. A subrule defines:
- `current_type`: The type the cell must have to match.
- `criteria_type`: The type considered "active" when building the neighborhood bitmask.
- `wolfram_code`: A bitmask (up to `u128`) representing the transition table. Serialized as a **string** so JSON stays lossless above 2^53; integers are also accepted on read.
- `n`: The neighborhood radius (1–3). The window size is `2n + 1`.
- `randomness`: Optional probability in `[0, 1]` that a matching subrule is skipped.

For `n=1` (radius 1), the window is 3 cells. There are 8 ($2^3$) possible patterns. Rule 30 corresponds to the bitmask `00011110` in binary. `n` up to 3 (128 patterns) is supported; `validate()` rejects larger radii (`RuleError::TooManyPatterns`) and codes too large for the window (`RuleError::InvalidWolframCode`).

`Rule1D::n_max()` returns the widest radius in the set — the stepper uses it as the interior margin.

### 2D Rules (`Rule2D`)

2D rules use **threshold-based neighbor counting**. A subrule defines:
- `current_type`: The cell's required current state.
- `criteria_type`: The type of neighbors to count.
- `count`: The threshold value.
- `op`: The comparison operator (`lt`, `gt`, `eq`). Note that `gt` and `lt` are **inclusive**: `gt` matches `neighbors >= count` and `lt` matches `neighbors <= count` ("at least" / "at most").
- `limit`: Optional second bound turning the comparison into an inclusive between-range:
  - with `op = gt`: matches `count <= neighbors <= limit`
  - with `op = lt`: matches `limit <= neighbors <= count`
  - with `op = eq`: `limit` must be `None` (rejected by `validate()`)
- `range`: The radius of the neighborhood (≥ 1).
- `neighborhood`: The shape of the neighborhood (`Moore`, `VonNeumann`, `Langton`, `StraightLine`, `Knight`).
- `randomness`: Optional probability in `[0, 1]` that a matching subrule is skipped.

Construct subrules with `Rule2DSubrule::new(current, criteria, count, op, range, neighborhood, output, randomness, limit)`. The constructor precomputes three derived fields that the stepper depends on:

- `offsets`: the sorted `(dx, dy)` neighbor list for the shape and range.
- `pad`: the Chebyshev radius (`max(|dx|, |dy|)` over the offsets) — the interior margin.
- `early_exit`: `op == Gt && limit.is_none()`, i.e. neighbor counting can stop the moment `count` is reached. Hoisted out of the per-neighbor loop.

All three are `#[serde(skip)]` and are rederived on deserialization, which routes through `new` — so JSON round-trips are safe, but building a `Rule2DSubrule` with struct literal syntax skips the precomputation. Use `new`.

#### Neighborhood Shapes
- **Moore**: A square area around the cell (e.g., $3 \times 3$ for range 1).
- **VonNeumann**: A diamond shape (Manhattan distance ≤ range).
- **Langton**: Diagonal neighbors only ($|dx| = |dy| \le$ range).
- **StraightLine**: Cardinal lines extending out to `range` distance.
- **Knight**: All cells reachable in at most `range` chess-knight hops (each hop ±1/±2 or ±2/±1). `range=1` gives exactly the 8 classic knight squares.

`neighborhood_offsets(shape, range)` and `neighborhood_contains(dx, dy, range, shape)` are both memoized (`memoize::SharedCache`). `neighborhood_contains` does a linear scan of the offset list and is intended for tests and tooling — not the hot path.

---

## Stepping: interior vs. edge

Both steppers split each chunk's cells into an **interior** fast path and an **edge** slow path, using the rule's `pad` (the widest neighborhood radius) as the margin:

- **Interior** — every neighbor is guaranteed in bounds. 2D reaches neighbors by adding a precomputed *linear* offset (`dy * width + dx`) to the cell's own index: no per-neighbor bounds comparison, no `y * width + x` multiply. 1D expands the `n = 1|2|3` windows into straight-line code rather than a runtime-bounded loop, which does not unroll.
- **Edge** — within `pad` of a border. Out-of-bounds neighbors read as `inactive`.

2D hoists the y-component of the interior test to the row level (`row_interior`), so the per-cell check only bounds `x`.

`Rule2DPlan` holds the per-step precomputation shared by every chunk: linear offsets per subrule, the max `pad`, whether any subrule `needs_rng`, and `work_per_cell`. It is rebuilt each `step()` rather than cached on the grid, so mutating `grid.rule` between steps can never leave a stale plan. Its cost is O(subrules × neighbors) against tens of thousands of cells.

RNG is only constructed when `needs_rng` is set, so deterministic rules pay nothing for the randomness feature.

---

## External Models (plugin transition engines)

`Grid2D` carries an optional `model: Option<Box<dyn ExternalModel>>`. When present, `step()` hands each chunk to the model instead of evaluating `Rule2D` subrules; the engine keeps ownership of ages, history, population counts, double buffering, and chunked parallelism, so a model cannot corrupt those invariants. The subrule hot path is untouched — a model-driven grid simply carries `rule: { subrules: [] }`.

The step pipeline: `chunks_for_work(total × model.work_per_cell())` sizes the split → each chunk calls `model.step_chunk(&ChunkCtx, next)` to fill the next types, then the engine runs its bookkeeping pass → chunk results (a `TypeCounter` and any `ModelEvent`s) are reduce-merged → events are sorted and applied serially before the buffer swap, gated by `model.event_applies` so application is idempotent and independent of chunk count.

**Determinism contract**: `step_chunk` runs concurrently under any grid partition. A model must derive randomness from per-cell counters (e.g. a stateless hash of `(seed, ctx.step, index)` — see `wildfire::cell_rand`) rather than shared RNG state; that makes stochastic runs exactly reproducible, thread-count-independent, and FNV-snapshot-testable.

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
            next[local] = transition_of(ctx.cells[idx]);
        }
        Vec::new()   // long-range writes go here (see ModelEvent)
    }
    fn boxed_clone(&self) -> Box<dyn ExternalModel> { Box::new(self.clone()) }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
```

Attach with `grid.attach_model(Box::new(model))?`, or in a config as `"model": {"my_model": {...}}` alongside an empty rule. The model round-trips through `GridState` snapshots (derived state is rebuilt via `attach` on restore). Optional hooks: `event_applies` (gate long-range writes), `on_paint` (refresh derived state when the user paints), `declared_types` (painting palette), `work_per_cell` (parallel split sizing).

### The wildfire model

`wildfire::WildfireModel` implements Alexandridis-style stochastic spread: per-cell base probability `p0 × veg_factor × density`, exponential wind (`c1`, `c2`) and slope (`slope_a`) modifiers with a `1/√2` diagonal correction, burn duration tracked through cell ages, and lognormal firebrand spotting delivered as `ModelEvent`s. Slope factors are precomputed per cell at attach; wind factors once per chunk; the per-cell loop is two multiplies per burning neighbor plus one hash draw. See `configs/2d_wildfire_demo.json` and the module docs for parameters (defaults follow Alexandridis et al. 2008).

### Model parameters

A model can describe its own tunable values so a generic UI can build controls for it without knowing anything about the model. Three types in `external.rs` carry the description:

- **`ParamValue`** — the value itself, as read from or written to a control: `Float(f64)`, `Int(i64)`, `Bool(bool)`, or `Choice(String)`.
- **`ParamKind`** — what kind of control the value wants, and its legal range: `Float { min, max, step }`, `Int { min, max }`, `Bool`, or `Choice { options }`.
- **`ParamDesc`** — one row of self-description: `key` (the stable name used with `get_param`/`set_param`), `label`, an optional `group` (so a panel can put related controls under one heading), an optional `help` tooltip, an optional `unit` suffix (`"m/s"`, `"°"`), the `kind`, whether changing it needs `reattach` (see below), and whether it is `read_only`.

Three `ExternalModel` trait methods carry these around, and **all three have default implementations** — `params() -> Vec<ParamDesc>` defaults to an empty list, `get_param(&self, key) -> Option<ParamValue>` defaults to `None`, and `set_param(&mut self, key, value) -> Result<(), ModelError>` defaults to rejecting every key. A model that implements none of them keeps compiling and simply shows no controls.

Two rules come with those methods, and both are easy to trip over. First, **`get_param` must return `Some` for every key `params()` lists**: the engine's rollback puts back the value `get_param` reported, so a `reattach: true` key it will not answer has no way home — and `set_model_param` refuses to write such a key at all rather than strand the model. Second, **`set_param` validates nothing and rebuilds nothing on its own**; it is the raw write, so library callers should go through `Grid2D::set_model_param`, which checks the descriptor first and re-runs `attach` when the descriptor asks for it. Calling `set_param` directly is for a model that is not attached to a grid, such as the clone inside a snapshot, which is re-attached when it is restored.

`Grid2D::set_model_param(&mut self, key: &str, value: ParamValue) -> Result<(), ModelError>` is the engine-side half: it does the generic work so a model author does not have to write range checks. It looks up `key` in `model.params()` (unknown key or `read_only` → `Err`), checks `value` against that descriptor's `ParamKind` bounds itself (bounds check), reads the old value with `get_param` and refuses the whole write if a `reattach` parameter has none (there would be nothing to roll back to), calls `model.set_param(key, value)` (set), and — only when the descriptor says `reattach: true` — calls `model.attach` again to rebuild any derived state (reattach). If that `attach` call fails, `set_model_param` writes the old value back with another `set_param` and calls `attach` once more so the model ends up exactly as it was before the edit (rollback), then returns the original error.

A one-parameter model implementing all three methods:

```rust
fn params(&self) -> Vec<ParamDesc> { vec![ParamDesc { key: "speed".into(), label: "Speed".into(), group: None, help: None, unit: Some("m/s".into()), kind: ParamKind::Float { min: 0.0, max: 10.0, step: 0.1 }, reattach: false, read_only: false }] }
fn get_param(&self, key: &str) -> Option<ParamValue> { (key == "speed").then(|| ParamValue::Float(self.speed)) }
fn set_param(&mut self, key: &str, v: ParamValue) -> Result<(), ModelError> {
    match (key, v) { ("speed", ParamValue::Float(v)) => { self.speed = v; Ok(()) }, _ => Err(ModelError::InvalidParam(format!("unknown '{key}'"))) }
}
```

---

## Fast Paths (bit-parallel stepping)

Three optimizations kick in automatically when a rule or model has the right
shape. All of them are invisible except for speed: each one checks its
preconditions every step and falls back to the normal ("scalar") path when
they don't hold, and property tests pin their output to be exactly identical
to the scalar path — cells, ages, history, and counts.

**1D packed Wolfram** (`Grid1D::step_packed`, detected by `Rule1DPlan`).
When a 1D rule is a classic two-state Wolfram automaton (two subrules sharing
one transition table — see `PackedWolfram` in `rules.rs`), the row is stored
one *bit* per cell inside 64-bit integers. Shifting a word left/right hands
all 64 cells their left/right neighbors at once, and the 8-entry transition
table becomes a handful of AND/OR operations per word. Roughly 40 % faster on
the rule-30 benchmarks.

**2D bit-plane threshold** (`Grid2D::step_packed`, detected by `Rule2DPlan`).
When a 2D rule is "life-like" (two cell types, every subrule counting the same
type over one shared radius-1 neighborhood — see `PackedThreshold2D`), the
next state only depends on (current state, neighbor count): 18 possible
situations, precomputed into a table. The grid is packed one bit per cell;
neighbor counts for 64 cells at a time are built by adding eight shifted words
with schoolbook binary carries, then the table is applied with bitwise masks.
Cuts the 256×256 Life benchmark by ~59 % — faster single-threaded than the
old 8-thread scalar path.

**Wildfire fire-front mask** (`WildfireModel::step_chunk`). The wildfire
model can't use a count table (its ignition math depends on *which* neighbors
burn, with per-direction wind and per-cell slope factors), but fire only
moves at its edges: a cell can only change if it is Burning or touches a
Burning cell. The step copies the grid through as the default, builds a
Burning bitmap, ORs its eight shifts into a "might change" mask, and runs the
full per-cell math only for those cells — a thin front line instead of the
whole grid. Roughly halves the wildfire benchmarks. Skipped cells never
consumed randomness, so the stochastic output is bit-identical.

The performance history and the experiment log behind these (including the
approaches that were tried and made things *worse*) live in
`docs/performance.md` §8.

---

## Parallelism and Threading

Both `Grid1D` and `Grid2D` stepping are parallelized to take advantage of multi-core CPUs.

### Configuration
The library looks for a `cella.properties` file in the current directory or up to five parent directories:
```properties
threads=8
```
- If the file is missing or the key is not set, it defaults to `std::thread::available_parallelism()` (or 1 on error). The resolved value is cached for the process.
- Set `threads=1` to force single-threaded execution (useful for debugging).
- Tests and benchmarks can use `threads::set_thread_override(n)` / `clear_thread_override()` for a process-local override.

### Chunk sizing is driven by work, not grid size

Waking a parked worker costs real time (tens of microseconds on some platforms), so the split is sized by *estimated work*, not by cell count:

```
nchunks = clamp(total_work / MIN_WORK_PER_CHUNK, 1, thread_count())
```

with `threads::MIN_WORK_PER_CHUNK = 400_000` nominal neighbor visits. `total_work` is `cells × work_per_cell`, where `work_per_cell` is the sum of neighborhood sizes over subrules (2D) or of `2n + 1` window widths (1D). It is an upper bound — it ignores early exit and non-matching subrules — which is the safe direction: it never promotes a grid that is too small to parallelize.

`nchunks <= 1` runs the step serially on the calling thread with no pool involvement. This replaced the old flat `width >= 8192` (1D) / `width * height >= 4096` (2D) thresholds; the `history_limit > 0` precondition is also gone, since `split_chunks` now handles empty history buffers.

`threads::set_min_work_per_chunk_override(n)` / `clear_min_work_per_chunk_override()` lower the threshold so tests can force the multi-threaded path on grids small enough to check exhaustively.

### Implementation
`step()` calls `chunking::split_chunks` to carve the output buffers (`next_cells`, `ages`, and the three history arrays) into disjoint per-worker `OutChunk`s, then runs `step_chunk` over them with `par_iter_mut` inside a **persistent, size-keyed rayon pool** (`threads::pool(n)`). Workers read the shared `cells` buffer and write only their own slices — no locking on cell data. Each chunk returns a `TypeCounter` (a small `Vec`-backed counter, no `HashMap` allocation) and the results are `reduce`d by merging.

Pools are built on first use per thread count and live for the process, so stepping only pays fork/join on already-parked workers rather than a fresh `std::thread::scope` spawn per step.

---

## Serialization and Configs

### JSON Configuration
The `CellaConfig` enum (in `config.rs`) provides a unified way to load 1D and 2D setups from JSON.

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

`initial` is a flat array of type *names* with length `width` (1D) or `width * height` (2D).

```rust
use cella_lib::config::CellaConfig;
let cfg = CellaConfig::from_file("configs/life.json")?;
let mut grid = cfg.build_grid2d().expect("2d config with matching initial length");
cfg.to_file_pretty("out.json")?;
```

`build_grid1d` / `build_grid2d` return `None` on a dimension mismatch (wrong variant, or `initial.len()` not matching the declared size).

### Snapshots
`GridState` can be used to capture the current state of a running grid:
```rust
let state = GridState::from_grid2d(&grid);
let json = state.to_json_pretty();
let restored = Grid2D::from_state(&GridState::from_json(&json)?).unwrap();
```

`CellType` serializes as its string name, and the SoA grid internals round-trip through `CellState` vectors, so snapshots remain human-readable JSON. `GridState` carries `counts_current` / `peak_counts` as `HashMap<String, u64>`; both default to empty on read, in which case `from_state` recounts from the cells. `grid2d_to_json(&grid)` is a shorthand for the pretty snapshot.

---

## Building and Testing

The workspace `Makefile` wraps the common cargo invocations:

| Target | Does |
| --- | --- |
| `make build` / `make build-release` | build the `cella` binary |
| `make run` / `make run-gui` | run CLI mode / GUI at 1024x768 |
| `make check` / `make clippy` / `make fmt` / `make fmt-check` | workspace lint & format (clippy runs with `-D warnings`) |
| `make doc` | `cargo doc --workspace --no-deps --open` |
| `make test` | fast pass — non-ignored `cella_lib` tests |
| `make test-all` | includes `#[ignore]`d long-running tests |
| `make test-all-single` | same, `--test-threads=1` (needed for the benchmark summary ordering) |
| `make coverage` / `make coverage-all` | `cargo llvm-cov --html` for `cella_lib` |
| `make test-create-snapshots` | regenerate `cella_lib/tests/snapshots/*.txt` |
| `make test-update-benchmarks` | rewrite `cella_lib/tests/benchmarks_last.json` |

Environment variables the suite reads:

- `CELLA_ASCII=1` — write ASCII renders of the initial and final grids (off by default; the Makefile pins it to `0`).
- `CELLA_UPDATE_SNAPSHOTS=1` — write snapshot hashes instead of asserting them.
- `CELLA_UPDATE_BENCH=1` — write benchmark baselines instead of comparing.
- `CELLA_BENCH=1` — print per-test timings as they complete.
- `CELLA_BENCH_RUNS=N` — repeats per benchmark (default 10). Timings are recorded per thread count (`<name>_t<threads>`).
- `CELLA_EXPORT_CONFIGS=1` — write a JSON config file for each scenario the tests build.

Integration tests live in `cella_lib/tests/`: `config_tests.rs` (JSON round-trips), `edge_cases.rs` (validation boundaries), `randomness.rs`, `soa_robust.rs` (history circular buffer + serial/parallel agreement), and `long_suite.rs` (ignored-by-default stress, snapshots, benchmarks).

---

## Performance

See [performance.md](performance.md) for a review of the engine's current optimizations (SoA layout, double buffering, interned types, incremental counting, interior/edge split, work-sized chunking on a persistent pool), known issues, recommended improvements, and a discussion of the Hashlife algorithm as a potential future direction.

---

## Rustdoc Integration

This library is fully documented with Rustdoc. You can generate and view the HTML documentation by running:
```bash
cargo doc --open -p cella_lib
# or, for the whole workspace:
make doc
```
This will include detailed API references for all public structs, enums, and methods, including usage examples.
