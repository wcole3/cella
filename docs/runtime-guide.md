# How long will my run take? Rules of thumb for runtime

This page is for someone who has a Cella setup (a config, a rule, a grid
size) and wants to guess how long it will take, and which knob to turn to
make it faster. It gives short rules, each with a number and the reason
behind it. You do not need to know how the engine works inside. If you want
the engine's internals, the optimization history and the raw experiments,
read [performance.md](performance.md) instead; that is the contributor log.

Words used on this page, defined once:

- **Cell**: one square of the grid. It holds one type (Alive, Forest, ...).
- **Step** (generation): one tick of time. Every cell looks at its
  neighbors and a rule decides what type it becomes.
- **Cell-step**: one cell updated for one step. A 100x100 grid run for 50
  steps is 100 x 100 x 50 = 500 000 cell-steps. **All the cost numbers below
  are per cell-step**, so you can scale them to your own grid.
- **ns**: nanosecond, one billionth of a second. 1 ns per cell-step on a
  1 000 000-cell grid is 1 ms per step.
- **Neighborhood**: which nearby cells a rule looks at (Moore = the 8
  surrounding cells, and so on).
- **History** (`history_limit`): how many past types each cell remembers.
- **Fast path** (the "bit-packed" path): a special, much quicker way to step
  a few simple kinds of rule. It stores one *bit* per cell instead of a
  4-byte type, so the CPU handles 64 cells in a single instruction.
- **Scalar path**: the general way, one cell at a time. It works for every
  rule, but it is slower.
- **Chunk**: a slice of the grid (a block of rows) given to one worker
  thread, so several threads can step the grid together.

**Contents**

