# Cella Library Documentation (`cella_lib`)

`cella_lib` is the core engine for the Cella cellular automata project. It provides the data structures and logic for simulating 1D and 2D automata, handling rules, and managing grid state.

## Architecture Overview

The library is designed with a focus on:
- **Composability**: Rules are built from multiple "subrules" that are evaluated in order.
- **Performance**: 2D grid stepping is parallelized using standard library scoped threads.
- **Serializability**: Grids and rules can be easily converted to and from JSON using `serde`.

### Core Components

- **`CellType`**: A thin wrapper around a `String` that represents a semantic state (e.g., "Alive", "Dead", "X", "Inactive").
- **`CellState`**: Tracks a cell's current type, how long it has been in that state (`age_in_state`), and its history.
- **`Grid1D` / `Grid2D`**: The primary containers for simulation. They hold the cells, the rules, and the current step count.
- **`Rule1D` / `Rule2D`**: Contain lists of subrules that define how cells transition between states.
- **`GridState`**: A flat, serializable representation of a grid's current configuration, useful for snapshots.

---

## The Rule System

Cella uses a priority-based subrule system. When `step()` is called, each cell evaluates the subrules in its assigned `Rule` set one by one. The first subrule that matches determines the cell's next state. If no subrule matches, the cell becomes `Inactive`.

### 1D Rules (`Rule1D`)

1D rules use **Wolfram-style codes**. A subrule defines:
- `current_type`: The type the cell must have to match.
- `criteria_type`: The type considered "active" when building the neighborhood bitmask.
- `wolfram_code`: A bitmask (up to `u128`) representing the transition table.
- `n`: The neighborhood radius. The window size is `2n + 1`.

For `n=1` (radius 1), the window is 3 cells. There are 8 ($2^3$) possible patterns. Rule 30 corresponds to the bitmask `00011110` in binary.

### 2D Rules (`Rule2D`)

2D rules use **Threshold-based neighborhoods**. A subrule defines:
- `current_type`: The cell's required current state.
- `criteria_type`: The type of neighbors to count.
- `count`: The threshold value.
- `op`: The comparison operator (`Lt`, `Gt`, `Eq`).
- `range`: The radius of the neighborhood.
- `neighborhood`: The shape of the neighborhood (`Moore`, `VonNeumann`, `Langdon`, `StraightLine`).

#### Neighborhood Shapes
- **Moore**: A square area around the cell (e.g., $3 \times 3$ for range 1).
- **Von Neumann**: A diamond shape (cardinal neighbors only).
- **Langdon**: Diagonal neighbors only ($|dx| = |dy|$).
- **StraightLine**: Cardinal lines extending out to `range` distance.

---

## Parallelism and Threading

2D grid updates are parallelized to take advantage of multi-core CPUs.

### Configuration
The library looks for a `cella.properties` file in the root of the project:
```properties
threads=8
```
- If the file is missing or the key is not set, it defaults to the number of available logical cores.
- Set `threads=1` to force single-threaded execution (useful for debugging).

### Implementation
The `step()` method in `Grid2D` splits the grid into horizontal chunks. Each chunk is processed by a separate thread using `std::thread::scope`, ensuring memory safety without the overhead of reference counting for the grid data.

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

---

## Rustdoc Integration

This library is fully documented with Rustdoc. You can generate and view the HTML documentation by running:
```bash
cargo doc --open -p cella_lib
```
This will include detailed API references for all public structs, enums, and methods, including usage examples.
