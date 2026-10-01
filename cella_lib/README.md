# cella_lib

`cella_lib` is the engine behind [Cella](https://github.com/wcole3/cella), a
Rust toolkit for cellular automata (CA). A CA is a grid of cells that change
step by step according to simple local rules, like Conway's Game of Life.

This crate is the library only. The GUI workbench and CLI demo menu live in
the [main repository](https://github.com/wcole3/cella) and are built on top
of this crate.

The library has three parts, each usable on its own:

- **An engine** for 1D and 2D rule-based CAs, with multi-threaded stepping
  and JSON configs you can save, share and resume.
- **A plugin seam** (`ExternalModel`) for replacing the built-in rules with
  your own transition code. A wildfire spread model ships as the worked
  example.
- **An Explore module** that runs any rule or model many times
  (ensembles), searches for good settings (evolution) and maps the range of
  behaviors a rule family can produce (MAP-Elites).

**Wildfire disclaimer.** The wildfire model (`cella_lib::wildfire`) and the
scoring tools in the repository's
[`validation/`](https://github.com/wcole3/cella/blob/main/validation/README.md)
folder are a personal-interest project. They must not be used to model or
predict real fires.

---

## Features

- 1D Wolfram-style rules (any code, any radius) and 2D neighborhood-count
  rules (Moore, Von Neumann, Langton, StraightLine, Knight), with
  multi-state cells and optional per-rule randomness.
- Deterministic results: the same config and seed give the same run, on any
  thread count.
- JSON configs: load, save, and resume a run part-way through.
- Plugin models: implement `ExternalModel` in your own crate and tag it
  with `typetag`. Configs then load it by name, and the engine and Explore
  drive it with no changes to `cella_lib`.
- Explore: Monte Carlo ensembles (probability maps), genetic-algorithm
  evolution, novelty search and MAP-Elites, for any rule or model.

---

## Installation

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
cella_lib = "1.0"
```

To use a local checkout of the repository instead, point `path` at the
`cella_lib` folder inside it:

```toml
[dependencies]
cella_lib = { path = "../cella/cella_lib" }
```

---

## Examples

Both samples below are checked to compile against the current API.

### Conway's Game of Life from a config

This sample reads `configs/life.json` from the
[main repository](https://github.com/wcole3/cella/tree/main/configs), so run
it from the repository root (or copy that file next to your program).

```rust
use cella_lib::*;
use cella_lib::config::CellaConfig;

fn main() {
    // Load a pre-made config
    let cfg = CellaConfig::from_file("configs/life.json").unwrap();
    let mut grid = cfg.build_grid2d().unwrap();
    // Capture the Reset target before stepping: the grid's own starting
    // cells, at step 0.
    let initial = GridState::from_grid2d(&grid);

    for _ in 0..50 {
        grid.step();
    }
    // counts_current is keyed by the interned type handle (Spur)
    let alive = CellType::from("Alive");
    println!("Step {} — Alive cells: {}",
        grid.step,
        grid.counts_current.get(&alive.0).unwrap_or(&0));

    // Save for later: the file is a config again (same shape as life.json),
    // plus a `snapshot` block holding the run in progress since we're past
    // step 0. Reset still works after loading this back, because `initial`
    // is the cells captured above, not wherever `grid` ended up.
    let out = CellaConfig::save_2d(&initial, &grid, Default::default());
    out.to_file_pretty("snapshot.json").unwrap();
}
```

### 1D Rule 30 in the terminal

This one builds the rule in code, so it needs no files.

```rust
use cella_lib::*;

fn main() {
    // CellType is an interned symbol — construct via From<&str>
    let x = CellType::from("X");
    let inactive = CellType::inactive();

    let rule = Rule1D { subrules: vec![
        Rule1DSubrule {
            current_type: x.clone(), criteria_type: x.clone(),
            wolfram_code: 30, n: 1, randomness: None,
            output_type: x.clone(),
        },
        Rule1DSubrule {
            current_type: inactive.clone(), criteria_type: x.clone(),
            wolfram_code: 30, n: 1, randomness: None,
            output_type: x.clone(),
        },
    ]};

    let width = 81;
    let mut init = vec![inactive; width];
    init[width / 2] = x.clone();

    let mut grid = Grid1D::new(width, 5, init, rule);
    for _ in 0..40 {
        grid.step();
        let line: String = (0..width)
            .map(|i| if grid.cell_type(i) == x { '#' } else { '.' })
            .collect();
        println!("{}", line);
    }
}
```

### Your own model

To plug in your own transition code, implement the `ExternalModel` trait
(in `src/external.rs`). The shipped example is the wildfire model in
`src/wildfire/`. It is not part of the engine and is not re-exported at the
crate root; reach it by name, for example
`use cella_lib::wildfire::WildfireModel;`. The full walkthrough is in
[docs/lib.md, "External Models"](https://github.com/wcole3/cella/blob/main/docs/lib.md).

---

## Thread count

The engine steps grids in parallel. To set the worker count, create a
`cella.properties` file in the directory you run your program from:

```properties
# Number of worker threads for grid stepping. 1 disables parallelism.
threads=4
```

Without the file, the engine uses `std::thread::available_parallelism()`,
usually the number of logical CPUs. The thread count never changes results,
only speed.

---

## Config format

A config is a JSON file with a `"dim"` field (`"1d"` or `"2d"`), a size, an
`initial` list of cell types, and a `rule` made of subrules. A subrule says
"a cell of type A, seeing at least N neighbors of type B, becomes type C".
Load one with `CellaConfig::from_file`, then call `build_grid1d` or
`build_grid2d`.

Here is Game of Life (the `"..."` entries stand in for the full cell list):

```json
{
  "dim": "2d",
  "width": 10,
  "height": 10,
  "history_limit": 5,
  "initial": ["Inactive","Inactive","...","Alive","Alive","Alive","..."],
  "rule": {
    "subrules": [
      { "current_type":"Alive", "criteria_type":"Alive", "count":4,
        "op":"gt", "range":1, "neighborhood":"Moore",
        "randomness":null, "output_type":"Inactive" },
      { "current_type":"Alive", "criteria_type":"Alive", "count":2,
        "op":"gt", "range":1, "neighborhood":"Moore",
        "randomness":null, "output_type":"Alive" },
      { "current_type":"Inactive", "criteria_type":"Alive", "count":3,
        "op":"eq", "range":1, "neighborhood":"Moore",
        "randomness":null, "output_type":"Alive" }
    ]
  }
}
```

Things that commonly trip people up:

- `op` is `"lt"`, `"gt"` or `"eq"`. `gt` and `lt` are **inclusive**: they
  mean "at least" and "at most".
- An optional `"limit"` turns `gt`/`lt` into an inclusive range. For
  example `"count":2, "op":"gt", "limit":3` means "2 to 3 neighbors".
- In JSON, a 1D rule's `wolfram_code` is a string (`"30"`); in the Rust API
  it is a number.

Ready-made configs (Life, Rule 30, multi-state cycles, wildfire demos,
Explore setups) are in the repository's
[`configs/`](https://github.com/wcole3/cella/tree/main/configs) folder. The
root README has
[a 1D example and more field notes](https://github.com/wcole3/cella#config-format),
and the full field guide (including the optional `model`, `colors`,
`snapshot`, `ensemble` and `evolve` blocks) is in
[docs/app.md, "Config file guide"](https://github.com/wcole3/cella/blob/main/docs/app.md#config-file-guide).

---

## Bundled examples

The crate ships runnable programs in `examples/`. Run them from inside the
`cella_lib/` folder:

```bash
cd cella_lib
cargo run --release --example explore -- <config.json> ensemble <steps> [report.json]
```

| Example | What it does |
|---------|--------------|
| `explore` | Runs the `ensemble` or `evolve` block of any config from the command line |
| `wildfire_validate` | Scores the wildfire model against an observed fire |
| `wildfire_smc` | Ensemble forecasting for the wildfire model, built on `Ensemble` |
| `wildfire_ros` | Measures the wildfire model's rate of spread |
| `wildfire_experiment` | Investigation-only variant of `wildfire_validate` |

---

## Module overview

| Module | Contents |
|--------|----------|
| `grid1d`, `grid2d` | The two grid types and their `step()` |
| `rules` | 1D Wolfram-style and 2D neighborhood-count rules |
| `types` | `CellType` (interned cell-type name) and `CellState` |
| `config` | `CellaConfig`: load, save and resume JSON configs |
| `state` | `GridState`: in-memory snapshot of a grid |
| `external` | The `ExternalModel` plugin trait |
| `explore` | Ensembles, evolution, novelty search, MAP-Elites |
| `threads` | Reads the thread count from `cella.properties` |
| `wildfire` | Worked-example plugin model (not engine code) |

The full module reference is in
[docs/lib.md](https://github.com/wcole3/cella/blob/main/docs/lib.md). For
generated API docs, run `cargo doc --open` inside `cella_lib/`.

---

## Building and testing

`cella_lib` is its own Cargo build root: the root `Cargo.toml` of the
repository depends on it by path but does not list it as a workspace
member. So library commands run from inside this folder:

```bash
cd cella_lib
cargo test                          # fast library tests
cargo test --example wildfire_smc   # the wildfire_smc example's own tests
cargo test -- --include-ignored     # also the slow #[ignore]d tests
cargo clippy --examples -- -D warnings  # lint the examples
```

The example's tests need their own command because `cargo test` builds
examples but does not run their tests. From the repository root,
`make test`, `make test-all` and `make clippy` run all of this for you and
also cover the binary. More on test environment variables, snapshots and
benchmarks is in
[docs/lib.md, "Building and testing"](https://github.com/wcole3/cella/blob/main/docs/lib.md#building-and-testing).

---

## More documentation

| Document | Contents |
|----------|----------|
| [Library guide](https://github.com/wcole3/cella/blob/main/docs/lib.md) | Architecture, module reference, rule system, external models, serialization, threading |
| [Explore guide](https://github.com/wcole3/cella/blob/main/docs/explore.md) | Ensembles, evolution and MAP-Elites; how to read results honestly |
| [Primer: Monte Carlo](https://github.com/wcole3/cella/blob/main/docs/primer-monte-carlo.md) | Plain-language: why run a simulation many times |
| [Primer: Genetic Algorithms](https://github.com/wcole3/cella/blob/main/docs/primer-genetic-algorithms.md) | Plain-language: populations, selection, mutation, MAP-Elites |
| [Performance review](https://github.com/wcole3/cella/blob/main/docs/performance.md) | Engine internals, optimizations, known issues |

---

## License

MIT — see [LICENSE](https://github.com/wcole3/cella/blob/main/LICENSE).
