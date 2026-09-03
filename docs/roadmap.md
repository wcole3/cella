# Cella Roadmap — GUI Performance, Pluggable Model UIs, and Usability

This document is the plan for the next three phases of work on the `cella`
binary crate (the GUI), plus one addition to `cella_lib`. It follows the same
conventions as [`performance.md`](performance.md): sections are marked *(done)*
or *(open)*, and **line numbers are deliberately omitted in favour of naming
functions**, because line numbers drift as soon as anyone edits a file.

It is written to be picked up cold. If you have never touched this codebase,
read [`app.md`](app.md) first — in particular its **Code Layout (Module Map)**
section, which lists what lives in which `src/gui/` file.

| Phase | Scope | Status |
|---|---|---|
| 1 | Reorganize the GUI so the rest is safe to change | *(done)* — commit `b092beb` |
| 2 | GUI performance: measure, then fix three suspected defects | *(done)* — commits `e5c304d..8c1c1a7` |
| 3 | Let external models describe their own parameters | *(done)* — commits `6d7e9b3..ad3654d` |
| 4 | Usability and fun | *(open)* |

## Background: why this order

The work started as a review of `cella_lib/src/rules.rs`. The finding was that
the **simulation engine is already near its practical ceiling** — it has
per-step rule "plans", flattened neighbour offsets, precomputed comparison
ranges, and bit-packed fast paths for both 1D Wolfram rules and 2D
threshold rules like Conway's Game of Life. The remaining engine ideas
(§3.7 SIMD, §3.13 interior bounds-check elision in `performance.md`) are the
hard, low-yield leftovers and are **out of scope here**.

Everything cheap and worthwhile is in the GUI, which never received the same
attention. Phase 1 came first because `src/gui/app.rs` was a single 2343-line
file, and changing anything inside it safely was the blocker for Phases 2–4.

---

## 1. What Phase 1 landed *(done)*

Commit `b092beb`, "restructure gui app to be more structured".

- `src/gui/app.rs` went from **2343 lines to 414**, split across 18 modules.
  See the module map in [`app.md`](app.md).
- The `eframe::App::ui` method went from **432 lines nested 16 levels deep to
  18 lines** that do nothing but name the panels in draw order.
- `CellaApp` went from **50 flat fields to 9 named groups** (`Scenario`,
  `Playback`, `ViewSettings`, `EditState`, `ExportState`, `StatsState`,
  `EditorState`, `Chrome`, `Inputs`), defined in `src/gui/state.rs`.
- The click-to-cell arithmetic, previously written out four separate times, is
  now one pure function `cell_index_at` in `src/gui/interact.rs`, with unit
  tests.
- Tests went from 10 to 18 (`cargo test --package cella --bin cella`).

### Field paths changed — this matters when reading older notes

Because of the regrouping, **every field access moved**. If you find a note or
a diff referring to the old flat names, translate it:

| Old | New |
|---|---|
| `self.playing` | `self.playback.playing` |
| `self.scale` | `self.view.scale` |
| `self.undo_stack` | `self.edit.undo_stack` |
| `self.export_join` | `self.export.join` |
| `self.stats_history` | `self.stats.history` |
| `self.d1` / `self.d2` / `self.dim` | `self.scenario.d1` / `.d2` / `.dim` |

All code snippets in this document use the **new** paths.

### Two corrections worth recording

Both of these were stated wrongly in the original plan, and both cost time:

1. **The struct had 50 fields, not 42.** The original count came from a grep
   using the pattern `[a-z_]+`, which silently skipped every field with a digit
   in its name: `d1`, `d2`, `history_1d`, `history_limit_1d`,
   `min_view_rows_1d`, `export_1d_with_history`, `rule_edit_1d`,
   `rule_edit_2d`. The same bug then bit the first automated rewrite pass. The
   compiler caught it, but the lesson stands: when matching Rust identifiers
   with a regex, use `[a-z_][a-z0-9_]*`, not `[a-z_]+`.
2. **Deduplicating the six copy-pasted type pickers saved 15 lines, not the
   ~120 predicted.** `rustfmt` wraps the helper's call arguments across several
   lines, and the long tooltip strings dominate either way. The deduplication
   was still the right change — six copies became one implementation in
   `src/gui/panels/widgets.rs` — but not for the reason originally claimed.

### Outstanding: Phase 1 is compile-checked only

**The manual click-through never ran.** The GUI cannot start in the development
environment used for Phase 1:

```
libEGL warning: failed to get driver name for fd -1
MESA: error: ZINK: failed to choose pdev
GUI error: winit EventLoopError: Exit Failure: 1
```

This is a WSLg/Mesa driver problem, not a code defect: the pristine
pre-refactor build (checked out into a scratch `git worktree`) fails
identically. The failure happens inside `eframe::run_native`, before any
application code runs.

The consequence is real, though. Rust cannot type-check *behaviour* inside a
closure it never runs, and the automated tests do not enter egui closures. So
every panel body and every gesture handler in Phase 1 is verified only by the
compiler and by a line-accounting audit, not by a human clicking things.

**Do this first, on a machine with working GL:**

```bash
cargo run --release -- --gui --size=1600x900
```

Check that every panel opens; play/pause/step/reset work; both Paint and Cycle
modes edit the cell you clicked; `Ctrl+Z` undoes; the rule editor applies a
rule; and GIF export runs to completion. The CLI path (`cargo run --release`)
does work headless and was smoke-tested.

---

## 2. Phase 2 — GUI performance *(done)*

### What Phase 2 landed

Commits `e5c304d..e6ea3ce`, plus the review fixes that followed them.

- **§2.1 — the painter compares types, not colours.** `RowPainter::emit_row`
  in `src/gui/painter.rs` decides where a paint run ends by comparing
  `CellType` (one `u32`) and only resolves a `Color32` when the type actually
  changes — still extending the run when two different types happen to share
  a colour, so the rectangle counts the older tests assert on are unchanged.
