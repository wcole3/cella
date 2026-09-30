# Cella Application Guide (`cella`)

`cella` is the program you run to watch cellular automata (CA) evolve. A
cellular automaton is a grid of cells, each holding one "type" (Alive, Dead,
Forest, Burning, ...), that updates in steps: every cell looks at its
neighbors and a rule decides what type it becomes next. Cella can run
1D automata (a row of cells, like Wolfram's Rule 30) and 2D automata (a
grid, like Conway's Game of Life).

The binary has two modes:

- a **GUI** (window) for watching, editing, and experimenting, and
- a small **CLI** (terminal menu) for quick demos.

This guide is for people *using* the program. To embed the engine in your own
Rust code, read [lib.md](lib.md) instead.

**Contents**

1. [First five minutes](#first-five-minutes)
2. [Starting the program](#starting-the-program)
3. [The CLI demo menu](#the-cli-demo-menu)
4. [The GUI workbench](#the-gui-workbench)
   - [Toolbar and keyboard shortcuts](#toolbar-and-keyboard-shortcuts)
   - [Viewport](#viewport)
   - [Control tabs](#control-tabs-left-panel)
   - [Workbench tabs](#workbench-tabs-right-panel)
5. [Saving, loading, and resuming](#saving-loading-and-resuming)
6. [GIF export](#gif-export)
7. [Config file guide](#config-file-guide)
8. [Troubleshooting](#troubleshooting)
9. [For contributors: GUI code layout](#for-contributors-gui-code-layout)

---

## First five minutes

You need a Rust toolchain (see the top-level [README](../README.md) for the
supported version and platform notes). From the repository root:

1. **Start the GUI.** The first build downloads and compiles dependencies,
   so it takes a while; later runs start fast.

   ```bash
   cargo run --release -- --gui
   ```

   A window opens showing Conway's Game of Life on a small grid.

2. **Play it.** Press `Space` (or the play button in the toolbar) to run,
   `Space` again to pause. `S` or `Right arrow` advances one step. Drag the
   **Speed** slider to go faster or slower.

3. **Draw on it.** Pause, then open the **Edit** tab in the left panel, choose
   the **Paint** tool, and drag across the grid. Press `Ctrl+Z` to undo a
   stroke. Press `R` (while paused) for a random fill if the grid is empty.

4. **Change the rule.** Open the **Rule** tab in the right panel. Each
   "subrule" is one line of the rule (see [Config file guide](#config-file-guide)
   for what the fields mean). Edit a number, press **Apply to grid**, and
   press `Space`. The live grid now follows your edited rule. `Ctrl+R` resets
   to the starting state.

5. **Load something else.** Open the **Scenario** tab and click a demo
   button, or **Load config JSON...** to open any file from
   [`configs/`](../configs/). Try `configs/2d_wildfire_demo.json`: it adds a
   **Model** tab with sliders for wind, spread probability and more. Save your
   work with the floppy-disk button (`Ctrl+S`).

If the **Load config JSON...** button seems to do nothing, see
[Troubleshooting](#troubleshooting) (it is a desktop-environment issue, and
there is a one-line workaround).

---

## Starting the program

```bash
cargo run --release              # CLI menu
cargo run --release -- --gui     # GUI
```

Everything after the lone `--` goes to Cella rather than to `cargo`. The
`Makefile` wraps the same thing: `make run` (CLI) and `make run-gui` (GUI at
1024x768). If you built the binary already, run `target/release/cella` with the
same options.

| Option | Meaning |
|---|---|
| `--gui` | Open the GUI instead of the CLI menu. (The bare word `gui` also works.) |
| `--size=WxH` | Initial window size in pixels, e.g. `--size=1280x720`. Also accepted: `--size 1280x720` and `--size=1280,720`. |
| `--width=W`, `--height=H` | Set one or both dimensions. If only one is given, the other follows a 16:9 ratio. The space forms (`--width 1920`) work too. |
| `--config=PATH` | Open this config file at startup instead of the Life demo. `--config PATH` also works. |

The default window is 1280x720 and never opens smaller than 800x450. The
size and `--config` options only apply together with `--gui`.

Set `RUST_LOG=debug` to see more log output in the terminal; the default shows
warnings and errors.

```bash
cargo run --release -- --gui --size=1600x900 --config configs/2d_wildfire_demo.json
```

---

## The CLI demo menu

Without `--gui` you get a numbered menu in the terminal:

1. **2D Game of Life (approx)**: a standard Conway-style rule.
2. **1D Wolfram Rule 30 (n=1)**: the classic elementary CA.
3. **1D Wolfram n=2 demo**: a larger neighborhood radius.
4. **1D Custom (Wolfram code + n)**: type your own Wolfram code and radius.
5. **Load from configuration file (JSON)**: run a config file.
6. **Langton's ant (placeholder)**: only prints "not yet implemented"
   (it needs a moving agent, which the engine does not have).
7. **1D three-state cycle demo**: several cell types cycling in 1D.
8. **2D three-state cycle demo**: the same in 2D.
9. **2D StraightLine neighborhood demo**: neighbors along the four cardinal
   lines only.
0. **Exit**.

Each demo asks how many steps to run (press Enter for the default) and then
prints the result as text: `.` is an Inactive cell, and every other cell type
gets a symbol.

### Running a config file from the CLI

1. Choose `5`.
2. Type a path, such as `configs/life.json`.
3. Type the number of steps (default 10).

For a 1D config the CLI prints the final row. For a 2D config it prints the
grid at the start and after every step, then writes the end state to
`snapshot.json` in the current directory (overwriting any file of that name).
If the config was saved mid-run, the CLI first asks `Resume at step N? [Y/n]`.

---

## The GUI workbench

The GUI is built with the `egui` framework. Everything happens in one window:

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
│ Step: 120 · 60 steps/s · Time: 2.00s · Avg: 0.50 ms/step ...│
└──────────────────────────────────────────────────────────────┘
```

The **control** panel on the left is what you *do* to a simulation: load it,
edit cells, style the view, watch statistics. The **workbench** on the right
is what you *build*: the rule, the model's parameters, the Explore tools.
Each is a strip of tabs, one visible at a time. Both panels collapse from the
toolbar buttons at the two ends (◧ for the control panel, ◨ for the
workbench), by pressing `L` / `W`, or by dragging the panel edge shut.

The status bar at the bottom shows the step count, speed, timing, and the
latest message (for example "Loaded demo: Life (2D)" or an error).

### Toolbar and keyboard shortcuts

Every shortcut lives in one table (`src/gui/shortcuts.rs`); the toolbar
tooltips, the `?` overlay, and the key handler all read it, so they cannot
disagree. Press `?` in the app for the same list. (On macOS, `Ctrl` means
`Cmd`.)

| Button | Does | Key |
|---|---|---|
| ▶ / ⏸ | Play / pause | `Space` |
| ⏭ | Step once | `S`, `→` |
| `+N` box and ⏩ | Run N steps as fast as the engine allows, then stop | none |
| Speed slider | Steps per second, 1-1000 on a log scale | none |
| Max | Ignore the slider; run as many steps per frame as fit the time budget | none |
| − / value / + | Zoom out / set pixels per cell / zoom in | `-`, `+` or `=` |
| ⛶ | Zoom to fit the grid in the viewport | `F` |
| ↺ | Reset to the initial state (step 0) | `Ctrl+R` |
| GIF | Export an animated GIF | `Ctrl+E` |
| 💾 | Save the scenario as a config file (includes the run if past step 0) | `Ctrl+S` |
| ◧ / ◨ | Show or hide the control / workbench panel | `L` / `W` |
| ? | The shortcut list | `?` |
| | Toggle grid lines / age layer / probability layer | `G` / `A` / `P` |
| | Undo the last paint stroke *(paused only)* | `Ctrl+Z` |
| | Undo the last rule change *(paused only)* | `Ctrl+U` |
| | Random fill / Surprise me / Mutate rule, using the Edit tab's settings *(paused only)* | `R` / `Shift+R` / `M` |
| | Smaller / bigger brush *(paused only)* | `[` / `]` |

Keys are ignored while a text box has focus, so typing `30` into the Wolfram
code box never steps the simulation. The editing keys marked "paused only"
are ignored while playing; playback and view keys always work.

### Viewport

- **Pan**: drag with the middle button, or left-drag on empty space.
- **Zoom**: mouse wheel, the toolbar, or `F` to fit.
- **Edit**: left-click or drag with the tool chosen in the Edit tab.
- **Hover inspector** (switch it on in the Style tab): a tooltip with the
  cell's `x, y`, index, type (with its color), and age; on a 1D history row it
  says which row.
- **Layers** are drawn over the cells: grid lines; the **age heat** (cells
  tinted by how recently they changed, so fronts glow); and the
  **probability map** an ensemble produces (blue = few members, red = most).
  Toggle them in the Edit tab or with `G` / `A` / `P`.

For 1D automata the viewport shows the current row at the bottom with its
recent past above it, so you see the pattern grow over time. How many rows
are kept is the **1D history rows** setting in the Scenario tab.

### Control tabs (left panel)

**Scenario.** Loading and sizing.
- *Load*: **Load config JSON...** and five built-in demos (Life, 1D Rule 30,
  1D radius 2, 2D three-state, 2D straight line).
- *Custom 1D rule*: a Wolfram **code** and radius **n** (1-3), then **Build**.
- *Grid*: **W** / **H** and **Resize**, plus **1D history rows** for 1D grids.

Resize changes the grid size without restarting the run. The top-left part of
the grid is kept, including each cell's age and history. New cells are
Inactive. The step count, rule, seed, model, and colors carry on, and Reset
goes back to the starting grid at the new size. Paint undo is cleared, because
it remembers cells by position. The button is greyed out when the grid is
already the size you typed. Changing the width also changes the random numbers
for most cells from then on; see [lib.md](lib.md#resizing-a-grid).

**Edit.** What the mouse does, and some shortcuts for filling the grid.
- *Tool*: **Cycle** (click a cell to step it to the next type), **Paint**
  (drag to paint the chosen **Paint type**), **Stamp** (2D only: place a
  Glider, Lightweight spaceship, R-pentomino, or Acorn at the click). In
  Paint and Stamp mode an outline follows the mouse showing exactly which
  cells the next click will touch. One undo entry per stroke or stamp.
- *Brush* (Paint tool): a **Brush range** 0-7 (0 = one cell) and, on 2D
  grids, a **Shape**: Moore (square), Von Neumann (diamond), Langton
  (diagonals), Straight lines (cross), or Knight (chess moves). The footprint
  is the center plus that neighborhood at that range, built from the same
  offset tables the rules use, so a Von Neumann brush of range 2 paints
  exactly the cells a Von Neumann range-2 subrule would count. `[` and `]`
  resize it.
- *Random fill*: **Share of cells**, **Type**, a visible **Seed** (the arrow
  button moves it on), and **Clear first**, then **Fill**. The same seed
  always paints the same picture. With *Clear first* the result becomes the
  new starting state that Reset returns to.
- *Fun*: **Surprise me** rolls every knob of the rule and model at random
  within its declared bounds and random-fills 30 % with the first type;
  **Mutate rule** nudges every knob a little (the **Mutation size** slider
  sets how much); **Undo rule** puts the knobs back (up to 16 changes deep).
  Each press moves the seed on so the next surprise differs.
- *Layers*: **Grid lines**, **Age heat** (with a fade-after step count),
  **Probability map**, and an **Opacity** slider.
- *Export GIF*: see [GIF export](#gif-export).

**Style.** Dark / Light theme; text size; grid-line color; the hover
inspector; a palette preset (Calm, Okabe-Ito, Tol bright, Viridis, Ember)
with **Re-slot**, which gives every type its automatic color again; the
Inactive (background) color; and one color picker per declared type. Colors
from a config's `colors` block survive a palette change.

**Stats.** Live population and peak counts per cell type, and a history
chart. Tick the type names above the chart to choose which lines it draws.
During a *Run to +N* the chart gains one point per drawn frame rather than per
step; the counters above it are exact either way.

### Workbench tabs (right panel)

**Rule.** The subrule editor. *Types/states available to rules* lists the
cell types; **Add type** declares a new one (`Inactive` is always present).
Each subrule has fields for its type, criteria, counts, and so on, with
**Up** / **Down** to reorder (order matters: the first match wins) and
**Remove**. **Add subrule** appends one. Values are held as text so
half-typed numbers are legal until you press **Apply to grid**; a rule the
library refuses stays in the editor with the reason shown.

**Model.** Shown only when the loaded grid has a `model` (for example
`configs/2d_wildfire_demo.json`); otherwise it says "No model attached".
Controls are grouped under headings the model supplies. Cheap parameters
commit as you drag; expensive ones (those needing an internal rebuild) commit
when the gesture ends. Sliders stop at each parameter's end stops and typed
values are pulled back into range, so an out-of-range value never reaches the
model. A read-only parameter (the seed) shows as a label. **Reset keeps the
values you set**: an accepted edit is mirrored into the snapshot Reset
restores from.

**Explore.** Run the loaded grid as an **ensemble** (*Monte Carlo*: many runs
with different random seeds, combined into a probability map that can learn
from the cells you paint) or **evolve** its knobs (*Best score*, *Novelty*,
or a *MAP-Elites* archive with a clickable gallery). The tab knows nothing
about which model is loaded: genes come from the grid's own parameter list,
tracked types from its declared types. The full walkthrough is
[explore.md](explore.md) section 14; the engines run on a background thread,
so the grid stays paintable and playable while they work.

---

## Saving, loading, and resuming

**Save** (💾 or `Ctrl+S`) writes the whole scenario as one JSON config:
dimensions, rule, model, seed, and every color you have set (including
Inactive). The default file name is `snapshot.json` in the current directory.

- If the grid is still at step 0, that is the whole file.
- If you have stepped past 0, the file also gets a `snapshot` block holding
  the run itself (current cells, ages, per-cell history, and peak counts), so
  the same **Load config JSON...** button opens it again.

Loading a file whose `snapshot` is past step 0 pops up a small prompt:
**Resume at step N** puts the grid back exactly where it was; **Start from
initial (step 0)** begins at step 0 instead, like any other config. Either
way Reset always goes back to step 0, and both choices restore the file's
colors. **Cancel** leaves whatever was already loaded untouched. See
[lib.md](lib.md#saving-and-resuming-a-run) for the file format.

One limitation: resuming a wildfire scenario under `spread: "arrival"`
recomputes its arrival-time table from the resumed cells rather than
restoring the original one, since that table is never saved (see the same
lib.md section).

Save also keeps your Explore settings. If the file you loaded had `ensemble`
or `evolve` settings, or you changed or ran Monte Carlo or Evolve this
session, those settings go into the file too. A short popup says what was
saved. Loading such a file fills the Explore tab with its settings and pops
up a note listing anything the tab cannot edit (for example a driver, or
genes that are not grid knobs). Those are kept exactly as loaded, and saved
back unchanged unless you edit the matching control. When a file has both
blocks, the tab opens on Monte Carlo.

---

## GIF export

1. Click **GIF** in the toolbar (or `Ctrl+E`, or **Export GIF...** in the Edit
   tab).
2. Pick a file name in the save dialog (the default is `cella.gif`).
3. An **Export GIF** box asks for the number of **steps** (frames, up to
   10,000), the frame rate in **fps** (up to 50), **Loop forever** (off means
   the GIF plays once and stops on its last frame), and, for 1D grids,
   **Stack rows into a space-time image**. It starts with your `+N` value and
   your playback speed.
4. Press **Export**. The frames are rendered on a worker thread starting from
   the current state; a progress bar shows in the toolbar and status bar. The
   picture size follows the current zoom (pixels per cell).

---

## Config file guide

The application uses one JSON format for both rules and grid states. Ready-made
examples are in [`configs/`](../configs/).

### `dim` discriminator
Every configuration must have a `"dim"` field, which is either `"1d"` or
`"2d"`. The other top-level fields are `width` (and `height` for 2D),
`history_limit` (how many past types each cell remembers), `initial` (the
starting cell types, one name per cell, row by row), and `rule`. Optional
fields are `seed`, `colors`, `model`, `ensemble`, `evolve`, and `snapshot`.

### `colors` (optional)
A map from cell-type name to an `#rrggbb` color. Types you leave out get an
automatic color, and `"Inactive"` sets the background:

```json
"colors": { "Forest": "#2e8b57", "Shrub": "#9acd32", "Burning": "#ff4500", "BurnedOut": "#6b6b6b" }
```

Without this, the GUI hands each declared type the next free slot of its
8-color palette, in alphabetical order, so types never share a color until a
scenario has more than eight. A value that is not `#rrggbb` is reported in
the status bar and ignored. Colors are display-only: the engine, snapshots,
and validation runs never see them.

### How rules are matched
A rule is an ordered list of **subrules**. For each cell, the engine tries the
subrules in order and the **first one that matches** decides the cell's next
type. If none matches, the cell becomes `Inactive`. That means a cell that
should stay as it is needs a subrule that says so, and Rule 30 below needs a
second subrule to let new cells be born next to existing ones.

### Example: Rule 30 (1D)
Here `"initial"` is shortened; a real file lists 257 names, all `"Inactive"`
except one `"X"` in the middle (see `configs/1d_rule30_center.json`).

```json
{
  "dim": "1d",
  "width": 257,
  "history_limit": 4,
  "initial": ["Inactive", "...", "X", "...", "Inactive"],
  "rule": {
    "subrules": [
      {
        "current_type": "X",
        "criteria_type": "X",
        "wolfram_code": "30",
        "n": 1,
        "randomness": null,
        "output_type": "X"
      },
      {
        "current_type": "Inactive",
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

The first subrule applies to cells that are already `X`; the second applies
to `Inactive` cells. Each looks at the `criteria_type` cells in a window of
`2n + 1` cells around it and uses the Wolfram code (30 here) as a lookup
table for whether the result is `output_type`.

### Example: a custom 2D rule
This is one subrule: an `Alive` cell stays `Alive` if it has exactly 3 `Alive`
neighbors. Use `configs/life.json` for a complete Life rule.

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

(`initial` is omitted here for brevity; a real 2D file needs
`width * height` entries unless you fill the grid in the GUI.)

Notes on 2D subrule fields:

- `op` is one of `"lt"`, `"gt"`, `"eq"`. The `gt`/`lt` comparisons are
  **inclusive** (`gt` = "at least `count`", `lt` = "at most `count`").
- `neighborhood` is one of `"Moore"` (the surrounding square),
  `"VonNeumann"` (a diamond), `"Langton"` (diagonals), `"StraightLine"`
  (the four cardinal lines), `"Knight"` (chess-knight moves).
- `range` is how far out the neighborhood reaches (1 = adjacent cells).
- An optional `"limit"` field turns `gt`/`lt` into an inclusive
  between-range. For example, "survive with 2 to 3 neighbors":

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

- `"randomness"` (optional, `0.0`-`1.0`) is the probability that a matching
  subrule is skipped. Both `"limit"` and `"randomness"` may be omitted.

For the full field list and validation rules, see
[lib.md](lib.md#the-rule-system).

---

## Troubleshooting

### "Load config JSON..." does nothing

The app does not draw its own file dialog. It asks the desktop for one,
through the `rfd` crate. On Linux that means: talk to `xdg-desktop-portal`
over the D-Bus *session* bus, and if that fails, run the `zenity` program.
When neither is available, `rfd` gives up in a few milliseconds and reports
"no file chosen", which is exactly what pressing Cancel reports, so the
button looks dead. The status bar says so ("No config chosen. If no dialog
appeared..."), and the reason is printed to the terminal, e.g.:

```
ERROR rfd::backend::xdg_desktop_portal::portal::libdbus] Failed to connect to session bus: ... /run/user/1000/bus: No such file or directory
WARN  rfd::backend::xdg_desktop_portal] Using zenity fallback
ERROR rfd::backend::xdg_desktop_portal] Failed to pick file with zenity: No such file or directory
```

This is typical of WSL (Windows Subsystem for Linux) without a user session.
Any one of these fixes it:
- **Skip the dialog**: launch with `--config PATH`, for example
  `cargo run --release -- --gui --config configs/2d_wildfire_demo.json`. It is
  the same as clicking **Load config JSON...** and picking that file.
- **Install `zenity`** (`sudo apt install zenity`); `rfd` uses it whenever the
  portal is unreachable.
- **Turn on systemd in WSL** so the session bus and the portal exist: add
  `[boot]` / `systemd=true` to `/etc/wsl.conf`, then run `wsl --shutdown` from
  Windows and reopen. You also need `xdg-desktop-portal-gtk` installed.

Save and Export GIF use the same dialog and fail the same way; their status
messages say "No save path chosen" / "No GIF path chosen", and `--config`
cannot help there, so install `zenity` if you want to save. A dialog that does
open but writes to an unwritable path reports the write error itself (for
example "Failed to save: ..."). To see more detail from `rfd`, run with
`RUST_LOG=rfd=debug`.

### The window does not open

Check the terminal for a `GUI error:` line. The GUI needs a working display
(X11 or Wayland on Linux); under WSL that means WSLg or an X server. The CLI
menu (`cargo run --release`) needs no graphics at all.

### Changing the number of worker threads

The engine reads `threads=N` from a `cella.properties` file in the current
directory or a parent (set `threads=1` to disable parallel stepping). See
[lib.md](lib.md#thread-configuration).

---

## For contributors: GUI code layout

The GUI source lives in `src/gui/`. Each file owns one concern. The design
borrows three habits from React-style UIs: panels **push actions** into a
queue and one reducer applies them after everything is drawn (so a button
never mutates state mid-frame); `state.rs` is the **single source of truth**,
with derived data (declared types, gene rows) rebuilt per frame; and **design
tokens** (`theme.rs`) hold spacing, radius, accent, and palettes so the panels
look alike without copying numbers around.

| File | What lives here |
|---|---|
| `gui.rs` | Module list and the single `pub use app::run_gui` the binary calls. |
| `gui/app.rs` | `CellaApp` and the `eframe::App` impl; `ui` only says which panel is drawn in what order, then drains actions and polls the workers. |
| `gui/state.rs` | The structs `CellaApp` is made of (`Scenario`, `Playback`, `ViewSettings`, `EditState`, `Chrome`, ...) with the starting values in their `Default`s. |
| `gui/actions.rs` | The `Action` enum (every user intent) and the reducer `apply_action`; also random fill and the paint/stamp/undo helpers. |
| `gui/shortcuts.rs` | The one keyboard table, read by the toolbar, the `?` overlay and the key handler. |
| `gui/theme.rs` | Design tokens, `ThemeChoice`, `section()`, the palette presets. |
| `gui/painter.rs` | Cells to rectangles with run merging; calls the layer pass between cells and grid lines. |
| `gui/layers.rs` | `LayerState`, the probability map, the quantized overlay row emitter. |
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

Tests run headless with `egui::__run_test_ui`, so they do not open a window.
The hand-run checklist for real rendering is in [roadmap.md](roadmap.md),
Phase 5.
