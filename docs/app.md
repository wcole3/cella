# Cella Application Documentation (`cella`)

The `cella` binary provides both a command-line interface (CLI) for quick demos and a rich graphical user interface (GUI) for interactive exploration of cellular automata.

## Command-Line Interface (CLI)

The CLI is the default mode when running the application without the `--gui` flag. It provides a menu-driven interface to explore various pre-defined simulations.

### Basic Usage

```bash
cargo run --release
```

When you run this command, you will see a numbered list of demos:
1.  **2D Game of Life (approx)**: A standard Conway-style implementation.
2.  **1D Wolfram Rule 30 (n=1)**: The classic elementary CA.
3.  **1D Wolfram n=2 demo**: Demonstrates a larger neighborhood radius.
4.  **1D Custom**: Enter your own Wolfram code and radius.
5.  **Load from configuration file (JSON)**: Load a custom config.
6.  **Langton's ant**: Placeholder — not yet implemented (requires a moving agent).
7.  **1D three-state cycle demo**: Multi-state cycling in 1D.
8.  **2D three-state cycle demo**: Multi-state cycling in 2D.
9.  **2D StraightLine neighborhood demo**: Cardinal-direction neighborhoods.
0.  **Exit**.

### Loading Custom Configs via CLI

You can use the CLI to run any JSON configuration file found in the `configs/` directory or elsewhere:
1.  Select option `5`.
2.  Provide the path (e.g., `configs/life.json`).
3.  Specify the number of steps to simulate.

The CLI will output the results (for 1D) or summary statistics (for 2D) to the terminal.

---

## Graphical User Interface (GUI)

The GUI is built using the `egui` framework and provides a powerful set of tools for creating and analyzing CA.

### Launching the GUI

```bash
cargo run --release -- --gui
```

#### Window Size
You can specify the initial window size using command-line arguments:
- `--size=1280x720`
- `--width=1920 --height=1080`
- If only one dimension is provided, the other defaults to a 16:9 ratio.

### Code Layout (Module Map)

The GUI source lives in `src/gui/`. Each file owns one concern, so you can
usually tell where a change belongs without reading the whole tree. If you are
adding something, put it in the file whose description matches — and if nothing
matches, that is a hint the thing deserves its own module.

| File | What lives here |
|---|---|
| `gui.rs` | Module list and the single `pub use app::run_gui` the binary calls. Nothing else. |
| `gui/app.rs` | The `CellaApp` struct and the `eframe::App` impl. The `ui` method is deliberately short: it only says which panel is drawn in what order. |
| `gui/state.rs` | The nine structs `CellaApp` is made of (`Scenario`, `Playback`, `ViewSettings`, …), each with the app's starting values in its `Default`. Add a new field to the group it belongs to, not to `CellaApp`. |
| `gui/painter.rs` | Turning grid cells into rectangles, including the run-merging that keeps large grids cheap to draw. |
| `gui/interact.rs` | Mouse and keyboard gestures on the viewport: zoom, pan, paint, click-to-cycle, undo. Also `cell_index_at`, the "which cell was clicked?" arithmetic. |
| `gui/sim.rs` | Stepping the simulation and recording statistics. The playback clock. |
| `gui/scenarios.rs` | Loading demos and JSON configs, resizing the grid, resetting to the initial state. |
| `gui/types.rs` | Cell-type helpers: which states a scenario declares, what order to list them in, what a click cycles to next. |
| `gui/export.rs` | Writing GIFs and JSON snapshots, plus the file dialogs that start them. |
| `gui/render.rs` | The colour palette and the single function that maps a cell type to a colour, shared by the viewport and the GIF exporter so the two cannot disagree. |
| `gui/panels/` | One file per region of the window — `toolbar`, `scenario`, `rule_editor`, `colors`, `statistics`, `model`. |
| `gui/panels/widgets.rs` | Controls used by more than one panel, such as the cell-type picker. Reach for this before hand-rolling a widget a second time. |
| `gui/panels/rule_edit_model.rs` | The rule editor's working copy of a rule, held as text so half-typed values are legal until "Apply to grid" converts them. |

### GUI Features

#### Viewport
- **Panning**: Click and drag with the middle mouse button (or left-click on empty space) to move the grid.
- **Zooming**: Use the mouse wheel to zoom in and out of the grid.
- **Painting**: Left-click on cells to toggle their state or "paint" with the currently selected `CellType`.

#### Control Panel (Left Side)
- **Simulation Controls**: Play/Pause, Step, and Reset.
- **Speed Slider**: Control the simulation speed (steps per second).
- **Preset Menu**: Quickly load built-in demos like Game of Life, Rule 30, and specialized neighborhoods.
- **Grid Settings**: Change grid width, height, and history limit on the fly.
- **Color Pickers**: Customize the colors for each `CellType` in the simulation.
- **Rules Editor**:
    - View and modify existing subrules.
    - Change neighborhood shapes (Moore, VonNeumann, Langton, StraightLine, Knight), ranges, and thresholds.
    - Add new subrules to create complex multi-state automata.