1. [Cheat sheet](#cheat-sheet)
2. [Quick estimate recipe](#quick-estimate-recipe)
3. [Settings that silently knock you off the fast path](#settings-that-silently-knock-you-off-the-fast-path)
4. [Grid size](#grid-size)
5. [Rule shape: fast path or scalar](#rule-shape-fast-path-or-scalar)
6. [Neighborhood shape and range](#neighborhood-shape-and-range)
7. [Number of cell types and subrules](#number-of-cell-types-and-subrules)
8. [What is on the grid matters too](#what-is-on-the-grid-matters-too)
9. [History](#history)
10. [Randomness](#randomness)
11. [Threads](#threads)
12. [Wildfire model and ensembles](#wildfire-model-and-ensembles)
13. [Debug build versus release build](#debug-build-versus-release-build)
14. [Using the GUI](#using-the-gui)
15. [Measured on](#measured-on)

---

## Cheat sheet

"Effect" is the change in run time compared with the setting in the middle
column's "base". Every number was measured on one machine (see
[Measured on](#measured-on)); treat the *ratios* as roughly transferable and
the absolute times as specific to that machine.

| Setting | Typical effect | Why |
| --- | --- | --- |
| Build with `--release` | **8 to 16 times faster** than a plain `cargo run` | The debug build turns the optimizer off. Always use `--release`. |
| Grid size | Time is **proportional to the number of cells** (double the cells, double the time) | Every cell is updated every step. Per-cell cost drifts up only 20 to 60 % from 32x32 to 1024x1024. |
| Number of steps | Proportional | Same reason. |
| Rule is a two-state, radius-1, deterministic "Life-like" rule | **About 3 times faster** than the same rule made ineligible | It takes the bit-packed fast path. See [the list of ways to lose it](#settings-that-silently-knock-you-off-the-fast-path). |
| 1D Wolfram rule with two states and radius 1 (Rule 30) | About **1.7 to 2 times faster** than the scalar path | The same fast path, but less to gain in 1D. |
| Third cell type, randomness, radius 2 or more, or mixed neighborhoods | Lose the fast path: **+150 to +210 %** for 2D, **+70 to +80 %** for 1D | The fast path only understands the simple shape. |
| History: 0 to 1 | **+20 to +30 %** on the fast path, **+5 %** on a scalar rule | Every cell gets a ring buffer to write into, even if it is only 1 long. |
| History: 1 to 7 | About **+1 %** on a small grid. **+15 to +40 % on big grids** | Depth is cheap until the history buffers stop fitting in the CPU cache (see [History](#history)). |
| Neighborhood size (scalar rule) | Cost rises **slowly with neighbors**: 4 neighbors about 14 ns, 8 about 21 ns, 24 about 24 ns | There is a fixed cost per cell; neighbors are the smaller part. Early exit hides a lot of it. |
| Rule uses `Gt` (at least N) | **Up to 2 times faster** than `Eq` on a large neighborhood | Counting stops as soon as N matching neighbors are found. |
| Many cell types present at once (12 or more) | **+55 % at 12, +100 % at 24** versus 3 | Per-cell population counting scans a list of types. |
| Extra subrules that never match | **Almost free** in 2D (+0.6 % for 36 extra), a little cost in 1D | A subrule is only examined for cells of its `current_type`. |
| Randomness on a scalar rule | **+10 % to +30 %** | One hash draw per matching, randomized subrule per cell. |
| Randomness on a rule that would have used the fast path | **About 2.5 times slower** | Randomness disables the fast path. |
| Threads (scalar rule, grid of 256x256 or more) | **2 to 5 times faster** with 4 to 16 threads, not 16 times | Only a big enough grid is split; waking threads costs time. |
| Threads (grid below about 130x130, or any fast-path 2D rule) | **No change** | Small work is not split; the fast path ignores threads. |
| Wildfire model (Bernoulli spread) | About **4.5 ns per cell-step** whether the fire is tiny or large | A fixed pass over the whole grid dominates; the fire front adds only a little. |
| Wildfire with `spread: "arrival"` | **About 1.1 to 1.3 times** the Bernoulli rule (about 5 to 6 ns per cell-step) | Since DS-005 it uses the same fire-front shortcut as Bernoulli. Before that it was about 20 times slower (80 to 93 ns). |

---

## Quick estimate recipe

```
time  ≈  cells  x  steps  x  (ns per cell-step)  /  1 000 000 000   seconds
```

Pick the ns per cell-step from this table. These are single-threaded
numbers (`threads=1`), history 0, from the machine described in
[Measured on](#measured-on). "Typical" means a randomly filled starting
grid, which is a fairly busy case; see [What is on the grid matters
too](#what-is-on-the-grid-matters-too).

| Kind of run | ns per cell-step (1 thread) |
| --- | --- |
| 2D Life-like rule, fast path | 3.5 to 5.5 |
| 2D Life-like rule, scalar path (not eligible) | 12 to 15 |
| 2D 3-type cyclic rule, Moore radius 1, scalar | about 21 |
| 2D scalar, small neighborhood (4 neighbors) | about 14 |
| 2D scalar, large neighborhood without early exit (Moore radius 2, `Eq`) | about 48 |
| 1D Rule 30, fast path | 4.4 to 7 |
| 1D scalar rule, 3 types | about 11 |
| Wildfire, Bernoulli spread | about 4.5 to 5 |
| Wildfire, arrival spread | about 5 to 6 |

Then add the setting multipliers from the sections below (history, many
types, randomness) and divide by the thread speedup if the grid is large.

**Worked example 1.** Conway's Life on a 256 x 256 grid for 1 000 steps, no
history.

- Cells = 65 536. Cell-steps = 65 536 x 1 000 = 65.5 million.
- Fast path, about 4.3 ns: 65.5 M x 4.3 ns = **0.28 s**.
- Same rule if it had lost the fast path, about 13 ns: **0.86 s**.
- With `history_limit = 7` on the fast path (about 5.6 ns): **0.37 s**.

**Worked example 2.** A three-type cyclic rule on a 1024 x 1024 grid for 500
steps with 16 threads.

- Cells = 1 048 576. Cell-steps = 524 million.
- One thread, about 21 ns: 11 s.
- 16 threads: this measured 3.1 to 3.8 ns per cell-step (5.6 to 6.9 times
  faster than one thread, see [Threads](#threads)), so **1.6 to 2.0 s**.

**Worked example 3.** The same Life grid at 1024 x 1024 for 1 000 steps, one
thread, fast path (5.3 ns per cell-step): 1.05 billion cell-steps = **5.6 s**.
That is a 16-times bigger grid and 20 times the time of example 1, which
shows the small per-cell slow-down at large sizes.

These are estimates. Expect a spread of roughly plus or minus 30 % because
of the starting pattern and the machine.

---

## Settings that silently knock you off the fast path

This is the most important section. The fast path is about 3 times faster
in 2D, and nothing tells you when you lose it; the run is just slower. It is
chosen **by looking at your rule and your grid** every step, and falls back
to the scalar path whenever anything does not fit.

**Why it exists.** For a two-state rule with a radius-1 neighborhood, the
next state of a cell depends on only two things: its own state, and *how many*
of its neighbors are Alive (0 to 8). The engine builds a tiny lookup table of
those answers once. It then stores the grid as bits and adds up the
neighbors for 64 cells at a time. Any rule that cannot be boiled down to
such a table cannot use it.

### 2D: what the rule must look like

A 2D rule takes the fast path only if **all** of these hold:

1. Every subrule counts the **same** neighbor type (say `Alive`), which is
   not `Inactive`.
2. Every subrule uses **radius 1** (`range = 1`). Radius 2 or more is
   ineligible.
3. Every subrule uses the **same neighborhood shape** (all Moore, or all Von
   Neumann, and so on). Mixing shapes between subrules is ineligible.
4. **No subrule has `randomness`**. Even `randomness = 0.0` counts.
5. The only types a rule mentions as `current_type` or `output_type` are the
   counted type and `Inactive`. So **one extra subrule that mentions a third
   type, even one that never matches, disqualifies the whole rule.**
6. No external model is attached (the wildfire model has its own path).

And the **grid** must hold only those two types. If you paint one cell with
a third type while the run is going, the engine notices, uses the scalar path
for as long as that cell is there, and goes back to the fast path when it is
gone. The answer is identical either way; only the speed differs.

The fast path is used at every thread count. It does not use extra threads
(it is faster than the threaded scalar path anyway).

### 1D: what the rule must look like

A 1D rule takes the fast path only if it is **exactly two subrules**, one
for `current_type = X` and one for `current_type = Inactive`, with the same
Wolfram code, radius `n = 1`, no randomness, and X as both the counted and
output type (Rule 30 and friends, as written in the bundled configs). One
more subrule of any kind, a third type, radius 2 or 3, or randomness each
drops it to the scalar path.

There is a second 1D catch: **the 1D fast path only runs when the step is not
split across threads.** Splitting happens at about 133 000 cells for a
Rule-30-sized rule (see [Threads](#threads)), so wider rows run the threaded
scalar path instead. Measured on a 262 144-cell row: one thread (fast path)
6.8 ns per cell-step, 16 threads (scalar, split) 4.5 ns. The threaded scalar
path wins here, so you do not lose; you just do not get both.

### What each change costs (measured, 2D, 256 x 256, one thread)

| Rule | ns per cell-step | Ratio |
| --- | --- | --- |
| Life, B3/S23, eligible | 4.25 | 1.0 |
| Same rule plus one inert subrule on a ghost third type | 13.0 | **3.1** |
| Same rule with `randomness = 0.01` on every subrule | 11.8 | 2.8 |
| Same rule with `randomness = 0.0` (never skips, still disqualified) | 12.9 | 3.0 |
| Same rule but a second subrule uses a different neighborhood shape | 10.8 | 2.5 |
| Radius 2 version of Life (a different rule, 24 neighbors) | 22.2 | 5.2 |

(1D, 65 536 cells: eligible Rule 30 6.8 ns, with one inert extra subrule 11.7
ns, with `randomness = 0.01` 12.2 ns; ratios 1.7 and 1.8.)

**How to tell if you have lost it.** There is no log line. The easiest test
is timing: a Life-like rule on a 256 x 256 grid that takes about 4 ns per
cell-step single-threaded is on the fast path; 11 to 14 ns means it is not.

---

## Grid size

**Rule: time is proportional to cells x steps.** Each cell is visited each
step, so double the cells and the time doubles. The cost *per cell* is nearly
flat, with a mild rise on large grids as the grid stops fitting in the CPU
cache. Measured, one thread, history 0:

| Grid | Fast-path Life | Scalar Life-like | Scalar 3-type cyclic |
| --- | --- | --- | --- |
| 32 x 32 | 4.01 | 12.4 | 20.1 |
| 64 x 64 | 3.40 | 12.0 | 21.1 |
| 128 x 128 | 3.76 | 12.2 | 21.0 |
| 256 x 256 | 4.26 | 13.1 | 20.7 |
| 512 x 512 | 4.73 | 13.9 | 20.6 |
| 1024 x 1024 | 5.32 | 14.6 | 20.5 |

(ns per cell-step.) From 64 x 64 to 1024 x 1024 the fast path gets 56 %
slower per cell and the Life-like scalar rule 21 % slower, because a 1024 x
1024 grid plus its buffers does not fit in the CPU's fast memory. The
cyclic rule is flat; its work per cell is large enough to hide the memory
cost. So a clean guess is "time = cells x steps x one number", good to about
plus or minus 30 % across a 1000-fold range of grid sizes.

Where extra threads start to help is a different question; see
[Threads](#threads).

---

## Rule shape: fast path or scalar

This is covered in detail [above](#settings-that-silently-knock-you-off-the-fast-path).
In short, on this machine:

- 2D: fast path 3 to 5 ns per cell-step, scalar 11 to 15 ns, a larger or
  busier scalar rule 20 to 50 ns.
- 1D Rule 30: fast path 4.4 to 7 ns, scalar 8.7 to 11.7 ns. The 1D fast path
  gains less because most of the remaining cost is bookkeeping (ages, history,
  population counts) rather than rule evaluation. That bookkeeping is the same
  on both paths.

The fast path's own cost is mostly that bookkeeping too, so it varies with how
busy the grid is: a Life grid with 35 % of cells alive costs 4.3 ns, an empty
one 3.2 ns. A busier Von Neumann rule that was tried cost 8.6 ns and still took the
fast path (it was 16.8 ns with one inert subrule added), because more cells
change per step.

---

## Neighborhood shape and range

**Rule: on a scalar rule, cost grows with the number of neighbors, but slowly.**
Each cell has a fixed cost of roughly 11 to 14 ns on top of about 0.5 to 1.5 ns
per neighbor actually examined. Measured on a 3-type cyclic rule using `Gt 2`
(256 x 256, one thread, ns per cell-step):

| Neighborhood | Range 1 | Range 2 |
| --- | --- | --- |
| Von Neumann (diamond) | 14.1 (4 neighbors) | 23.4 (12) |
| Straight line (four arms) | 14.2 (4) | 20.7 (8) |
| Langton (diagonals) | 14.7 (4) | 21.7 (8) |
| Moore (square) | 20.8 (8) | 23.9 (24) |
| Knight | 22.0 (8) | 24.9 (40) |

The number in brackets is how many neighbors the shape has. Radius 1 to 2
costs only +15 % for Moore even though the neighbor count triples. The reason
is **early exit**: with `Gt N` ("at least N") the engine stops counting as
soon as it has found N matching neighbors, so it often visits only a few of
the neighbors, however many there are.

You lose that help when the rule cannot stop early. Same grid, Moore
neighborhood, comparing the operator (ns per cell-step):

| Operator | Range 1 | Range 2 (24 neighbors) |
| --- | --- | --- |
| `Gt 1` (at least 1) | 18.6 | 21.8 |
| `Gt 2` | 20.7 | 23.9 |
| `Gt 4` | 19.9 | 29.5 |
| `Gt 8` | 22.9 | 40.4 |
| `Eq 2` (exactly 2, no early exit) | 24.0 | **47.5** |
| `Eq 8` | 22.5 | 48.1 |

So on a large neighborhood, `Eq` and large `Gt` thresholds can be **twice as
slow** as small `Gt` thresholds. (The `Eq` case counts every neighbor because
it needs the exact number.)

**`Gt 0` is special.** A subrule of the form "`Gt 0` of type X" is satisfied by
zero neighbors, so the engine skips the neighbor scan completely. In a test
on a three-type cyclic rule, `Gt 0` cost 9.6 ns against 18.3 ns for `Gt 1`.
That is a different rule (it always matches), so it is not a speed trick for
you to adopt blindly, but if your rule really is "always do this", writing it
as `Gt 0` is the cheap way.

**Fast path caveat.** On the fast path the work per cell is the same for every
shape: it always adds up eight neighbor bits and ignores the ones the shape
does not include. Radius 1 is the only radius it supports. (Differences you
see between shapes on the fast path come from how busy the pattern is, not
from the shape.)

---

## Number of cell types and subrules

Two different things are easy to confuse here: the number of **types**, and
the number of **subrules**.

**Subrules that never match are almost free in 2D.** A cell only examines the
subrules whose `current_type` equals its own type. Adding 36 subrules for types
that no cell ever has changed the time of a 3-type rule from 20.87 to 21.01 ns
(+0.6 %). In 1D it is not free: adding 4 inert subrules to a scalar Rule 30
took it from 12.0 to 13.3 ns (+11 %), and 12 inert ones to 15.1 ns (+26 %).

**Many types actually present on the grid cost more.** Each step also counts
the population of every type (the live counts in the GUI's Stats tab and in
`counts_current`). That count is a short list scanned per cell, so a grid
that is a fine mix of many types pays more. Measured on a cyclic rule with a
random start (128 x 128, one thread, 2 subrules per type):

| Types | Subrules | ns per cell-step | vs 3 types |
| --- | --- | --- | --- |
| 2 | 4 | 17.8 | 0.85 |
| 3 | 6 | 21.0 | 1.0 |
| 6 | 12 | 20.8 | 1.0 |
| 12 | 24 | 32.8 | 1.56 |
| 24 | 48 | 41.8 | 2.0 |

Up to about 6 types there is no visible penalty; after that it grows. In 1D
(the 1D cyclic rule used here) cost went 11.0, 16.9, 24.5 ns at 3, 6, 12 types, mixing a
type effect and the subrule effect. (These sweeps did not separate the two causes; the
reason given for the 2D numbers, the type-count scan, is the engine's design
as described in [performance.md](performance.md) section 1, not something
isolated with a profiler.)

**A sparse grid is cheaper.** One type, usually `Inactive` (the background),
is the "dominant" type, and the engine does not count it cell by cell; it
fills it in by subtraction. So a grid that is mostly background, such as a
small pattern in a big empty field, avoids most of the counting cost.

---

## What is on the grid matters too

The same rule on a different starting pattern can cost noticeably different
amounts per cell. Same 256 x 256 grid and rule, one thread:

| Starting pattern | ns per cell-step |
| --- | --- |
| 3-type cyclic rule, random start | 20.9 |
| Same rule, diagonal stripes | 13.5 |
| Same rule, every cell the same type | 18.0 |
| Scalar Life-like rule, 35 % alive | 12.9 |
| Same, 5 % alive | 11.1 |
| Same, empty | 11.1 |
| Fast-path Life, 35 % alive / 5 % / empty | 4.3 / 3.3 / 3.2 |

Reasons: noisy patterns make the CPU guess wrong on the "does this subrule
match?" branches (it is a speed penalty for unpredictable data), and cells
that change need their age and history updated, while cells that stay put
are cheaper. So a random start is the slow end of the range. A rule that
settles to a stable picture gets cheaper over time.

---

## History

`history_limit` is how many past types each cell remembers (0 to 255). It
costs time because every step writes the old type into a per-cell ring buffer
(a small circular array), and memory because there is one such array per
cell.

**Rule: turning history on gives an immediate cost, and extra depth after
that is almost free until the history stops fitting in the CPU cache.**

Fast-path Life on 256 x 256 (one thread, ns per cell-step):

| `history_limit` | ns | vs 0 | vs 1 |
| --- | --- | --- | --- |
| 0 | 4.24 | 1.00 | |
| 1 | 5.48 | **1.29** | 1.00 |
| 2 | 5.50 | 1.30 | 1.00 |
| 4 | 5.50 | 1.30 | 1.00 |
| 7 | 5.57 | 1.31 | **1.02** |
| 16 | 5.70 | 1.34 | 1.04 |
| 64 | 13.5 | 3.2 | 2.5 |
| 255 | 13.3 | 3.1 | 2.4 |

The same sweep on a **scalar** 3-type rule: 20.6, 21.7, 21.7, 21.7, 21.9, 21.9
ns at limits 0, 1, 2, 4, 7, 16. That is **+5 % for 0 to 1 and +1 % for 1 to
7**. The jump costs less in relative terms there because the scalar path is
already slow. For a 1D fast-path Rule 30 (65 536 cells): 6.8, 7.2, 7.3, 7.3,
7.3, 7.5 ns at 0, 1, 2, 4, 7, 16, which is **+6 %**, then +1 %, then +3 % at
16.

(performance.md's own history benchmark, on a different rule, found +19 % for
0 to 1 and +1 % for 1 to 7. This guide's numbers differ because they use a
different rule and pattern, but the shape agrees.)

**Why "turning it on" costs and "making it deeper" does not.** With history
on, each cell also needs a write position and a count. The per-cell work is
the same for a ring of 1 and a ring of 7; only the amount of memory touched
differs.

**The exception: large grids and deep histories.** History memory is
**cells x `history_limit` x 4 bytes**. If that stays in the low megabytes it
fits in the CPU cache and depth is nearly free, as above. Once it is larger
than the cache, it is not. Fast-path Life, ns per cell-step:

| Grid | history 0 | 1 | 7 | 16 | 32 |
| --- | --- | --- | --- | --- | --- |
| 128 x 128 | 3.74 | 5.00 | 5.07 | 5.20 | |
| 256 x 256 | 4.30 | 5.47 | 5.55 (1.8 MB) | 5.74 (4.2 MB) | 7.11 (8.4 MB) |
| 512 x 512 | 4.74 | 6.08 | 7.06 (7.3 MB) | 12.05 (16.8 MB) | 16.6 (33.6 MB) |
| 1024 x 1024 | 5.28 | 6.43 | 7.52 | 10.44 | |

So the "1 to 7 costs about 1 %" rule holds for grids up to about 256 x 256.
At 1024 x 1024 going from 1 to 7 costs **+17 %** and 1 to 16 **+62 %**. The
rough rule: **keep cells x history x 4 bytes under about 4 MB** to keep depth
cheap, and expect a clear slowdown above about 8 MB. (The reason is an interpretation of the pattern; no cache
profiler was run. The machine reports 512 KB of L2 cache per core and 16 MB of L3
shared.)

**Practical advice.** If you do not use the per-cell history (for example the
age layer needs no history, only the 1D history rows in the GUI and some
Explore metrics use it), leave `history_limit` at 0; it is the cheapest
setting. If you want it, 1 to 7 is a very cheap range on small grids.

---

## Randomness

A subrule can have `randomness` (the chance it is skipped when it matches).
The random numbers come from a hash of (seed, step, cell, subrule), so there
is no shared generator to slow threads down, and the run is reproducible.

**Rule: on a rule that is already scalar, randomness costs +10 % to +30 %;
on a rule that would have used the fast path, it costs about 2.5 times,
because it disables the fast path.**

| Case | ns per cell-step | Ratio |
| --- | --- | --- |
| 3-type cyclic, 256 x 256, no randomness | 20.7 | 1.00 |
| Same, `randomness 0.01` on the advance subrules | 22.9 | 1.11 |
| Same, 0.5 | 23.2 | 1.12 |
| Same, 0.99 | 27.0 | 1.31 |
| Fast-path Life, no randomness | 4.29 | 1.00 |
| Same, `randomness 0.01` | 11.8 | 2.75 |
| Same, 0.5 | 10.8 | 2.5 |
| 1D Rule 30, 65 536 cells, none / `0.01` | 6.80 / 12.2 | 1.8 |

The draw happens only when a subrule that has `randomness` matches a cell, so
the cost scales with how often that subrule matches (hence the climb at 0.99
in the cyclic rule, where the draw result almost always decides). The Life
numbers for 0.5 are lower than for 0.01 partly because a random Life rule at
0.5 changes the pattern a lot (it thins out), and thin grids are cheaper; do
not read that as "more randomness is cheaper".

The 1D `0.01` result of 12.2 ns is about the same as the same rule made scalar
without randomness (11.7 ns): the random draws themselves cost only about 4 %
in 1D. Almost all of the loss is the fast path.

---

## Threads

By default the engine uses all the cores your OS reports. Set it with a
`cella.properties` file (the line `threads=4`, in the current directory or up
to four directories above it), or from Rust with
`cella_lib::threads::set_thread_override(n)`. **The `cella.properties` that
ships at the root of this repository sets `threads=4`**, so running from the
repo folder uses 4 threads, not all of your cores, unless you edit it.

### The 400 000 "work" threshold, in plain words

Splitting a step across threads has a cost: each worker thread has to be
woken up, and the main thread has to wait for all of them to finish. On the
machine measured, waking one sleeping worker costs on the order of 30 to 40
microseconds and a four-way hand-off about 170 microseconds (details in
[performance.md](performance.md) section 3.2). If the whole step only takes
100 microseconds, splitting it makes it *slower*.

So the engine estimates how much work a step is, and only splits it if the
work is big enough. The estimate is

```
work  =  cells  x  (sum over subrules of the number of neighbors it examines)
chunks = work / 400 000   (rounded down), at least 1, at most `threads`
```

Each chunk is a block of rows given to one thread. `400 000` is the constant
`MIN_WORK_PER_CHUNK`. The estimate is deliberately high, because it counts
every neighbor of every subrule even though early exit and non-matching
subrules skip most of them.

Example: a 3-type cyclic rule with 6 subrules of 8 neighbors has 48 work
units per cell. A 128 x 128 grid is 16 384 x 48 = 786 432 work, which is 1
chunk (it would need 800 000 for 2). A 256 x 256 grid is 3.1 million, so up
to 7 chunks. A fast-path Life rule is never split.

**Below the threshold, extra threads do nothing.** The step runs on one
thread no matter what `threads` is.

### Measured effect of threads

Scalar 3-type cyclic rule (48 work per cell), ns per cell-step, and the
speed-up over 1 thread in brackets:

| Grid (work) | 1 thread | 2 | 4 | 8 | 16 |
| --- | --- | --- | --- | --- | --- |
| 64 x 64 (197 k) | 21.3 | 21.0 | 21.0 | 21.0 | 21.0 |
| 128 x 128 (786 k) | 20.9 | 21.0 | 20.9 | 21.1 | 21.0 |
| 256 x 256 (3.1 M) | 21.2 | 12.4 (1.7x) | 9.2 (2.3x) | 10.3 (2.1x) | 10.4 (2.0x) |
| 512 x 512 (12.6 M) | 21.7 | 11.2 (1.9x) | 6.3 (3.4x) | 5.3 (4.1x) | 5.8 (3.8x) |
| 1024 x 1024 (50 M) | 21.0 | 10.5 (2.0x) | 5.7 (3.7x) | 4.6 (4.6x) | 3.8 (5.6x) |

Scalar Life-like rule (inert extra subrule, 32 work per cell):

| Grid | 1 thread | 2 | 4 | 8 | 16 |
| --- | --- | --- | --- | --- | --- |
| 128 x 128 | 12.2 | 12.4 | 12.4 | 12.5 | 12.4 |
| 256 x 256 | 13.0 | 12.4 | 7.0 (1.9x) | 7.5 | 7.7 |
| 512 x 512 | 13.8 | 7.4 (1.9x) | 4.5 (3.1x) | 4.5 (3.1x) | 5.1 (2.7x) |
| 1024 x 1024 | 14.6 | 7.4 (2.0x) | 4.3 (3.4x) | 3.4 (4.3x) | 3.5 (4.2x) |

Fast-path Life: 3.4, 3.8, 4.3, 4.7, 5.3 ns at 64, 128, 256, 512, 1024, the
same for 1, 4 and 16 threads.

What to take from this, on this machine:

- Threads help only past the threshold, and the speed-up is far below the
  thread count: about 2x with 2 threads, 3 to 4x with 4 to 8, and 4 to 6x with
  16, even on a 1024 x 1024 grid. Memory speed and the hand-off cost limit it.
- **More threads is not always faster.** At 256 x 256 and 512 x 512, 16
  threads were slower than 4 or 8. A mid-sized grid is best with a middling
  thread count. When unsure, 4 is a safe choice for most grids up to 512 x
  512, and use the full count only for 1024 x 1024 and larger.
- Fast-path rules and small grids gain nothing from threads. Do not raise
  `threads` for them.

**A cheaper-per-cell rule or an ordered pattern can flip the sign.** The
performance log (section 4, DS-004 scenarios) found that for a 3-state cycle
starting from diagonal stripes, 2 chunks were neutral and **4 chunks about 15
% slower than 1 thread** on mid-size grids (about 183 x 183). The reason is that cheap cells (about 8
ns per cell there) do not carry enough real work per chunk to pay for the
hand-off, while the engine's estimate over-counts. The slowdown could **not be reproduced**
with the sweeps for this guide on the same kind of rule (at 183 x 183,
4 threads was 1.5 times *faster* than 1 thread, for both random and striped
starts). The difference is probably the different cost per cell: it is
in the right direction (cheap cells gain less), but that
explanation has not been verified. Treat "mid-size grid, cheap cells" as the case where threads
may not help, and measure yours with `threads=1` versus `threads=4`.

**This is WSL2.** All of the above was measured on WSL2 (Linux inside
Windows), where thread wake-up is unusually slow (the log notes about 5 to
20 microseconds on native Linux for the four-way hand-off, against about 170
here). On native Linux the same grids may speed up more and at smaller sizes.
The constant 400 000 was set from WSL2 measurements, so native Linux might
want a lower one (see `CELLA_MIN_WORK` below). Native
Linux has not been measured.

### Two environment variables

Both are read once when the process starts and do nothing unless set:

- `CELLA_MIN_WORK=<n>` replaces 400 000. Lower it to split smaller grids;
  raise it to split fewer.
- `CELLA_MEMBER_PAR=<n>` is for ensembles (next section).

---

## Wildfire model and ensembles

### One fire: cost tracks the whole grid first, the fire front second

The wildfire model has its own fast shortcut: a fire cell can only change if
it is burning or touches a burning cell. So the engine finds the "fire
front" and runs the expensive per-cell math only there. This is why the
wildfire model is about 50 % faster than before the shortcut
([performance.md](performance.md) section 8, E8).

But the shortcut has a floor. Everything else the engine does each step
(copying the grid, updating ages and counts, and building the map of burning
cells) is a pass over **every** cell. The cost of one step was measured as a
fire grew and then burned out on a 512 x 512 grid (one thread, Bernoulli
spread, history 0):

| Step range | Burning cells (avg) | Step time | ns per cell |
| --- | --- | --- | --- |
| 0 to 39 | 187 | 1.17 ms | 4.46 |
| 40 to 79 | 606 | 1.19 ms | 4.53 |
| 120 to 159 | 1 476 | 1.22 ms | 4.66 |
| 200 to 239 | 2 366 (peak) | 1.24 ms | 4.73 |
| 280 to 319 | 1 202 | 0.95 ms | 3.61 |
| 360 to 399 | 1 048 | 0.87 ms | 3.32 |

An **idle** grid with no fire cost 1.17 ms per step, the same as a grid with a
fire of three cells. The marginal cost of a burning cell is roughly 30 to 50 ns
(about 0.07 ms more step time for 2 200 more burning cells), versus about 4.5 ns per
cell for *every* cell. So:

**Rule: a Bernoulli wildfire step costs about 4.5 ns x cells, whatever the
fire is doing; a big fire adds only a few percent.** The fire front is why it
is not 20 ns or more per cell, but it is not why it is cheap to run a big
fire on a small grid; area still dominates. (This corrects a loose reading of
"cost tracks the perimeter": it is the *extra* cost that tracks the
perimeter. The floor tracks the area.) The cost per step also drifts down
about 25 to 30 % late in the run, as burnt-out area grows; the cause was not
investigated.

Whole 150-step runs, one thread, ns per cell-step:

| Grid | Bernoulli | + spotting | + history 7 | Arrival spread |
| --- | --- | --- | --- | --- |
| 128 x 128 | 4.22 | 4.54 | 5.00 | 6.0 |
| 256 x 256 | 4.75 | 4.84 | 5.51 | 5.6 |
| 512 x 512 | 4.72 | 4.90 | 5.88 | 5.1 |
| 1024 x 1024 | 4.96 | 5.28 | 6.62 | 5.2 |

- **Spotting** (firebrands that start fires far away) adds 2 to 7 %.
- **History 7** adds 18 to 33 %.
- **`spread: "arrival"` costs about 5 to 6 ns per cell-step, close to the
  default Bernoulli rule** (4.2 to 5 ns). It used to be about 20 times slower
  (80 to 93 ns), for two reasons that were fixed under DS-005 (see
  [performance.md](performance.md) section 8, E11): the compiler was running
  the per-cell random "jitter" maths (a logarithm, a cosine and an
  exponential) for every cell instead of only cells next to fire, and the
  rule re-examined all eight neighbors of every cell each step instead of
  using the fire-front shortcut. The arrival column in the table above was
  re-measured after both fixes (one thread, 150 steps, same landscape as the
  Bernoulli column). Arrival is still a little dearer than Bernoulli because
  a cell next to the fire does more work (it looks at every burning or burned
  neighbor).

Threads on a Bernoulli wildfire (ns per cell-step):

| Grid | 1 thread | 2 | 4 | 16 |
| --- | --- | --- | --- | --- |
| 128 x 128 | 4.22 | 4.20 | 4.20 | 4.20 |
| 256 x 256 | 4.75 | 3.61 (1.3x) | 3.56 (1.3x) | 3.52 (1.3x) |
| 512 x 512 | 4.72 | 2.80 (1.7x) | 2.14 (2.2x) | 4.00 (1.2x) |
| 1024 x 1024 | 4.96 | 2.67 (1.9x) | 1.56 (3.2x) | 1.51 (3.3x) |

Again, 16 threads is slower than 4 at 512 x 512.

**Example.** A 1000 x 1000 grid, Bernoulli spread, no history, 500 steps:
1 000 000 x 500 x 4.9 ns = **2.5 s** on one thread, around 0.8 s with 4 to
16 threads.

### Ensembles (many copies of one run)

An ensemble (see [explore.md](explore.md)) steps *N* copies of the grid, so
the time is roughly **N times the cost of one run** (an estimate: ensembles were not
re-run for this page). A 32-member ensemble of a 500 x 500 wildfire
for 100 steps is about 32 x 250 000 x 100 x 4.9 ns = 3.9 s of single-thread
work, less with threads.

When there are several members and several cores, the engine can step
members at the same time. These numbers come from
[performance.md](performance.md) section 9 and were not re-measured. They
are seconds for a 32-member ensemble over 5 observation days on the machine
below with 16 threads:

| Scenario (cells) | Default heuristic | `CELLA_MEMBER_PAR=16` | Speed-up |
| --- | --- | --- | --- |
| Bear_2020, Bernoulli (463 k) | 11.3 | 4.9 | 2.3x |
| Bear_2020, arrival | 42.5 | 23.7 | 1.8x |
| Ferguson_2018, Bernoulli (1.5 M) | 25.8 | 17.1 | 1.5x |
| Ferguson_2018, arrival | 100.1 | 76.3 | 1.3x |

(These ensemble timings were measured before the DS-005 fixes and were not
re-measured. Arrival stepping is now far cheaper, so expect the arrival rows
to drop towards the Bernoulli rows; the ratio of 2.3x to 4.9x shown here is
the old, slow arrival rule.)

**Rule: for a single ensemble run on a 16-core machine, set
`CELLA_MEMBER_PAR` to your thread count to step members side by side.** The
result is bit-identical; only the scheduling changes. The same document found
that if you launch several ensemble *processes* at once, leave that alone;
four concurrent processes beat one-at-a-time, and forcing the knob inside
four concurrent processes gained only about 11 %.

---

## Debug build versus release build

`cargo run` with no flag builds a **debug** build: no optimizations, and the
program is much slower. Always use `--release` for anything you time or
watch:

```
cargo run --release -- --gui
```

Measured, one thread, same grids, release versus debug (ns per cell-step):

| Case | Release | Debug | Debug is slower by |
| --- | --- | --- | --- |
| 2D Life, fast path (128 x 128) | 4.48 | 71.4 | 16x |
| 2D Life-like, scalar | 13.4 | 208.9 | 16x |
| 2D 3-type cyclic | 21.1 | 163.2 | 7.7x |
| Wildfire (128 x 128) | 4.83 | 48.9 | 10x |
| 1D Rule 30 (16 384 cells) | 6.85 | 73.3 | 11x |

(The release and debug columns are separate runs of one test program on the
same machine, so each pair differs only in the build.) A debug run of the
Life example above (65 million cell-steps) would take about 4.7 s instead of
0.28 s.

---

## Using the GUI

The simulation steps are the same as above; the GUI adds drawing time.

- **Speed slider** is steps per second, 1 to 1000. It never goes faster than
  you ask for. **Max** and **Run to +N** instead run as many steps as fit in
  an 8 ms slice of each frame, so how many steps per second you get depends
  on your grid: roughly 1 / (cells x ns per cell-step). For a 256 x 256 fast-path Life
  grid that is about 3 500 steps per second; for a 1024 x 1024 scalar 3-type
  rule on one thread, about 50 (an estimate from the numbers above, not a
  GUI measurement; the GUI could not be launched in the test environment, so
  no stopwatch figure exists yet).
- **Painting cost**: drawing the grid is not free. A synthetic benchmark of
  the painter (900 rows of 2 000 cells, a stand-in for a 1600 x 900 window
  at zoom 1) took about 1.2 ms per frame after the E10 optimization
  ([performance.md](performance.md) section 8), against about 3.1 ms before.
  That is small next to a 1000 x 1000 scalar step (about 20 ms), but it
  matters for a fast-path grid that steps in under a millisecond: there, the
  drawing is the larger share.
- **Stats tab** during Run to adds a chart point once per frame, not once per
  step, so it does not slow a long run down.

---

## Measured on

These numbers came from a set of single-factor sweeps: each one changes
exactly one setting from a fixed base and holds the others fixed.

| | |
| --- | --- |
| CPU | AMD Ryzen 7 3800X, 8 cores / 16 threads |
| OS | Linux 6.18 under **WSL2** (Windows Subsystem for Linux), not native Linux |
| Memory | 94 GB visible to WSL |
| Compiler | rustc 1.98.1, release profile (thin LTO, one codegen unit) |
| Code | repository commit `fb9e664`, branch `perf-review` |
| Date | 2026-10-02 |
| Method | A small test program using only the public `cella_lib` API. Each timing is a run of N steps after 20 warm-up steps, repeated 9 times after one discarded run, and the **minimum** is reported (the median was within about 1 to 3 % of the minimum for the single-thread sweeps; threaded runs were noisier). Starting grids are random (35 % alive for Life, uniform types for cyclic rules), history 0, one thread unless the table says otherwise. |
| Machine load | One-minute load average was 1.1 to 2.9 before most batches, with a few threaded batches starting at 3.1 to 4.5, partly left over from the sweep's own 16-thread runs. The single-thread numbers repeated to within 1 % when re-run. |

**Absolute numbers will differ on your machine; the ratios should transfer
roughly.** A faster or slower CPU scales every number together. The things
most likely to differ are the thread numbers (WSL2 versus native Linux, and
how many cores you have) and the cache-related effects (history on large
grids, size drift), which depend on your CPU's cache sizes. Rows marked as an
estimate were computed from other rows rather than timed directly. For the
slow and slightly noisy cases (threads on mid-sized grids, wildfire at
512 x 512 with 16 threads) re-measure on your own box before relying on them.
The quickest way to do that is to time a short run of your own config with
`threads=1` and with `threads=4`.

For the engine internals and the full experiment log, see
[performance.md](performance.md).
