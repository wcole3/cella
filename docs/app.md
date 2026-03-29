# Cella Application Documentation (`cella`)

The `cella` binary provides both a command-line interface (CLI) for quick demos and a rich graphical user interface (GUI) for interactive exploration of cellular automata.

## Command-Line Interface (CLI)

The CLI is the default mode when running the application without the `--gui` flag. It provides a menu-driven interface to explore various pre-defined simulations.

### Basic Usage

```bash
cargo run --release
```

When you run this command, you will see a numbered list of demos:
1.  **2D Game of Life**: A standard Conway-style implementation.
2.  **1D Wolfram Rule 30**: The classic elementary CA (radius n=1).
3.  **1D Wolfram n=2**: Demonstrates a larger neighborhood radius (n=2).
4.  **1D Custom**: Allows you to enter your own Wolfram code and radius.
5.  **Load from config**: Load a custom JSON configuration file.
6.  **Cycle Demos**: 1D and 2D demos showing multi-state cycles (options 7 and 8).
7.  **StraightLine Neighborhood**: A 2D demo using cardinal directions (option 9).

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
    - Change neighborhood shapes, ranges, and thresholds.
    - Add new subrules to create complex multi-state automata.

#### Statistics Panel (Right Side)
- **Population Counts**: Live counters for each cell type.
- **Peak Counts**: Tracks the maximum population reached for each type.
- **History Charts**: View live line graphs of population changes over time.

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

For more examples, see the `configs/` directory.
