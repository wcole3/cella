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

#### Opening a config at startup
```bash
cargo run --release -- --gui --config configs/2d_wildfire_demo.json
```
`--config PATH` (or `--config=PATH`) opens that file instead of the Life demo.
It is the same as pressing "Load Config JSON..." and picking the file — handy
on machines where no file dialog can open (see Troubleshooting below).

#### Troubleshooting: "Load Config JSON..." does nothing
The app does not draw its own file dialog. It asks the desktop for one, through
the `rfd` crate. On Linux that means: talk to `xdg-desktop-portal` over the
D-Bus *session* bus, and if that fails, run the `zenity` program. When neither
is available, `rfd` gives up in a few milliseconds and reports "no file chosen"
— exactly what pressing Cancel reports — so the button looks dead. The status
bar now says so ("No config chosen. If no dialog appeared…"), and the reason is
printed to the terminal, e.g.:

```
ERROR rfd::backend::xdg_desktop_portal::portal::libdbus] Failed to connect to session bus: ... /run/user/1000/bus: No such file or directory
WARN  rfd::backend::xdg_desktop_portal] Using zenity fallback
ERROR rfd::backend::xdg_desktop_portal] Failed to pick file with zenity: No such file or directory
```

This is typical of WSL without a user session. Any one of these fixes it:
- **Install `zenity`** (`sudo apt install zenity`) — the simplest; `rfd` uses
  it whenever the portal is unreachable.
- **Turn on systemd in WSL** so the session bus and the portal exist: add
  `[boot]` / `systemd=true` to `/etc/wsl.conf`, then `wsl --shutdown` from
  Windows and reopen. You also need `xdg-desktop-portal-gtk` installed.
- **Skip the dialog**: launch with `--config PATH` as above.

Save and Export GIF use the same dialog and fail the same way; their status
messages say "No save path chosen" / "No GIF path chosen". A dialog that does
open but writes to an unwritable path reports the write error itself (e.g.
"Failed to save: ..."), rather than silently doing nothing. To see more
detail from `rfd`, run with `RUST_LOG=rfd=debug`.

### The workbench, in one picture

```
┌───────────────────────── toolbar ───────────────────────────┐
│ ◧ │ ▶ ⏭ +N ⏩ │ Speed ──●── Max │ − 8 + ⛶ │ ↺ │ GIF 💾 │ ? │ ◨ │
├─────────────┬───────────────────────────────┬───────────────┤
│ control     │                               │ workbench     │
│ Scenario    │                               │ Rule          │
│ Edit        │         viewport              │ Model         │
│ Style       │      (grid + layers)          │ Explore       │
│ Stats       │                               │               │
├─────────────┴───────────────────────────────┴───────────────┤
│ Step 120 · 60 steps/s · Time 2.0s · Avg 0.5 ms/step · status │
└──────────────────────────────────────────────────────────────┘
```

The **control** panel on the left is what you *do* to a simulation (load
it, edit cells, style the view, watch statistics). The **workbench** on the
right is what you *build* (the rule, the model's parameters, the Explore
tools). Each is a strip of tabs, one visible at a time, so neither becomes
a long scroll of collapsed headers. Both collapse from the toolbar or by
dragging their edge shut: the ◧ button sits at the toolbar's left end,
above the panel it hides, and ◨ at the right end above the workbench
(`L` / `W` do the same).

### Toolbar and keyboard shortcuts

Every shortcut lives in one table (`src/gui/shortcuts.rs`); the toolbar
tooltips, the `?` overlay and the key handler all read it, so they cannot
disagree. Press `?` in the app for the same list.