- **§2.2 — "Run to +N" is budgeted by time, not by a fixed step count.**
  `tick_play` in `src/gui/sim.rs` keeps stepping until `RUN_TO_FRAME_BUDGET`
  (8 ms) is spent, in `RUN_TO_CHUNK`-sized (32-step) chunks through the
  clock-free `run_to_batch`, so the clock is read once per chunk rather than
  once per step. `start_run_to` remembers the play state the run interrupted;
  `cancel_run_to` gives it back, and is called by `toggle_play` (the
  Play/Pause button), by `reset_to_initial`, and by `stats_clear_and_init`, so
  Pause, Reset, and loading any scenario all stop a run in progress. The
  `Pacing` enum in `src/gui/state.rs` is in place for a future "Max speed"
  playback setting; nothing in the UI selects it yet.
- **§2.3 — a burst samples the chart once per frame.** `advance_grid` is the
  inner step — 1D history push, `Grid1D::step` / `Grid2D::step`,
  `timed_steps` — with no clock read and no statistics sample, and it is what
  `run_to_batch` loops. `step_once` (the Step button and the paced tick) is
  `refresh_play_timer` + `advance_grid` + `stats_record_step`, one sample per
  step. `tick_play`'s burst branch calls `stats_record_step` once, after its
  whole chunk loop.
- **Not taken:** the optional `pub fn cells(&self) -> &[CellType]` accessor on
  `Grid2D` floated at the end of §2.1. It crosses into `cella_lib`, which this
  phase otherwise stays out of, and the §2.4 numbers did not ask for it.
- **Numbers:** logged as E10 in [`performance.md`](performance.md). The
  painter number is measured (`paint_bench`: ~3.1 → ~1.18 ms per synthetic
  frame). The "Run to +100000" wall-clock number is **still to be taken by
  hand on a machine with working GL** — the GUI cannot be launched in this
  environment.

The §2.1–§2.4 text below is the plan as it stood *before* implementation, kept
for its reasoning; where it and the code disagree, the code is what shipped.

Three suspected defects. **None has been measured yet** — they are reasoned
from the code, so do §2.4 first and let the numbers decide what is worth
landing. 2.1 and 2.2 are independent; 2.3 builds on the `run_to_batch` helper
that 2.2 introduces, so land 2.2 before 2.3.

### 2.1 `emit_row` looks up a colour for every cell

**Where:** `RowPainter::emit_row` in `src/gui/painter.rs`.

**The problem.** To avoid drawing one rectangle per cell, the painter walks a
row and merges neighbouring cells of the same colour into one wide rectangle.
That part is good — cellular automata are strongly spatially correlated, so a
row of 500 cells usually collapses to a handful of rectangles.

The problem is *how* it decides where a run ends. It calls
`self.color_of(cell_at(x))` for **every visible cell**, and `color_of` is a
linear scan over a small cache. How much that costs depends entirely on how
many cells are on screen, and the shipped configs are small:

- `configs/2d_large_moore_256.json` at `scale = 1` is 65,536 cells in a
  256-pixel square, scanning a cache of two or three entries. That is well
  under a millisecond per frame — not a defect anyone can see.
- The Scenario panel's resize controls (`Inputs::grid_width` /
  `grid_height`, range 1–2000) allow a 2000×2000 grid, and
  `1d_large_rule30_2049.json` with `history_limit_1d` raised to 900 fills a
  1600×900 window at `scale = 1`. *Those* are millions of scans per frame to
  produce a few hundred rectangles.

So measure on a grid that can actually show the problem (§2.4). If the paint
time turns out to be dominated by egui's tessellation rather than by
`color_of`, keep the change as a cleanup but do not count it as a win.

**The fix.** Compare the **cell type** instead of the colour. `CellType` is a
newtype wrapper around `lasso2::Spur`, which is a `u32` — so comparing two cell
types is a single integer comparison, versus a cache scan. Resolve a colour only
when the type actually changes.

**The trap — read this before you start.** Two *different* cell types can map to
the *same* colour. A user can assign the same colour to two types in the Colors
panel, or two type names can collide in the same palette slot via
`palette_index_for` in `src/gui/render.rs` (it is an FNV hash modulo the palette
length). If you merge purely on type, those cases emit two adjacent rectangles
where the old code emitted one.

That is visually identical, but the existing tests
`adjacent_same_color_cells_merge_into_one_rect` and
`distinct_colors_split_into_separate_rects` assert on **rectangle counts**, so
they will fail — correctly.

So keep both properties: on a type change, resolve the new colour, and if it
equals the current run's colour, **extend the run anyway** instead of flushing.
That is one extra comparison per *run*, not per cell.

**Add a test the current suite lacks:** a row of two different types that share
a colour must still emit exactly one rectangle.

**Optional, same file:** after the fix, the remaining per-cell cost is the
`cell_at` closure. For 2D that is `g.cell_type(y * w + x)`, which does a
multiply and a bounds-checked `Vec::get` per cell. Indexing a row slice
instead removes both — but `Grid2D::cells` is `pub(crate)`, so it needs a
one-line `pub fn cells(&self) -> &[CellType]` accessor in `cella_lib`. That
crosses the crate boundary this phase otherwise stays out of; take it only if
the §2.4 numbers say the painter still matters after the `color_of` fix.

### 2.2 "Run to +N" is limited by the screen, not the CPU

**Where:** `RUN_TO_STEPS_PER_FRAME` and `tick_play` in `src/gui/sim.rs`;
`request_next_repaint` in `src/gui/app.rs`.

**The problem.** `tick_play` runs at most `RUN_TO_STEPS_PER_FRAME` (100) steps
per frame, and `request_next_repaint` asks egui to wake up again after
`refresh_ms.min(100)` milliseconds. So throughput is capped at about
`100 × (1000 / refresh_ms)` steps per second, whatever the engine can do. With
the default `refresh_ms` of 100 that is roughly **1000 steps per second**, and
a 1,000,000-step run takes about **16 minutes** of wall clock while the engine
sits idle between frames. The toolbar lets the user drag `refresh_ms` down to
10 (the `DragValue` range is `10..=2000`), which lifts the cap to about 10,000
steps per second — still set by the frame rate, still nowhere near the engine.

**The fix.** Budget by time, not by a fixed count:

- Add `const RUN_TO_FRAME_BUDGET: Duration = Duration::from_millis(8);` — about
  half a 60 fps frame, so the UI still feels responsive.
- Loop `while current_step() < target`, breaking once the budget is spent.
- Keep an absolute per-frame step cap (say 1,000,000) as a backstop, so a
  trivial rule on a coarse system clock cannot make the loop effectively
  unbounded.
- In `request_next_repaint`, call `ctx.request_repaint()` (immediately) while
  `self.playback.run_to_target.is_some()`, so consecutive budgets run
  back-to-back.
- **Make Pause actually stop the run.** Today "Run to +N" in the toolbar sets
  `playback.playing = true`, and Pause only flips `playing` back to `false`.
  It leaves `run_to_target` set, and `tick_play` keeps stepping for as long as
  `run_to_target.is_some()`. So Pause does nothing during a burst; the only
  way out is Reset, which throws the state away. At 1000 steps per second
  that is merely annoying. With the engine running flat out for 8 ms of every
  frame it is a trap. Pause must set `run_to_target = None`. While there: a
  finished run currently forces `playing = false` even if the user was already
  playing before pressing "Run to +N"; restore the previous value instead.
- **Write the loop once.** Phase 4 item 2 wants this same time-budgeted loop
  for ordinary playback ("Max" speed). Express the pacing as one enum on
  `Playback` — `Pacing::Interval(Duration)` for the paced path,
  `Pacing::Unbounded` for bursts — so item 2 becomes a one-line switch rather
  than a second copy of the loop.

**Leave the ordinary `playing` path alone.** That one is *supposed* to be paced
by `refresh_ms` — that is the animation speed the user chose.

**Determinism is unaffected:** the number of steps and their order do not
change, only how many run per frame.

**Testability.** `tick_play` reads `Instant::now()` directly, so it cannot be
unit-tested. Split it: leave the clock in `tick_play`, and have it call

```rust
fn run_to_batch(&mut self, target: u64, max_steps: u32) -> u32
```

which is independent of time and can be tested directly.

That has a consequence: `run_to_batch` cannot see the 8 ms budget, because it
cannot see the clock. So `tick_play` must call it in **small chunks** and
check the time between chunks:

```rust
let deadline = Instant::now() + RUN_TO_FRAME_BUDGET;
let mut done = 0u32;
while Instant::now() < deadline && done < RUN_TO_MAX_STEPS_PER_FRAME {
    let n = self.run_to_batch(target, RUN_TO_CHUNK);
    done += n;
    if n < RUN_TO_CHUNK {
        break; // reached the target
    }
}
```

A `RUN_TO_CHUNK` of 32 reads the clock once per 32 steps, which is noise even
on a 50×30 grid. Do not read the clock per step inside `run_to_batch`.

### 2.3 Statistics are recorded for frames nobody sees

**Where:** `step_once` and `stats_record_step` in `src/gui/sim.rs`.

**The problem.** `step_once` unconditionally calls `stats_record_step`. During a
"Run to +N" burst that is a `BTreeMap` walk plus a `VecDeque` push per tracked
series, per step — and the rolling window (`StatsState::window_len`, default
300) throws nearly all of it away immediately.

**The fix.** Split into two step paths:

- `step_once` — records statistics. Used by the Step button and the paced
  `playing` tick.
- an untracked step — the same work without the statistics sample.

Then `run_to_batch` (from 2.2) uses the untracked step, and `tick_play` calls
`stats_record_step` **once per frame**, after its chunk loop.

*What shipped:* the untracked step is `advance_grid`, which reads no clock
either, and `run_to_batch` loops it directly; `step_once` is
`refresh_play_timer` + `advance_grid` + `stats_record_step`. No third method
was needed.

**What the untracked step must keep.** `step_once` does three things
besides the step itself, and only one of them is the statistics call:

1. It bumps `playback.timed_steps` and refreshes `playback.play_start`. Those
   feed the "Avg ms/step" readout in the status bar; drop them and the readout
   lies. Keep the count — but the two `Instant::now()` reads per step
   (`start.elapsed()` and the fresh `Some(Instant::now())`) belong at the
   batch boundary, not inside it. Do them once per `run_to_batch`.
2. It pushes the current row onto `view.history_1d` for 1D grids. Keep it:
   for 1D that space-time diagram *is* the output, and the row buffer is
   already recycled rather than reallocated.
3. It calls `stats_record_step`. This is the only thing that goes.

**This changes visible behaviour, and that must be documented, not slipped in.**
During a burst the chart samples once per frame instead of once per step. It is
an improvement — with a 300-sample window, a million-step run previously showed
only its last 300 steps anyway — but it is a real change to what the user sees.
Add a note to the statistics section of [`app.md`](app.md). That section is
currently headed "Statistics Panel (Right Side)"; the panel is drawn by
`ui_left_panel`, on the left. Fix the heading while you are there.

**What does not change:** the current/peak table in `ui_statistics` reads
`counts_current` and `peak_counts` straight from the grid, not from the
sampled series, so peaks stay exact through a burst. Only the chart is
affected.

A non-issue, recorded so nobody re-finds it: `step_once` does build and
immediately discard a history row when `history_limit_1d` is 0. But the only
control that sets it (the `DragValue` in `src/gui/panels/scenario.rs`) has
range `1..=10_000`, so no user can reach that state. Not worth a change.

### 2.4 Measure it — and do this first

None of 2.1–2.3 has been measured; they are reasoned from the code. Take the
numbers before touching anything, so the "after" has something to compare
against and a fix that turns out to be noise can be dropped.

**The paint benchmark cannot live in `cella_lib`.** `RowPainter` and
`tick_play` are in the binary crate, `pub(in crate::gui)`, and `cella_lib` is
the binary's *dependency* — it cannot see them. (`cella_lib/tests/long_suite.rs`
benchmarks the engine, which is already fast; it has nothing to say about the
GUI.) Neither crate has a `benches/` directory. So:

- Add an `#[ignore]`d timing test next to the existing `runs_for` helper in
  `src/gui/painter.rs` — it already builds a `RowPainter` with no egui context
  — and run it with
  `cargo test --release --package cella --bin cella -- --ignored paint_bench`.
  Use a synthetic 2000-cell row with a handful of types, painted 900 times, so
  the number stands for a full 1600×900 window at `scale = 1`. Print
  milliseconds per frame.
- For 2.2 the number that matters is wall clock for `Run to +100000` from the
  toolbar. Take it by hand, before and after, on
  `configs/2d_large_moore_256.json`; a stopwatch is accurate enough when the
  expected change is minutes to seconds.

Log both as a new `§8 E<n>` experiment entry in
[`performance.md`](performance.md), matching the format already used there
(the next free number is E10). If the painter is already under a millisecond
before the change, say so and file 2.1 as a cleanup rather than a win.

---

## 3. Phase 3 — external models that describe their own parameters *(done)*

### What Phase 3 landed

Commits `6d7e9b3..ad3654d`.

- **§3.1 — vocabulary and trait defaults**, `cella_lib/src/external.rs`. The
  `ParamValue`, `ParamKind`, and `ParamDesc` types described below, each
  deriving `Clone, Debug, PartialEq, Serialize, Deserialize`, re-exported from
  `cella_lib/src/lib.rs`. Three new `ExternalModel` trait methods — `params`,
  `get_param`, `set_param` — all defaulted (empty list, `None`, and an
  `Err(ModelError::InvalidParam(..))` naming the key, respectively), so a
  model outside this repo that implements none of them keeps compiling.
  `as_any_mut` is untouched.
- **§3.2 — apply-with-rollback**, `Grid2D::set_model_param` in the same file.
  Looks up the parameter's own `ParamDesc` and refuses an unknown or
  `read_only` key; runs the private `check_value_against_kind` to check the
  value against the descriptor's `ParamKind` bounds generically (a `Float` or
  `Int` outside `[min, max]`, or a `Choice` not in `options`) before the model
  is touched; calls `model.set_param`; and, only when the descriptor says
  `reattach`, re-runs `model.attach` to rebuild derived state. On a rejected
  `attach` it restores the value `get_param` reported before the write and
  re-runs `attach` again, so a refused edit cannot leave the model in a state
  `attach` would not accept.
- **§3.3 — `WildfireModel` implements 13 keys**, `cella_lib/src/wildfire.rs`.
  Wind group: `wind_speed`, `wind_from_deg`, `c1`, `c2`. Fire group: `p0`,
  `burn_duration`. Terrain group: `slope_a`, `cell_size`. Spotting group (only
  when `params.spotting` is `Some`): `spotting.p_spot`,
  `spotting.median_distance`, `spotting.sigma`, `spotting.angle_jitter_deg`.
  `seed`, ungrouped and `read_only: true`. Of the 13, only `p0`, `slope_a`,
  and `cell_size` set `reattach: true` — they feed the precomputed `p_base`
  and slope buffers; everything else is read live per chunk or per cell, so
  it needs no rebuild. §3.5's round-trip test (set a parameter, snapshot,
  restore, assert `get_param` still reports the edit) lives alongside these
  in `wildfire.rs`'s test module.
- **§3.4 — the generic panel**, new file `src/gui/panels/model.rs`, wired
  into `ui_left_panel` in `src/gui/app.rs`. `ui_model_params` draws nothing
  when no model is attached, otherwise one control per `ParamDesc` under a
  heading of `model.typetag_name()`, in a section that starts expanded. Its
  decision logic lives in pure, unit-tested functions with no egui dependency:
  `group_params` (stable ordering — ungrouped first, then each group in
  first-appearance order), `commit_on` (a `reattach: false` parameter commits
  on every widget change; `reattach: true` waits for the gesture to end;
  `read_only` never commits), and `gesture_ended` (a drag release, a lost
  focus, or any change made without dragging). `commit_now` joins them to an
  `egui::Response` and refuses to commit a value equal to the one the model
  already holds; the widgets themselves clamp only edits
  (`SliderClamping::Edits`, `clamp_existing_to_range(false)`), so an existing
  off-grid value is shown rather than rewritten. `CellaApp::apply_model_param`
  calls `Grid2D::set_model_param` and, on success,
  `mirror_param_into_initial_state`, which copies the accepted
  key/value into the model cloned inside `scenario.initial_state` with a
  plain `set_param` call — so `reset_to_initial` rewinds the cells but keeps
  the parameter value someone just set with a slider, instead of snapping it
  back to what the scenario loaded with. A failed write goes to the status
  bar via `set_status`; nothing else, since `set_model_param` already rolled
  the model back and the control redraws from `get_param` next frame. 14 new
  tests cover this file and the mirroring behavior in `src/gui/sim.rs`'s test
  module. Grepping `src/gui/panels/model.rs` for `wildfire`/`Wildfire` finds
  nothing, confirming the file stays model-agnostic.
- **Acceptance tests**: the out-of-tree `TestModel` in
  `cella_lib/tests/external_model.rs` grew a `params()` implementation
  covering one parameter per `ParamKind` (`rate`: `Float`, `steps`: `Int`,
  `enabled`: `Bool`, `mode`: `Choice`, plus a read-only `Int`); the in-crate
  `ConstModel` in `cella_lib/src/external.rs`'s own test module grew one
  editable `Float` (`threshold`) and a read-only `Choice` (`out_name`). Both
  models' `attach` rejects a value inside its descriptor's range but above a
  narrower threshold (`TestModel`'s `rate > 0.9`, `ConstModel`'s
  `threshold > 5.0`), which is what makes the rollback branch (value
  accepted by step 3, then rejected by `attach`) reachable and covered.
- **Bit-identity result**: re-running `wildfire_validate` for all six
  validation scenarios (`Bear_2020`, `Brattain_2020`, `Buck_2017`,
  `Chimney_2016`, `Ferguson_2018`, `Pier_2017`) against the Task 1 baseline
  produced six byte-identical JSON files (empty `diff`, matching MD5s).
  Phase 3 moved no IoU, Sørensen, or arrival-time figure — see
  [task-5-report.md](../.superpowers/sdd/roadmap/task-5-report.md) for the
  per-scenario commands and checksums.