#### Statistics Panel (Left Side)
- **Population Counts**: Live counters for each cell type.
- **Peak Counts**: Tracks the maximum population reached for each type.
- **History Charts**: View live line graphs of population changes over time.

#### Model Panel (Left Side)
This panel only appears when the loaded config has a `model` (e.g.
`configs/2d_wildfire_demo.json`) — a scenario with no model, like Game of
Life, shows nothing here. The section starts expanded, and its controls are
grouped under headings the model supplies (for the wildfire model: Wind, Fire,
Terrain, and Spotting when spotting is turned on). Cheap parameters commit as
soon as you move their slider, so the effect looks live; a handful of
expensive ones wait until the gesture is over — you release the slider, tap an
arrow key, or click away — so an internal rebuild only happens once per edit
instead of once per frame. Opening the panel changes nothing by itself: a
value is only written when it actually differs from the one the model already
holds. A read-only parameter, such as the seed, shows as a plain label instead
of a control — there is nothing to drag.

The controls keep you inside each parameter's allowed range: a slider stops at
its end stops, and a value you type is pulled back into range before the panel
sees it, so an out-of-range number never reaches the model. A parameter error
in the status bar therefore means something rarer — the model's own validation
refused a value its own descriptor said was allowed. No wildfire parameter
does that today (a test pins every descriptor's end stops to values the model
accepts), so in practice you will not see one.

Pressing **Reset** rewinds the grid but keeps the values you set with the
sliders — an accepted edit is mirrored into the same snapshot Reset restores
from, so tuning a model and then resetting the cells does not also undo your
tuning.

While you step or play normally, the history chart gains one point per step. A
"Run to +N" run is different: it packs as many steps as it can into each drawn
frame, and the chart gains one point per *frame* instead. Nothing useful is
lost — the chart keeps a rolling window of the most recent samples
(`StatsState::window_len`, 300 by default), so a run of thousands of steps would
have thrown away all but the last few hundred points anyway, and recording them
only to discard them slowed the run down. The population and peak counters above
the chart are not affected at all: `ui_statistics` reads `counts_current` and
`peak_counts` straight off the grid, so they are exact after every step no matter
how the steps were paced.

---

## GIF Export

The GUI allows you to record your simulation and export it as an animated GIF.

1.  Pause the simulation.
2.  Open the **Export** section in the control panel.
3.  Choose the **Frame Delay** (ms) and **Scale** (pixels per cell).
4.  Toggle **Recording** to "On".
5.  Press **Play** or **Step** to capture frames.
6.  When finished, toggle **Recording** to "Off".
7.  Provide a filename and click **Save GIF**.

---

## Configuration Guide

The application uses a common JSON format for storing both rules and grid states.

### `dim` Discriminator
Every configuration must have a `"dim"` field, which is either `"1d"` or `"2d"`.

### Example: Rule 30 (1D)
```json
{
  "dim": "1d",
  "width": 257,
  "history_limit": 4,
  "initial": ["Inactive","...","X","...","Inactive"],
  "rule": {
    "subrules": [
      {
        "current_type": "X",
        "criteria_type": "X",
        "wolfram_code": "30",
        "n": 1,
        "randomness": null,
        "output_type": "X"
      }
    ]
  }
}
```

### Example: Custom 2D Rule
```json
{
  "dim": "2d",
  "width": 100,
  "height": 100,
  "rule": {
    "subrules": [
      {
        "current_type": "Alive",
        "criteria_type": "Alive",
        "count": 3,
        "op": "eq",
        "range": 1,
        "neighborhood": "Moore",
        "output_type": "Alive"
      }
    ]
  }
}
```

Notes on 2D subrule fields:

- `op` is one of `"lt"`, `"gt"`, `"eq"`. The `gt`/`lt` comparisons are **inclusive** (`gt` = "at least `count`", `lt` = "at most `count`").
- `neighborhood` is one of `"Moore"`, `"VonNeumann"`, `"Langton"`, `"StraightLine"`, `"Knight"`.
- An optional `"limit"` field turns `gt`/`lt` into an inclusive between-range. For example, "survive with 2 to 3 neighbors":

```json
{
  "current_type": "Alive",
  "criteria_type": "Alive",
  "count": 2,
  "op": "gt",
  "limit": 3,
  "range": 1,
  "neighborhood": "Moore",
  "output_type": "Alive"
}
```

- `"randomness"` (optional, `0.0`–`1.0`) is the probability that a matching subrule is skipped. Both `"limit"` and `"randomness"` may be omitted entirely.

For more examples, see the `configs/` directory.