| Button | Does | Key |
|---|---|---|
| ▶ / ⏸ | Play / pause | `Space` |
| ⏭ | Step once | `S`, `→` |
| ⏩ +N | Run N steps as fast as the engine allows, then stop | — |
| Speed slider | Steps per second, 1–1000 on a log scale | — |
| Max | Ignore the slider; as many steps per frame as fit the time budget | — |
| − / value / + | Zoom out / set pixels per cell / zoom in | `−`, `+` or `=` |
| ⛶ | Zoom to fit the grid in the viewport | `F` |
| ↺ | Reset to the initial state | `Ctrl+R` |
| 🎞 | Export a GIF (opens the Edit tab's Export section) | `Ctrl+E` |
| 💾 | Save the scenario as a config file, including the run if past step 0 | `Ctrl+S` |
| ◧ / ◨ | Show or hide the control / workbench panel | `L` / `W` |
| ? | The shortcut list | `?` |
| | Toggle grid lines / age layer / probability layer | `G` / `A` / `P` |
| | Undo the last paint stroke *(paused only)* | `Ctrl+Z` |
| | Undo the last rule change *(paused only)* | `Ctrl+U` |
| | Random fill / Surprise me / Mutate rule with the Edit tab's settings *(paused only)* | `R` / `Shift+R` / `M` |
| | Smaller / bigger brush *(paused only)* | `[` / `]` |

Keys are ignored while a text box has focus, so typing `30` into the
Wolfram code box never steps the simulation. The editing keys are ignored
while playing; playback and view keys always work.

### Viewport

- **Pan**: drag with the middle button, or left-drag on empty space.
- **Zoom**: mouse wheel, the toolbar, or `F` to fit.
- **Edit**: left-click / drag with the tool chosen in the Edit tab (below).
- **Hover inspector** (Style tab): a tooltip with the cell's `x, y`, index,
  type (with its colour) and age; on a 1D history row it says which row.
- **Layers** are drawn over the cells: grid lines, the **age heat** (cells
  tinted by how recently they changed, so fronts glow), and the
  **probability map** an ensemble produces (blue = few members, red = most).
  Toggle them in the Edit tab or with `G` / `A` / `P`.

### Control tabs

**Scenario.** Built-in demos (Life, Rule 30, radius-2, three-state 2D,
straight-line), *Load Config JSON…*, the custom 1D builder (Wolfram code +
radius), grid size + *Resize*, and the 1D history row count.

**Save (💾 / `Ctrl+S`) and Load, together.** Save writes the whole scenario
as one JSON config: dimensions, rule, model, seed, and every colour you have
set (including Inactive). If the grid is still at step 0, that's the whole
file. If you have stepped past 0, the file also gets a `snapshot` block
holding the run itself — current cells, ages, per-cell history and peak
counts — so the exact same "Load Config JSON…" button opens it again. Loading
a file whose `snapshot` is past step 0 pops up a small prompt: **Resume at
step N** puts the grid back exactly where it was; **Start from initial**
begins at step 0 instead, the same as any other config. Either way Reset
always goes back to step 0, and both choices restore the file's colours.
Cancel leaves whatever was already loaded untouched. See `docs/lib.md`
"Saving and resuming a run" for the file format. One limitation: resuming a
wildfire scenario under `spread: "arrival"` recomputes its arrival-time table
from the resumed cells rather than restoring the original one, since that
table was never saved in the first place — see the same doc section.

**Edit.**
- *Tool*: **Cycle** (click a cell to step it to the next type), **Paint**
  (drag to paint the chosen type), **Stamp** (2D: place a Glider,
  lightweight spaceship, R-pentomino or Acorn at the click). In Paint and
  Stamp mode an outline follows the mouse showing exactly which cells the
  next click will touch. One undo entry per stroke or stamp.
- *Brush*: a **range** 0–7 (0 = one cell) and, on 2D grids, a **shape**:
  Moore (square), Von Neumann (diamond), Langton (diagonals), straight
  lines (cross) or Knight (chess moves). The footprint is the centre plus
  that neighbourhood at that range, built from the same offset tables the
  rules use, so a Von Neumann brush of range 2 paints exactly the cells a
  Von Neumann range-2 subrule would count. `[` and `]` resize it.
- *Random fill*: density, type, a visible **seed** (↻ moves it on), and
  *Clear first*. The same seed always paints the same picture. With *Clear
  first* the result becomes the new starting state Reset returns to.
- *Fun*: **Surprise me** rolls every knob of the rule and model at random
  within its declared bounds and random-fills 30 % with the first type;
  **Mutate rule** nudges every knob a little (the slider sets how much);
  **Undo rule** puts the knobs back (up to 16 changes deep). Each press
  moves the seed on so the next surprise differs.
- *Layers*: grid lines, age heat + fade cap, probability map + opacity.
- *Export GIF*: frame delay, scale, 1D space-time stacking, *Export…*.

**Style.** Dark / light theme; font size; grid-line colour; hover
inspector; a palette preset (Calm, Okabe-Ito, Tol bright, Viridis, Ember)
with *Re-slot colours*; the Inactive colour; and one picker per declared
type. Colours from a config's `colors` block survive a palette change.

**Stats.** Live population and peak counts, and the history chart. During
a *Run to +N* the chart gains one point per drawn frame rather than per
step; the counters above it are exact either way.

### Workbench tabs

**Rule.** The subrule editor. Values are held as text so half-typed numbers
are legal until *Apply to grid*; *Add type* declares a new cell type.

**Model.** Shown only when the loaded grid has a `model` (for example
`configs/2d_wildfire_demo.json`); Life shows a note instead. Controls are
grouped under headings the model supplies. Cheap parameters commit as you
drag; expensive ones (those needing an internal rebuild) commit when the
gesture ends. Sliders stop at each parameter's end stops and typed values
are pulled back into range, so an out-of-range value never reaches the
model. A read-only parameter (the seed) shows as a label. **Reset keeps the
values you set**: an accepted edit is mirrored into the snapshot Reset
restores from.

**Explore.** Run the loaded grid as an **ensemble** (a probability map that
can learn from the cells you paint) or **evolve** its knobs (best score,
novelty, or a MAP-Elites archive with a clickable gallery). The tab knows
nothing about which model is loaded: genes come from the grid's own
parameter list, tracked types from its declared types. The full walkthrough
is [explore.md](explore.md) §14; the engines run on a background thread,
so the grid stays paintable and playable while they work.

### Code Layout (Module Map)

The GUI source lives in `src/gui/`. Each file owns one concern. The design
borrows three habits from React-style UIs: panels **push actions** into a
queue and one reducer applies them after everything is drawn (so a button
never mutates state mid-frame); `state.rs` is the **single source of
truth**, with derived data (declared types, gene rows) rebuilt per frame;
and **design tokens** (`theme.rs`) hold spacing, radius, accent and palettes
so the panels look alike without copying numbers around.

| File | What lives here |
|---|---|
| `gui.rs` | Module list and the single `pub use app::run_gui` the binary calls. |
| `gui/app.rs` | `CellaApp` and the `eframe::App` impl; `ui` only says which panel is drawn in what order, then drains actions and polls the workers. |
| `gui/state.rs` | The structs `CellaApp` is made of (`Scenario`, `Playback`, `ViewSettings`, `EditState`, `Chrome`, …) with the starting values in their `Default`s. |
| `gui/actions.rs` | The `Action` enum (every user intent) and the reducer `apply_action`; also random fill and the paint/stamp/undo helpers. |
| `gui/shortcuts.rs` | The one keyboard table, read by the toolbar, the `?` overlay and the key handler. |
| `gui/theme.rs` | Design tokens, `ThemeChoice`, `section()`, the palette presets. |
| `gui/painter.rs` | Cells → rectangles with run merging; calls the layer pass between cells and grid lines. |
| `gui/layers.rs` | `LayerState`, the probability map, the quantised overlay row emitter. |
| `gui/render.rs` | `color_for`, the heat and age ramps; shared by viewport and GIF export so they cannot disagree. |
| `gui/interact.rs` | Viewport gestures: zoom, pan, brush painting, cycle, stamp ghost, hover inspector, hotkeys. |
| `gui/patterns.rs` | The stamp patterns as offset tables. |
| `gui/sim.rs` | Stepping, the playback clock and rate meter, statistics recording, the `test_app()` fixture. |
| `gui/scenarios.rs` | Demos, config loading, resize, reset. |
| `gui/types.rs` | Which types a scenario declares and what a click cycles to. |
| `gui/export.rs` | GIF and JSON writing plus the file dialogs. |
| `gui/explore.rs` | Explore state, the worker thread and its messages, gene rows from `Grid::params()`, Surprise me / Mutate rule / Undo rule, applying genomes. |
| `gui/gallery.rs` | Archive heat-map layout, top elites, thumbnail pixels (pure, unit-tested). |
| `gui/panels/` | One file per region: `toolbar`, `tabs` (the two strips), `scenario`, `edit`, `style`, `statistics`, `rule_editor`, `model`, `explore`. |
| `gui/panels/widgets.rs` | Controls used by more than one panel (type picker, randomness control). |
| `gui/panels/rule_edit_model.rs` | The rule editor's text-form working copy. |

Tests run headless with `egui::__run_test_ui`; the app cannot open a window
on this repo's WSL box, so the manual GL checklist lives in
[roadmap.md](roadmap.md) Phase 5.

---

## GIF Export

1. Pause.
2. Edit tab → **Export GIF**: set the frame delay (ms), scale (pixels per
   cell), and for 1D whether to stack steps into a space-time image.
3. Press **Export…** (or `Ctrl+E`), pick a file. The frames are rendered on
   a worker thread; the status bar reports progress and completion.

---

## Configuration Guide

The application uses a common JSON format for storing both rules and grid states.

### `dim` Discriminator
Every configuration must have a `"dim"` field, which is either `"1d"` or `"2d"`.

### `colors` (optional)
A map from cell-type name to an `#rrggbb` colour. Types you leave out get an
automatic colour, and `"Inactive"` sets the background:

```json
"colors": { "Forest": "#2e8b57", "Shrub": "#9acd32", "Burning": "#ff4500", "BurnedOut": "#6b6b6b" }
```

Without this, the GUI hands each declared type the next free slot of its
8-colour palette, in alphabetical order, so types never share a colour until a
scenario has more than eight. (Earlier versions hashed the name into a slot,
which let Forest and Burning both land on sky blue in the wildfire demo.) A
value that is not `#rrggbb` is reported in the status bar and ignored. Colours
are display-only: the engine, snapshots, and validation runs never see them.

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