- **Still owed**: the manual GL checks in §5 for §3.4 (the wildfire-demo
  panel walkthrough and the "no model" negative case) have not been run —
  this environment cannot launch the GUI (see §1's WSLg/Mesa blocker). The
  walkthrough must include one check the headless tests can only approximate:
  open the demo's panel, touch nothing, and confirm `c2` still reads exactly
  `0.131`. A widget that rewrites an existing value onto its own step grid
  would show `0.13` there and would have written that value to the model and
  to the Reset snapshot.

### The problem

`cella_lib` has a plugin seam: an `ExternalModel` (in
`cella_lib/src/external.rs`) can replace the subrule engine entirely and compute
the next state however it likes. `WildfireModel` in
`cella_lib/src/wildfire.rs` is the first real implementation.

But the only way for an application to reach a model's tunable values is:

```rust
model.as_any_mut().downcast_mut::<WildfireModel>()
```

That requires the caller to **name a concrete type**. Any control panel built on
it would hard-code knowledge of one specific model, so a third-party model in a
downstream crate could never get a UI.

**The goal is the inverse: the model declares what it exposes, and the GUI builds
controls from that declaration — with no model-specific code anywhere under
`src/gui/`.**

### 3.1 New vocabulary in `cella_lib/src/external.rs`

```rust
/// A model parameter's value, as exchanged with an application UI.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    Float(f64),
    Int(i64),
    Bool(bool),
    Choice(String),
}

/// What kind of control a parameter wants, and its valid range.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamKind {
    /// Continuous value; bounds are inclusive.
    Float { min: f64, max: f64, step: f64 },
    /// Discrete value; bounds are inclusive.
    Int { min: i64, max: i64 },
    Bool,
    /// One of a fixed set of names.
    Choice { options: Vec<String> },
}

/// Self-description of one tunable parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamDesc {
    /// Stable machine key, used with `get_param` / `set_param`.
    pub key: String,
    /// Short human label for the control.
    pub label: String,
    /// Optional group heading, so a panel can section related controls.
    pub group: Option<String>,
    /// Optional tooltip text.
    pub help: Option<String>,
    /// Optional unit suffix for display, e.g. "m/s", "°".
    pub unit: Option<String>,
    pub kind: ParamKind,
    /// Whether changing this invalidates derived state, and so requires the
    /// engine to re-run `attach`. See `Grid2D::set_model_param`.
    pub reattach: bool,
    /// Shown but not editable. `set_model_param` rejects writes to it.
    pub read_only: bool,
}
```

Then three new methods on the `ExternalModel` trait. **All three have default
implementations**, so every existing model — including any outside this repo —
keeps compiling untouched:

```rust
/// Parameters this model exposes for interactive tuning. Default: none.
fn params(&self) -> Vec<ParamDesc> { Vec::new() }

/// Current value of `key`, or `None` if this model has no such parameter.
fn get_param(&self, _key: &str) -> Option<ParamValue> { None }

/// Write `key`. Implementations need only check what `attach` does not; the
/// engine re-runs `attach` and rolls back on failure.
fn set_param(&mut self, key: &str, _value: ParamValue) -> Result<(), ModelError> {
    Err(ModelError::InvalidParam(format!("unknown parameter '{key}'")))
}
```

**Keep `as_any_mut`.** It is still useful for typed library callers, and removing
it would be a breaking change for no benefit. The GUI simply stops needing it.

### 3.2 Apply-with-rollback on `Grid2D`

Add alongside the existing `attach_model` and `model_mut` in
`cella_lib/src/external.rs`:

```rust
/// Set a parameter on the attached model, re-validating through `attach`.
///
/// On validation failure the previous value is restored, so a rejected edit
/// cannot leave the model in a state `attach` would not accept.
pub fn set_model_param(&mut self, key: &str, value: ParamValue)
    -> Result<(), ModelError>
```

Steps:

1. Take the attached model, or return `ModelError::InvalidParam` if none.
   (`Grid2D::model` is already `pub`, so no new accessor is needed. Borrow
   `model` and `cells` as separate fields — `attach_model` shows the shape —
   so the `GridView` can be built while the model is mutably borrowed.)
2. Find `key` in `model.params()` to read its descriptor. Unknown key, or
   `read_only`, → `InvalidParam`.
3. **Check `value` against `desc.kind` before the model is touched:** a
   `Float` or `Int` outside `[min, max]`, or a `Choice` not in `options`, is
   `InvalidParam`. This is the only validation most parameters need, and the
   model author writes nothing for it.
4. Save the old value: `let prev = model.get_param(key);`
5. `model.set_param(key, value)?`
6. If `reattach` is true, build a `GridView` and call `model.attach(&view)`. On
   `Err(e)`: restore `prev` (when it was `Some`), re-run `attach` to rebuild
   consistent derived state, and return `Err(e)`.

**Why this is the load-bearing design choice.** `WildfireModel::attach` *already
contains every validation rule* — it checks `p0` is in `[0, 1]`,
`burn_duration >= 1`, `cell_size > 0`, that at least one fuel class exists, that
each `veg_factor >= 0`, and the spotting bounds. Routing edits through `attach`
means a model gets validation **and** derived-state rebuilding for free, and the
value the UI believes it set can never diverge from the value `attach` would
accept.

With step 3 there are now *two* places holding a bound — the `ParamKind` range
and `attach` — and they must agree. Make the agreement a test rather than a
promise: for every descriptor `params()` returns, set the parameter to `min`
and then to `max` and assert `attach` still succeeds. If someone later
tightens `attach` without tightening the descriptor, that test fails. Note
that `wind_speed`, `wind_from_deg`, `c1`, and `c2` are checked by nobody today
(`attach` never looks at them); step 3 gives them bounds for free.

**Why `reattach` is per-parameter and not always true.** `attach` rebuilds a
slope buffer of `8 × width × height` floats. On a 256×256 grid that is over half
a million values. Running it on every frame of a slider drag would stutter
badly.

Most parameters do not need it. `wind_speed` and `wind_from_deg` are read live,
per chunk, by `WildfireModel::dir_factors`, which recomputes the eight
per-direction wind factors from scratch each time. `burn_duration` is read
live per cell in `next_type`, and every `spotting.*` value is read live per
cell in `spot_target`. Nothing is precomputed from any of them, so they can be
set with no rebuild at all — their range checks are covered by step 3 above.
Only `p0`, `slope_a`, and `cell_size` feed the precomputed `p_base` and slope
buffers, so only those three need the rebuild. (An earlier draft marked
`burn_duration` and `spotting.*` as `reattach`, which would have rebuilt half
a million slope values on every spot-probability change just to run a range
check.)

### 3.3 Reference implementation on `WildfireModel`

In `cella_lib/src/wildfire.rs`. **Purely additive** — no change to `step_chunk`
or any numerical path, so no validation figure can move.

| `key` | Label | Kind | Group | `reattach` |
|---|---|---|---|---|
| `wind_speed` | Wind speed | `Float { 0.0, 30.0, 0.1 }`, unit `m/s` | Wind | no |
| `wind_from_deg` | Wind from (compass) | `Float { 0.0, 360.0, 1.0 }`, unit `°` | Wind | no |
| `c1` | Wind coefficient c1 | `Float` | Wind | no |
| `c2` | Wind coefficient c2 | `Float` | Wind | no |
| `p0` | Base ignition probability | `Float { 0.0, 1.0, 0.01 }` | Fire | yes |
| `burn_duration` | Burn duration | `Int { 1, 1000 }`, unit `steps` | Fire | no |
| `slope_a` | Slope coefficient | `Float { 0.0, 1.0, 0.01 }` | Terrain | yes |
| `cell_size` | Cell size | `Float { 0.1, 1000.0, 1.0 }`, unit `m` | Terrain | yes |
| `spotting.p_spot` | Spot probability | `Float { 0.0, 1.0, 0.01 }` | Spotting | no |
| `spotting.median_distance` | Median spot distance | `Float`, unit `cells` | Spotting | no |
| `spotting.sigma` | Spot distance spread | `Float { 0.0, 5.0, 0.05 }` | Spotting | no |
| `spotting.angle_jitter_deg` | Spot angle jitter | `Float { 0.0, 180.0, 1.0 }`, unit `°` | Spotting | no |

`wind_from_deg` replaced `wind_dir_deg` on 2026-09-01 after a tester read
0° as a north wind. The old field was the grid angle the wind blew *toward*
(0° = +x, 90° = +y) — a convention we had invented; Alexandridis only
defines a relative angle, and every weather source and operational
simulator (ERA5, FARSITE, Prometheus/Cell2Fire, WindNinja) uses the
meteorological bearing the wind comes *from*, 0° = north, clockwise. That
is now what the field, the config files, the scenario format (v2) and the
panel all hold. The kernel converts once per chunk
(`wind_toward_grid_deg`: `toward = from + 90°`, north at row 0). Old files
with `wind_dir_deg` are rejected (`deny_unknown_fields`), never
reinterpreted; convert with `from = toward − 90°`.
| `seed` | Seed | `Int`, `read_only: true` | — | n/a |

Every `Float` needs a real `min` and `max`, or step 3 of `set_model_param` has
nothing to check. The rows above that say only `Float` (`c1`, `c2`,
`spotting.median_distance`) need bounds chosen from the runs in `validation/`
— the widest range that still produced sane spread rates. Give
`spotting.median_distance` a positive `min`; `attach` rejects zero.

Two deliberate decisions:

- **`seed` is not live-editable.** Changing it mid-run would silently break the
  reproducibility that everything in `validation/` depends on. Mark it
  `read_only: true`; the panel renders it as a plain label and
  `set_model_param` refuses to write it. (An earlier draft encoded this as an
  `Int` whose `min` and `max` both equal the seed. That works, but it is a
  rule every third-party model author and the panel must both know without
  being told. One explicit field is cheaper than one hidden convention.)
- **`spotting.*` returns `None` when `params.spotting` is `None`**, and
  `params()` omits the whole Spotting group in that case. The panel then adapts
  with no special-casing, which is exactly the property being tested.

### 3.4 The generic panel

New file `src/gui/panels/model.rs`, registered in `src/gui/panels/mod.rs` and
called from `ui_left_panel` alongside the colours and statistics panels.

- Render nothing when no model is attached.
- Heading is `model.typetag_name()` — already available on the trait object via
  the `typetag` crate, so no new trait method is required.
- Read `model.params()`, group by `ParamDesc::group`, and map each `kind` to a
  widget: `Float` → `Slider` with `step_by`, `suffix`, and `on_hover_text`;
  `Int` → `DragValue` with `.range()`; `Bool` → `Checkbox`; `Choice` →
  `ComboBox`. Sliders must also set `.clamping(SliderClamping::Edits)` (and
  `DragValue` `.clamp_existing_to_range(false)`), so a value that is already
  off the step grid or outside the range is *displayed* rather than rewritten:
  egui 0.35 defaults to clamping existing values on every draw and reporting
  that as a change, which would make "commit on `changed()` for cheap
  parameters" write a value nobody touched the first time the panel is drawn.
- **Commit timing follows `reattach`:** use `response.changed()` for
  `reattach: false` so cheap parameters stay smooth under a drag, and, for
  `reattach: true`, the end of the gesture — `drag_stopped() || lost_focus() ||
  (changed() && !dragged())` — so the expensive rebuild happens once per
  gesture rather than once per frame. The last clause matters: a focused slider
  takes arrow keys, and a keyboard edit is a whole gesture with no drag in it.
  Guard every commit with "the widget's value differs from the model's", so a
  bare click or a Tab away never pays for a rebuild that changes nothing.
- `read_only` descriptors render as a label, never a control.
- On `Err` from `set_model_param`, report via the existing `set_status` and
  redraw the widget from `get_param` — rollback has already restored it.

**Reset must not undo a slider.** `reset_to_initial` in `src/gui/scenarios.rs`
restores `scenario.initial_state`, which holds a *clone* of the model taken at
load time — with the parameters it had then. Without extra work: edit wind,
press Reset, wind snaps back. That is the wrong surprise for someone tuning a
model; Reset should rewind the cells, not the sliders. After a successful
`set_model_param`, apply the same key and value to the model inside
`initial_state` (a `GridState::D2 { model: Some(..), .. }`) with a plain
`set_param` — it is not attached to a grid, and `Grid2D::from_state` re-runs
`attach` on restore anyway. Mention this in the app.md note from §2.3 too.

**Keep egui out of the decision logic.** Automated tests cannot easily enter
egui closures (see §1), so put everything worth testing in pure functions:

```rust
fn group_params(descs: Vec<ParamDesc>) -> Vec<(Option<String>, Vec<ParamDesc>)>
fn commit_on(desc: &ParamDesc, changed: bool, ended: bool) -> bool
fn gesture_ended(changed: bool, dragged: bool, drag_stopped: bool, lost_focus: bool) -> bool
```

Only the thin adapter that reads those four flags off an `egui::Response` and
compares the widget's value with the model's needs a real `Ui`, and
`egui::__run_test_ui` gives the tests one of those headlessly.

Group order must be stable: the ungrouped bucket first, then groups in
first-appearance order. This mirrors how `RowPainter` is already tested through
the `runs_for` helper in `src/gui/painter.rs`.

### 3.5 Saving and loading

**No new serialization work is needed.** Parameters live in each model's own
serde-derived fields, and `Box<dyn ExternalModel>` already round-trips through
`typetag` in both `CellaConfig` and `GridState`. So a value edited in the GUI is
captured by "Save Final State" for free.

Add one integration test that proves it: set a parameter, snapshot, restore, and
assert `get_param` returns the edited value.

### 3.6 The acceptance test

The real proof that no per-model GUI code is needed: extend the `TestModel`
that already lives in `cella_lib/tests/external_model.rs` so it declares one
parameter of each `ParamKind` and nothing else, then assert that `params()`,
`get_param`, `set_param`, and rollback all behave. The existing `ConstModel` in
`external.rs`'s own test module is the natural place to grow a `params()`
implementation for the in-crate case.

**Cover the rollback branch on purpose.** The branch where `set_param`
accepts a value and `attach` then rejects it needs a model whose `attach` can
fail on a parameter, and `ConstModel::attach` only checks grid width today.
Give it one parameter that `attach` rejects above a threshold, with a
descriptor range deliberately *wider* than that threshold so step 3 lets the
value through, and assert that after the rejected write `get_param` returns
the old value. Without this the branch is uncovered and the ≥ 99% coverage
bar (`make coverage`) drops.

If that test passes and `src/gui/panels/model.rs` contains no mention of
wildfire, Phase 3 succeeded.

---

## 4. Phase 4 — usability and fun *(open)*

Ordered by value per line of code. Every item lands in a module that Phase 1
created.

1. **Keyboard shortcuts** — `src/gui/interact.rs`, extending `handle_hotkeys`.
   `Space` play/pause, `→` or `S` step, `Ctrl+R` reset, `+`/`-` zoom, `G` grid
   lines, `Ctrl+Shift+Z` redo. Only `Ctrl+Z` exists today. Add a `?` hover
   list in the toolbar so they are discoverable. Two traps in the function as
   it stands:
   - It returns early when `playback.playing` is true, so `Space` cannot pause
     from inside it as written. Narrow that guard to the *editing* keys (undo,
     redo); playback keys must work while playing.
   - It never checks `ui.ctx().wants_keyboard_input()`. Without that, typing
     `30` into the Wolfram code box, or a type name containing `s`, `r`, or
     `g`, would step, reset, or toggle the grid. Add the check first, before
     any new key. Reset gets `Ctrl`, not a bare key, because it throws away
     unsaved painting with no confirmation.
2. **Speed in steps per second, not "Refresh ms"** — `src/gui/panels/toolbar.rs`.
   Keep `refresh_ms` as the stored field; present it as a logarithmic slider
   labelled "Speed" with a `steps/s` suffix, plus a "Max" toggle that selects
   `Pacing::Unbounded` from §2.2 — no second loop. "Refresh ms" is
   implementation jargon, and
   it is also *inverted* — a bigger number means slower, which is the opposite of
   what a speed control should do.
3. **Redo stack** — `src/gui/state.rs` and `src/gui/interact.rs`. Mirror
   `EditState::undo_stack` with a `redo_stack`, cleared on any new edit. The
   batch type is already `Vec<(usize, CellType)>`, so this is symmetric with
   undo.
4. **Zoom to fit** — `src/gui/panels/toolbar.rs`. Set `self.view.scale` from the
   central panel's available size and the grid dimensions. This removes the main
   first-run annoyance: loading a 256×256 config at the default `scale: 8` shows
   you one corner of it. The toolbar is drawn before the viewport in `ui`, so
   this frame's viewport size is not known yet; have `ui_viewport` store its
   `available_size()` in `ViewSettings` and read last frame's value in the
   toolbar. One frame of lag is invisible.
5. **Random fill** — `src/gui/scenarios.rs`. A density slider plus a type picker
   fed by the existing `declared_types()`, seeded from a visible seed field so a
   fill is reproducible. This is the single biggest "fun" lever, because it makes
   the app playable without hand-painting cells or writing a JSON config.
   The binary has no `rand` dependency and should not grow one for this:
   `cella_lib::wildfire::cell_rand(seed, step, idx, stream)` is public,
   deterministic, and already what the engine uses — call it per cell with
   `step = 0` and a fresh stream number. Write cells through `set_cell` so
   `Grid2D::transition_state_and_buffer` fires the model's `on_paint` hook and
   keeps `counts_current` right; do not poke the cell vector directly. Validate
   the chosen type once up front — `set_cell` prints and sets a status message
   per failure, and 65,536 failures is not a status message.
6. **Brush size** — `src/gui/interact.rs`, in `handle_paint`. A radius of 1–15,
   writing every affected cell into one undo batch. The current one-cell brush
   makes seeding anything larger tedious. `handle_paint` currently
   de-duplicates with `batch.iter().any(|(j, _)| *j == idx)` — a linear scan
   of the batch for every painted cell. With a one-cell brush that is fine.
   With radius 15 a single stamp is about 700 cells and a drag pushes the
   batch into the thousands, so the scan goes quadratic. Keep a
   `HashSet<usize>` of touched indices in `EditState` for the lifetime of the
   stroke and clear it when the batch is closed.
7. **Pattern stamps (2D)** — new `src/gui/patterns.rs`. Glider, lightweight
   spaceship, R-pentomino, and acorn as `&[(i32, i32)]` offset tables, stamped at
   the cursor through the existing edit path, one undo batch each.
8. **CSV export for statistics** — `src/gui/export.rs`. The series already live
   in `StatsState::history`; reuse the `rfd::FileDialog` path already in that
   file.

Items 1–4 are plumbing over state that already exists. Items 5–7 add state but
reuse the painting and undo machinery Phase 1 consolidated. Item 8 reuses the
existing file-dialog code.

---

## 5. Verification

### The commands that actually gate this repo

```bash
# The `cella` binary
cargo clippy --workspace -- -D warnings
cargo test  --package cella --bin cella

# The library — separate build root, must be run from its own directory
cd cella_lib && CELLA_ASCII=0 cargo test --package cella_lib
make coverage    # = cd cella_lib && CELLA_ASCII=0 cargo llvm-cov --package cella_lib --html; the >= 99% bar
```

`make clippy`, `make test`, and `make coverage` wrap these; see the `Makefile`.

### Two traps in the tooling

These were both found the hard way while establishing a baseline. Neither is a
bug to fix — they are facts to work around.

**1. `cargo clippy --workspace` never lints `cella_lib`.** The root
`Cargo.toml` has no `[workspace]` section, so `cella_lib` is a *path dependency*
rather than a workspace member. Clippy lints members only; dependencies are
merely compiled. So the standard gate covers the `cella` binary alone.

Phases 1 and 2 are confined to the binary, so this does not matter there.
**Phase 3 touches `cella_lib`**, so lint it explicitly:

```bash
cd cella_lib && cargo clippy --package cella_lib -- -D warnings
```

Expect **22 pre-existing errors** (16 `collapsible_if`, 3 `clone_on_copy` on
`CellType`, 2 `too_many_arguments`, 1 `redundant_closure`). Compare against that
count. Do **not** fix them as a drive-by — that is unrelated churn in a
determinism-critical crate — and do not add new ones.

**2. Do not use `--all-targets`.** On `cella_lib` it reports **169**
pre-existing errors in test code, which drowns any real signal.

### The determinism gate

Simulation output must stay bit-identical for a given seed. The repo already has
the right tool: the FNV snapshot tests, regenerated with
`CELLA_UPDATE_SNAPSHOTS=1` (or `make test-create-snapshots`). They fail on any
change to simulation output. No ad-hoc scripting is needed.

Note that `cella_lib/examples/wildfire_validate.rs` takes a **scenario
directory**, not a `configs/*.json` file:

```bash
cd cella_lib && cargo run --release --example wildfire_validate -- \
    ../validation/data/scenarios/Bear_2020
```

Available scenarios: `Bear_2020`, `Brattain_2020`, `Buck_2017`, `Chimney_2016`,
`Ferguson_2018`, `Pier_2017`.

**Phases 1 and 2 cannot move library snapshots** — they only touch the binary,
so running them is cheap confirmation rather than the real risk. **Phase 3 does
touch `cella_lib`**: re-run the validation harness over those scenarios and
confirm the metrics in [`../validation/EXPERIMENT_LOG.md`](../validation/EXPERIMENT_LOG.md)
reproduce exactly. Phase 3 is additive and must not move a single IoU or
Sørensen figure.

### Baselines to diff against

Captured after Phase 1, at commit `b092beb`:

| Suite | Count |
|---|---|
| `cella` binary | **18** passed (was 10 before Phase 1: +6 `cell_index_at`, +2 widgets) |
| `cella_lib` unit | 94 passed, 1 ignored |
| `config_tests` | 8 |
| `edge_cases` | 22 |
| `external_model` | 18 |
| `long_suite` | 3 passed, 47 ignored |
| `randomness` | 2 |
| `soa_robust` | 15 |
| `wildfire_stats` | 0 passed, 2 ignored |
| doc tests | 9 |

### Manual GUI checks

All of these need a machine with working GL — see the blocker in §1.

```bash
cargo run --release -- --gui --size=1600x900
```

- **§2.1** — resize to 2000×2000 from the Scenario panel (or load
  `1d_large_rule30_2049.json` and set the 1D history limit to 900), set scale
  to 1, press Play. Frame time should drop and the picture must be
  pixel-identical. `2d_large_moore_256.json` at scale 1 is a 256-pixel square
  and will not show a difference either way.
- **§2.2** — "Run to +100000" should finish in seconds, not minutes. The Pause
  button must stay clickable throughout **and must actually stop the run**
  (today it does not — see §2.2).
- **§3.4** — load `configs/2d_wildfire_demo.json`. A panel titled with the model
  name appears already expanded, with Wind / Fire / Terrain / Spotting groups.
  Dragging Wind direction visibly bends the fire front on the next step, with
  no stutter. Without touching anything, check that `c2` still reads `0.131`
  exactly, the value the demo config sets — the controls must display an
  off-grid value, not round it onto the slider's step grid. The controls hold
  you inside each range (the `p0` slider stops at its end stops, a typed value
  is clamped before the panel sees it), so no out-of-range value can reach the
  model; a status-bar error appears only if a model's own `attach` refuses a
  value its descriptor allowed, which no wildfire parameter does today. Seed
  shows as a label, not a control, and hovering it shows its tooltip. Change
  Wind speed, press Reset: the new wind speed must survive.
- **§3.4, negative case** — load `configs/life.json`, which has no model. No
  model panel should appear at all.
- **§4** — exercise every shortcut. Confirm that Randomize with a fixed seed,
  followed by a `Run to +N`, reproduces identically across two runs.
