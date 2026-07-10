# Cella Library Documentation (`cella_lib`)

`cella_lib` is the core engine for the Cella cellular automata project. It provides the data structures and logic for simulating 1D and 2D automata, handling rules, and managing grid state.

## Architecture Overview

The library is designed with a focus on:
- **Composability**: Rules are built from multiple "subrules" that are evaluated in order.
- **Performance**: Grids store cell data as struct-of-arrays with double-buffered stepping; both 1D and 2D stepping parallelize with standard library scoped threads above a size threshold. See [performance.md](performance.md) for a detailed review.
- **Serializability**: Grids and rules can be easily converted to and from JSON using `serde`.

### Core Components

- **`CellType`**: An interned symbol identifying a semantic state (e.g., "Alive", "Dead", "X", "Inactive"). Internally it wraps a `lasso2::Spur` — a 4-byte handle into a global string interner — so it is `Copy` and comparisons in the stepping hot path are single integer compares. Create one with `CellType::from("Alive")` or `CellType::new("Alive")`; get the name back with `.as_str()`.
- **`CellState`**: A per-cell snapshot (current type, `age_in_state`, bounded history). Used as a serialization intermediate; the live grids do not store `CellState`s.
- **`Grid1D` / `Grid2D`**: The primary simulation containers. Internally they use a struct-of-arrays layout: flat `Vec`s for current types, ages, and a per-cell circular history buffer, plus a second cell buffer for double-buffered stepping (`history_limit` must be ≤ 255).
- **`Rule1D` / `Rule2D`**: Contain lists of subrules that define how cells transition between states.
- **`GridState`**: A flat, serializable representation of a grid's current configuration, useful for snapshots.

### Population counts

Each grid maintains `counts_current` and `peak_counts` as `HashMap<Spur, u64>` — keyed by the interned type handle, not by `String`. To look up a count by name:

```rust
use cella_lib::CellType;
let alive = CellType::from("Alive");
let n = grid.counts_current.get(&alive.0).copied().unwrap_or(0);
```

Counts are maintained incrementally during stepping; the dominant (most common) type is counted by subtraction rather than per cell.

---

## The Rule System

Cella uses a priority-based subrule system. When `step()` is called, each cell evaluates the subrules in its assigned `Rule` set one by one. The first subrule that matches determines the cell's next state. If no subrule matches, the cell becomes `Inactive`.

### 1D Rules (`Rule1D`)

1D rules use **Wolfram-style codes**. A subrule defines:
- `current_type`: The type the cell must have to match.
- `criteria_type`: The type considered "active" when building the neighborhood bitmask.
- `wolfram_code`: A bitmask (up to `u128`) representing the transition table.
- `n`: The neighborhood radius (1–3). The window size is `2n + 1`.
- `randomness`: Optional probability in `[0, 1]` that a matching subrule is skipped.

For `n=1` (radius 1), the window is 3 cells. There are 8 ($2^3$) possible patterns. Rule 30 corresponds to the bitmask `00011110` in binary. `n` up to 3 (128 patterns) is supported; `validate()` rejects larger radii.

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
- `range`: The radius of the neighborhood.
- `neighborhood`: The shape of the neighborhood (`Moore`, `VonNeumann`, `Langton`, `StraightLine`, `Knight`).
- `randomness`: Optional probability in `[0, 1]` that a matching subrule is skipped.

Construct subrules with `Rule2DSubrule::new(current, criteria, count, op, range, neighborhood, output, randomness, limit)` — this precomputes the neighborhood offsets used during stepping (they are also rederived automatically on deserialization).

#### Neighborhood Shapes
- **Moore**: A square area around the cell (e.g., $3 \times 3$ for range 1).
- **VonNeumann**: A diamond shape (Manhattan distance ≤ range).
- **Langton**: Diagonal neighbors only ($|dx| = |dy| \le$ range).
- **StraightLine**: Cardinal lines extending out to `range` distance.
- **Knight**: All cells reachable in at most `range` chess-knight hops (each hop ±1/±2 or ±2/±1). `range=1` gives exactly the 8 classic knight squares.

---

## Parallelism and Threading

Both `Grid1D` and `Grid2D` stepping are parallelized to take advantage of multi-core CPUs.

### Configuration
The library looks for a `cella.properties` file in the current directory or up to five parent directories:
```properties
threads=8
```
- If the file is missing or the key is not set, it defaults to the number of available logical cores.
- Set `threads=1` to force single-threaded execution (useful for debugging).
- Tests and benchmarks can use `threads::set_thread_override(n)` / `clear_thread_override()` for a process-local override.

### Implementation
`step()` splits the grid into contiguous chunks. Each chunk is processed by a separate thread using `std::thread::scope`, writing into disjoint slices of the double buffer — no locking on cell data. Small grids fall back to single-threaded stepping: 1D requires `width >= 8192` and 2D requires `width * height >= 4096` (and in both cases `history_limit > 0`) before threads are used.

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
  "initial": [...],
  "rule": {
    "subrules": [...]
  }
}
```

### Snapshots
`GridState` can be used to capture the current state of a running grid:
```rust
let state = GridState::from_grid2d(&grid);
let json = state.to_json_pretty();
```

`CellType` serializes as its string name, and the SoA grid internals round-trip through `CellState` vectors, so snapshots remain human-readable JSON.

---

## Performance

See [performance.md](performance.md) for a review of the engine's current optimizations (SoA layout, double buffering, interned types, incremental counting, chunked parallelism), known issues, recommended improvements, and a discussion of the Hashlife algorithm as a potential future direction.

---

## Rustdoc Integration

This library is fully documented with Rustdoc. You can generate and view the HTML documentation by running:
```bash
cargo doc --open -p cella_lib
```
This will include detailed API references for all public structs, enums, and methods, including usage examples.
