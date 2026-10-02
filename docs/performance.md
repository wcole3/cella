# Cella Performance Review

> **Contributor notes.** This is a developer document about how fast the
> `cella_lib` engine is and why; you do not need it to use cella. To use the
> app or library, start with [the README](../README.md), [app.md](app.md),
> [lib.md](lib.md), or [explore.md](explore.md) (ensembles and search). This
> file is a dated working log: sections are marked *(done)* or *(open)*, and
> it keeps the measurements behind each decision.

A review of the `cella_lib` simulation engine, originally written against the
struct-of-arrays (SoA) / double-buffer refactor (July 2026) and **updated after
acting on it**. It covers the current architecture and its optimizations, the
correctness issues found, what was implemented and measured, and what remains.

**Status: the §2 correctness fixes and the §3.1–§3.3, §3.6–§3.11 performance
items have been implemented and measured, and most of §3.12 too.** §3.4 was
deliberately not done and §3.5 was folded into §3.12. Still open: §3.13, §3.14,
and the last §3.12 item. (The benchmark harness in §4 was hardened instead of
moving to criterion.) Sections marked
*(done)* describe shipped code; sections marked *(open)* are still
recommendations. §7 records the performance decisions baked into the
wildfire/external-model work. Line references are omitted in favour of naming
functions, since line numbers drift.

Headline: the first pass (§3.1–§3.6) took the benchmark suite total from
**1329 ms to 840 ms (−37 %)** on the reference machine. The later rounds in §8
(including the bit-packed fast paths of §3.7) brought the comparable 40-entry
total down to **580.67 ms**. Output stayed cell-for-cell identical throughout:
the FNV snapshot tests in `tests/snapshots/` (stored hashes of the final grid)
pass unchanged.

---

## 1. Current Architecture and Existing Optimizations

The engine is a straightforward synchronous stepper: every cell is re-evaluated
every step against an ordered list of subrules.

### Struct-of-arrays (SoA) grid layout

Both `Grid1D` and `Grid2D` store cell data as parallel flat vectors rather than a
`Vec<CellState>` of per-cell structs:

| Field | Type | Purpose |
|---|---|---|
| `cells` | `Vec<CellType>` | current types, row-major in 2D |
| `next_cells` | `Vec<CellType>` | double buffer for the next step |
| `ages` | `Vec<u32>` | steps spent in current state |
| `history_data` | `Vec<CellType>` | flat circular history; cell *i* owns `[i*hl .. (i+1)*hl)` |
| `history_heads` / `history_counts` | `Vec<u8>` | write head / entry count per cell (`history_limit <= 255`) |

`CellState` (with its per-cell `VecDeque` heap allocation) survives only as a
serialization intermediate, used by `state.rs` and the custom `Deserialize`
impls. The live stepping path never touches it.

### Interned cell types

`CellType` is a 4-byte `Copy` wrapper around a `lasso2::Spur` handle into a global
`ThreadedRodeo` interner. Type comparison in the hot loop is a single integer
compare; string resolution (`as_str`) only happens at serialization/display
boundaries.

### Double buffering

`step()` writes the next generation into `next_cells` and then `mem::swap`s the
buffers. No per-step allocation of a new grid.

### Incremental population counting

Each `step_chunk` accumulates new-cell counts into a `TypeCounter` — a linear-scan
`Vec<(CellType, u64)>` that avoids HashMap hashing for the typically <20 distinct
types. The **dominant type** (usually the background/Inactive type, i.e. the vast
majority of cells) is *skipped* during counting and back-filled by subtraction
from the total in `rules::apply_counts`. On a sparse grid this removes the counter
update for most cells.

### Precomputed neighborhood offsets

2D neighborhood shapes are computed once per `(neighborhood, range)` pair via
`#[memoize(SharedCache)] neighborhood_offsets` and cached on each `Rule2DSubrule`
as a sorted `offsets: Vec<(i32,i32)>`, rederived on deserialize. The Knight
neighborhood's BFS reachability (`knight_reachable`) only runs at rule
construction. `Rule2DSubrule::new` additionally precomputes `pad` (the
neighborhood's Chebyshev radius) and `early_exit`.

### Early exit in neighbor counting

For the common `Gt`-without-`limit` case, counting stops as soon as the threshold
is reached. `Gt`/`Lt` semantics are inclusive (`>=` / `<=`, see
`Rule2DSubrule::eval_condition`), so the early exit at `neighbors >= count` is
correct. The predicate is precomputed as `Rule2DSubrule::early_exit` rather than
re-derived per neighbor.

### Inlined rule evaluation with a branchless interior *(done, §3.1)*

The grids inline neighbor lookup and counting instead of going through a
closure-based API. Each grid has two evaluators:

- `next_type_interior` — for cells at least `pad` from every border. Neighbors are
  reached by adding **precomputed linear offsets** to the cell index: no
  per-neighbor bounds comparisons and no `y * width + x` multiply.
- `next_type_edge` — the bounds-checked path, treating out-of-range neighbors as
  `inactive`.

1D folds its `2n+1` window into a Wolfram-code bit index with the `n` arms spelled
out (a loop over a runtime `n` does not unroll and measured slower).

### Chunked parallelism sized by work *(done, §3.2)*

Above a work threshold, `step()` splits the output buffers into disjoint chunks
(`chunking::split_chunks`) and runs `step_chunk` per chunk on a **persistent rayon
pool** (`threads::pool`), keyed by thread count and reused for the process
lifetime. Cell/age/history chunks are disjoint slices, so no locking is needed;
per-chunk `TypeCounter`s are merged by `reduce`.

Crucially, the chunk *count* is proportional to estimated work rather than a flat
size threshold — see `threads::chunks_for_work` and §3.2 below.

### Benchmark harness

`cella_lib/tests/long_suite.rs` contains a hand-rolled harness (all `#[ignore]`,
run with `cargo test -p cella_lib --release -- --ignored --test-threads=1`):

- FNV-1a hashes of final grid state compared against golden files in
  `tests/snapshots/` — cheap, deterministic regression detection.
- `run_benchmark_1d/2d` time `steps` iterations with `Instant`, repeated
  `CELLA_BENCH_RUNS` times (default 10), summarized (mean ± std, delta vs the
  `tests/benchmarks_last.json` baseline) by `zzz_benchmark_summary`.
- Thread-count variants (`_t1`, `_t4`, `_t8`) via `set_thread_override`.
- `CELLA_BENCH=1` prints every individual run immediately, which is the more
  useful mode for A/B work — see §4.

---

## 2. Correctness Issues Found — all fixed *(done)*

Each fix has a test that fails without it; the "caught by" column names it.

| # | Issue | Caught by |
|---|---|---|
| 2.1 | `TypeCounter::merge` dropped entries | `soa_robust::parallel_counts_match_serial_{1d,2d}` |
| 2.2 | 2D dominant-type switching was dead code | `lib::tests::dominant_type_is_re_elected_when_majority_flips{,_1d}` |
| 2.3 | 1D/2D `recompute_counts_from_cells` asymmetry | (unified; no longer two copies) |
| 2.4 | Off-by-one bounds guard in `transition_state_and_buffer` | `soa_robust::transition_state_and_buffer_rejects_out_of_bounds_{1d,2d}` |
| 2.5 | `CountOp` doc comments contradicted the implementation | (docs) |

### 2.1 `TypeCounter::merge` dropped entries (parallel counts corrupted)

```rust
pub fn merge(&mut self, other: &Self) {
    for (t, c) in &other.entries {
        for e in &mut self.entries {
            if e.0 == *t {
                e.1 += *c;
                return;          // <-- exits the whole function
            }
        }
        self.entries.push((*t, *c));
    }
}
```

The `return` was meant to terminate the *inner* search but exited `merge`
entirely: as soon as one incoming type was found in the accumulator, **all
remaining entries of `other` were silently dropped**. `add` has the same shape but
is correct there, because a single add is complete after the early return.

Impact: whenever the multi-threaded path ran and chunk counters shared types —
the normal case — `counts_current` and `peak_counts` were wrong. Cell contents
were unaffected (the merge only feeds statistics), which is why the FNV snapshot
tests never caught it.

Fixed with a labeled `continue 'outer`.

### 2.2 2D dominant-type switching was dead code

`new_dominant_type` was declared `(&CellType, u64)` but the update assigned only
`.0`, so `.1` stayed `0` and the switch condition
`total_count < new_dominant_type.1` could never be true — `dominant_type` never
changed after construction in 2D. The 1D version updated the whole tuple.

Impact: counts stayed *correct* (the dominant type is still back-filled by
subtraction), but the *optimization* silently degraded — once the initially
dominant type stopped being the majority, every step counted the actual majority
type cell-by-cell, exactly the work the skip exists to avoid.

Because counts remain correct either way, this is only observable from inside the
crate; the regression test is a `#[cfg(test)]` unit test asserting on
`dominant_type` directly, not an integration test.

### 2.3 1D/2D `recompute_counts_from_cells` asymmetry

1D inserted with overwrite semantics while 2D used `entry().or_insert`. Both
worked only because the map was `.clear()`ed first. Both are now gone, replaced by
a single `rules::apply_counts` free function shared by the two grids, so they
cannot drift again.

### 2.4 Off-by-one bounds guards in `transition_state_and_buffer`

Both grids guarded with `idx > len` instead of `idx >= len`, so `idx == len`
passed the guard and panicked inside `transition_cell`. These are the interactive
painting entry points, so the panic was reachable from the GUI. Now `>=`.

### 2.5 `CountOp` doc comments contradicted the implementation

The enum doc comments described `Lt`/`Gt` as strict (`<`, `>`), but
`eval_condition` implements inclusive `<=` / `>=`. The implementation is what the
configs and tests rely on (the Game of Life config uses `gt 2` to mean "2 or
more"), so the doc comments were corrected to "at least" / "at most".

---

## 3. Performance Work

Measurements are `cargo test --release`, `CELLA_BENCH_RUNS=12`, on a 16-logical-CPU
WSL2 machine, `cella.properties` `threads=4`. Suite total: **1329 ms → 840 ms**.

Representative per-case results (ms, lower is better):

| Benchmark | before | after | |
|---|---|---|---|
| `stress_config_100x100_t4` | 17.8 | 3.8 | −79 % |
| `2d_large_moore_256_t1` | 244.1 | 156.2 | −36 % |
| `2d_large_moore_256_t4` | 142.2 | 87.7 | −38 % |
| `2d_large_moore_256_t8` | 175.8 | 94.0 | −47 % |
| `2d_large_vn_256_t8` | 111.8 | 54.7 | −51 % |
| `2d_three_state_cycle_t8` | 154.4 | 46.7 | −70 % |
| `2d_life_like_moore_t1` | 8.2 | 5.7 | −33 % |
| `2d_straightline_threshold_t1` | 3.3 | 2.4 | −29 % |
| `2d_vonneumann_threshold_t1` | 3.7 | 3.3 | −12 % |
| `1d_n3_custom_t1` | 1.04 | 1.01 | −3 % |
| `1d_large_rule30_2049_t1` | 23.5 | 28.5 | **+21 %** |
| `1d_three_state_cycle_t1` | 6.2 | 7.6 | **+23 %** |

The two 1D regressions were fixed later: see §3.9, §8 E1, and §8 E6.

### 3.1 Branchless interior stepping *(done)*

Every neighbor access used to branch on bounds: 2D `neighbor()` cost 4
comparisons plus `isize` casts and a multiply *per neighbor, per subrule, per
cell*. For interior cells — the overwhelming majority — the branch always took the
in-bounds path but still cost a compare/branch and defeated auto-vectorization.

Rather than reallocate the grid with a halo border (which would have touched
serialization, every accessor, the GUI, and chunk indexing), this took the
cheaper variant of the same idea: **keep the layout, split the work**.

- `Rule2DSubrule::linear_offsets(width)` converts `(dx, dy)` pairs into linear
  index offsets; `Rule2DPlan` builds them once per step for the whole rule, along
  with the rule-wide `pad` and a `work_per_cell` estimate.
- 2D `step_chunk` tracks whether the current *row* clears the top/bottom margin,
  so the per-cell test reduces to two comparisons on `x`. Interior cells then go
  to `next_type_interior`, which does `cells[idx.wrapping_add_signed(off)]` — no
  bounds logic, no multiply.
- 1D does the same with `applies_interior` / `applies_edge`.

`Rule2DPlan` is rebuilt every step rather than cached on the grid. That costs tens
of operations against tens of thousands of cells, and it means a caller mutating
`grid.rule` between steps can never observe a stale plan.

Measured contribution: this is the bulk of the 2D wins above. Confirmed by
ablation — forcing every 1D cell down the bounds-checked edge path regresses
`1d_large_rule30_2049` from 25.8 ms to 28.6 ms and `1d_n3_custom` from 0.91 ms to
1.52 ms.

This path does not use explicit SIMD. The bit-packed fast paths in §3.7 cover
the rule shapes where packing pays off.

### 3.2 Parallelism: persistent pool, work-proportional split, `hl == 0` *(done)*

Three separate problems compounded here.

**1. Thread spawn per step.** `std::thread::scope` spawned fresh OS threads every
`step()`. Replaced with a persistent, size-keyed rayon pool (`threads::pool`),
leaked for the process lifetime.

**2. Waking a parked worker is not free either.** This turned out to matter more
than expected. A direct microbenchmark of fork/join latency on the reference
machine:

| workers | rayon `install` + `par_iter` | hand-rolled spin barrier |
|---|---|---|
| 2 | 23 µs | 0.10 µs |
| 4 | 187 µs | 0.43 µs |
| 8 | 453 µs | 1.98 µs |

At ~50 µs per worker wakeup, rayon is *the same order of cost as the thread spawn
it replaced*. That is why the original code showed parallelism as a net loss:
`2d_three_state_cycle` measured 41.8 ms at `t1` but 154.4 ms at `t8`.

The fix that shipped is therefore not "use a thread pool" but **size the split by
work**: `threads::chunks_for_work(total_work)` returns
`clamp(total_work / MIN_WORK_PER_CHUNK, 1, thread_count())`, with
`MIN_WORK_PER_CHUNK = 400_000` nominal neighbor visits. `total_work` is
`cells × work_per_cell`, where `work_per_cell` sums each subrule's neighbor count
— an over-estimate for rules that early-exit, which is the safe direction (it
never promotes a grid that is too small to parallelize).

So a cheap rule on a mid-sized grid now stays serial or uses two workers instead
of paying eight wakeups to save a few microseconds of compute. This replaced the
old flat `total < 4096` / `width < 8192` thresholds entirely. Effect:
`2d_three_state_cycle_t8` 154.4 → 46.7 ms, and `t8` is no longer slower than `t4`
on the 256×256 cases.

`threads::set_min_work_per_chunk_override` exists so tests can force the
multi-chunk path on grids small enough to verify exhaustively — without it the
heuristic keeps test grids serial and the parallel merge path goes untested (which
is how bug 2.1 survived).

**3. `history_limit == 0` disabled parallelism entirely** — because
`chunks_mut(chunk * 0)` is invalid — even though `hl == 0` is the *cheapest*
configuration and benefits most from threads. `chunking::split_chunks` now hands
empty history slices to each chunk instead, and the serial fallback no longer
special-cases `hl`.

**Note on the remaining headroom.** The spin-barrier column above is not
hypothetical: a spin-then-park pool would make parallelism profitable at much
smaller grid sizes than 400 k work units. It was *not* implemented, because the
safe formulations of a scoped pool that hands borrowed grid slices to persistent
workers all require an `unsafe` type-erased job pointer, and the crate is
currently 100 % safe code. Also note that ~190 µs for a 4-way fork/join is
unusually bad and probably specific to this WSL2 environment; on native Linux the
figure is typically 5–20 µs, where the current heuristic already engages
correctly. If this is revisited, measure the fork/join latency on the target
platform *first* and re-tune `MIN_WORK_PER_CHUNK` rather than assuming these
numbers transfer.

**Re-investigated 2026-10-01 (ticket DS-001): closed as not worth it.** This
re-check asked three questions. Did the WSL2 numbers still hold? Could a crate
give low latency without `unsafe` in our own code? What would we actually gain?
"Fork/join latency" means the time to hand work to the worker threads and wait
for them all to finish, with almost no work in each chunk.

| approach (persistent pool, trivial chunks) | 2 workers | 4 workers | 8 workers |
|---|---|---|---|
| rayon `install` + `par_iter_mut` (current code) | 41 µs | 166 µs | 452 µs |
| forte 1.0.0-beta.1 (best crate with a safe API) | 17.5 µs | 41 µs | 51 µs |
| paralight 0.0.12 | 76 µs | 101 µs | 171 µs |
| hand-written spin-then-park pool (needs `unsafe`) | 0.31 µs | 0.85 µs | 1.07 µs |

(Medians. Most batches ran with a 1-minute load average above 4, so treat
these as approximate.)

- **The old numbers still hold.** Moving to WSL kernel 6.18 did not help.
  Waking one sleeping worker costs roughly 30–40 µs here.
- **No safe-API crate is good enough.** forte is the fastest, but it is a beta
  that needs Rust 1.96. In a real `Grid2D::step` it was *slower* than serial up
  to 128×128, and only 1.1–1.4× faster at 256×256. That is about what rayon
  already gets. chili runs serially when work is split into flat chunks, and
  orx-parallel and paralight were too slow.
- **The `unsafe` spin pool pays off, but it is fragile.** It was 2.5–4× faster
  on about six benches (the 256×256 grids, `2d_three_state_cycle`, and wildfire
  up to 256×256), and its output was bit-identical. But on a busy machine
  (load 7.6) it got **20–30× slower than serial**. That happens because
  spinning workers fight over the CPUs. The validation runners run several
  16-thread processes at once, which is exactly that situation.

Reopen only if (a) `unsafe` is allowed inside a separate, Miri-tested helper
crate, *and* (b) a measurement on a native (non-WSL) target still shows a large
gap. (Miri is Rust's tool for checking `unsafe` code for undefined behaviour.)
The scratch code and full tables came from the DS-001 spike and were not
committed.

### 3.3 RNG acquisition hoisted out of the per-cell loop *(done, later superseded)*

*Update (2026-09-04, commit `2f04706`):* the `SmallRng` described below no
longer exists. Subrule randomness now uses the same stateless hash as the
wildfire model, `rng::cell_rand(seed, step, cell, STREAM_RULE + subrule)`
(see §7). "Stateless" means there is no RNG object to create or pass around:
each random number is computed from those four inputs. The same commit added
two randomness benches, `1d_randomness_512` and `2d_randomness_128`, with
snapshots. The text below is the original record.

`rand::thread_rng()` was fetched inside `next_type` per firing cell — a TLS lookup
plus `Rc` refcount traffic on every call. Now one `SmallRng` is created per
`step_chunk` invocation and threaded down as `Option<&mut SmallRng>`, and it is
only created at all when `Rule*::needs_rng()` reports that some subrule uses
randomness.

Measured effect on the current benchmark set: none detectable, because no
benchmark uses a randomness subrule (see §5, gap 5 — still open). Ablation
confirmed the plumbing itself costs nothing: reverting 1D to `thread_rng()`
changed `1d_large_rule30_2049` by less than the noise band (26.3 vs 25.8 ms).
The change stands on the ability to seed reproducible randomized runs later.

### 3.4 Replace SipHash maps for counts *(open — deliberately not done)*

`counts_current` / `peak_counts` are `HashMap<Spur, u64>` with the default SipHash
hasher, cleared and rebuilt every step by `apply_counts`.

On measurement this is not worth changing. After the dominant-type skip, these
maps hold at most one entry per distinct cell type — under 20, typically 2–4 — so
a step does a handful of hash operations against tens of thousands of cell
updates. Both fields are also `pub` and read by the GUI, so swapping the hasher
type is a public API change for no measurable gain.

Revisit only if a workload with many dozens of types appears; the options then, in
increasing order of change, are `FxHashMap<Spur, u64>`, syncing the public maps
on demand instead of every step, or a dense `Vec<u64>` indexed by
`Spur::into_inner()`.

### 3.5 Small-buffer offsets *(closed — subsumed by §3.12/§8 E2's flat plan, no SmallVec dependency)*

`sr.offsets` remains a heap `Vec<(i32,i32)>`, and `Rule2DPlan::lin` a
`Vec<Vec<isize>>`. Common shapes are tiny (Moore n=1 → 8, VonNeumann n=1 → 4), so
a `SmallVec<[isize; 8]>` would keep them inline and improve locality. Not done:
it adds a dependency, and after §3.1 the offsets are walked once per matching
subrule per cell from a hot cache line, so the expected gain is small. Worth
bundling with any future §3.7 work.

### 3.6 Cache `thread_count()` outside the step loop *(done)*

The override path took a `Mutex` lock on **every** `thread_count()` call, and
`step()` calls it every step. The override is now an `AtomicUsize` (0 = no
override). One lock per step was minor but pure waste.

### 3.7 Bit-packing and SIMD *(done — both stages shipped, §8 E6/E7)*

**What shipped.** Both stages exist in the code now. Each one is a separate
stepper. A rule uses it only when its shape fits, and a step falls back to the
normal (scalar) path when the grid content doesn't fit:

- **Stage 1, 1D:** `Grid1D::step_packed` (§8 E6) handles pure two-state
  Wolfram n=1 rules (rules like rule 30). It packs the row into `u64` words,
  one bit per cell, so one word holds 64 cells. `1d_large_rule30_2049` got
  43 % faster.
- **Stage 2, 2D:** `Grid2D::step_packed` (§8 E7) handles two-state radius-1
  threshold rules (Life-like rules). It uses bit-planes: one bit per cell,
  64 cells per word. Neighbor counts are summed with SWAR adders ("SIMD within
  a register": ordinary integer operations that work on all 64 cells in a word
  at once). Life-like benches got 58–59 % faster.
- **Tried and rejected:** a general version for more cell types and larger
  radii (§8 E9) ran *slower* than the scalar path on every bench where it
  applied.
- **Not used:** `std::simd`. It is still nightly-only, so the stable-Rust
  routes are SWAR (as above) and compiler auto-vectorization (§3.13).

What is left in the eligible benches is mostly bookkeeping: the per-cell sweep
that updates ages, history, and counts, plus converting bits back to cells
every step. The original recommendation is kept below for context.

**Original recommendation.** For the dominant two-type workloads (Life-like
rules), the 4-bytes-per-cell layout still leaves a lot on the table:

- Pack "is `criteria_type`" as 1 bit per cell per relevant type; neighbor counts
  become shifts + adds (SWAR) or `popcount` over adjacent words. This routinely
  yields 10–50× over scalar per-cell counting.
- Alternatively keep bytes but process rows with `std::simd` / autovectorizable
  interior loops. §3.1 is the prerequisite and is now in place — the interior loop
  no longer carries bounds branches.
- The 1D Wolfram path is even easier: a packed `u64` row implements any n=1 rule
  with three shifts and a table lookup per 64 cells. Given that n=1 is exactly
  where 1D is currently slowest (§3.9), this is the obvious next move for 1D.

This conflicts with the fully generic `CellType`-per-cell model, so it fits best
as a specialized fast path chosen when a rule set is detected to be two-state and
deterministic (the same detection Hashlife would need, §6).

### 3.8 Minor items *(done)*

- `Grid2D::new` did two `entry()` lookups per initial cell; collapsed to one.
- `(h + 1) % history_limit` in the per-cell history write was a **hardware divide**
  on every cell of every step, because `history_limit` is a runtime value. Since
  `h < history_limit` always holds, it is now a compare. This was not in the
  original review and is a straightforward win for every history-enabled workload.
- The hot loops reborrow the `OutChunk` slices into locals up front. Indexing
  through `&mut OutChunk` made the loop reload each slice's pointer and length
  from the struct on every access; hoisting them recovered ~6 % on 1D.
- 1D `next_type` no longer `clone()`s `CellType` into window arrays, and the
  unsupported-`n` case is handled in one place (`applies_*` reports no match)
  rather than being a silent `continue` in the middle of the dispatch.

### 3.9 1D `n = 1` regression *(resolved — §8 E1 and §8 E6)*

*Resolution:* E1 fixed `1d_three_state_cycle` and `1d_rule30_center`. E6's
packed path then took `1d_large_rule30_2049` to 14.66 ms, well under the
22.7 ms it measured before the regression. The analysis below is kept as the
record.

Three 1D benchmarks are slower than before this work, and the pattern is precise:
`n = 1` rules regressed 13–23 %, while `n = 2` and `n = 3` rules improved.
Against HEAD, using best-of-8 (which has a much tighter noise band than the mean):

| Benchmark | HEAD | now | |
|---|---|---|---|
| `1d_large_rule30_2049` (n=1) | 22.7 | 25.8 | +14 % |
| `1d_three_state_cycle` (n=1) | 5.9 | 7.1 | +21 % |
| `1d_rule30_center` (n=1) | 1.28 | 1.45 | +13 % |
| `1d_n2_alt` (n=2) | 1.09 | 1.05 | −4 % |
| `1d_n3_custom` (n=3) | 0.98 | 0.91 | −7 % |

Hypotheses tested and **ruled out** by ablation:

- The hoisted-RNG plumbing (§3.3) — removing it entirely changed nothing.
- Re-loading `cells[idx]` for the window's centre slot instead of reusing the
  already-loaded current type — passing it in changed nothing.
- The interior fast path being a pessimization for 1D — no, removing it is much
  worse (25.8 → 28.6 ms).
- Code growth from inlining both the interior and edge evaluators into the hot
  loop — marking the edge path `#[cold] #[inline(never)]` made it *worse*
  (25.8 → 28.9 ms), and made the whole suite worse (840 → 928 ms).
- Splitting the chunk loop into `[left edge][interior][right edge]` ranges so the
  interior loop carries no per-cell test at all — also worse (→ 28.9 ms).

So the cause is not any of the obvious suspects and is most likely a codegen
effect in the `n = 1` window fold. This is 6–7 ms of absolute cost against a
490 ms overall gain, so it was left open rather than chased further with
`Instant`-based means. The right next steps are (a) move to `criterion`/`divan`
(§4) to get a noise floor tight enough to attribute this, and (b) look at §3.7's
packed-`u64` 1D path, which would make the current n=1 fold irrelevant anyway.

**Addendum — an untested prime suspect.** A later hot-path review found a
candidate none of the ablations above covered: the window fold evaluates
`(s.wolfram_code >> bits) & 1u128` — a **128-bit variable-count shift** — per
cell, which lowers to a multi-instruction `shrd`/`shr`/`cmov` sequence on
x86-64 while `n = 1` only ever uses 3 live bits; the per-cell `match s.n`
dispatch and `applies_edge`'s per-cell re-validation of `n` sit in the same
loop. The natural fix is a `Rule1DPlan` built per step in `Grid1D::step`
(mirroring `Rule2DPlan` — `Rule1DSubrule` has no constructor, so derived state
cannot be cached on the subrule itself) that downcasts the code per subrule:
`u8` for n = 1, `u32` for n = 2, `u128` only for n = 3, hoisting the `n`
validity check out of the cell loop. **Implemented and confirmed (§8 E1)**:
`1d_three_state_cycle` −13.4 %, `1d_n2_alt` −13 %, `1d_rule30_center` −8 %,
n=3 unchanged — the u128 shift was the missing suspect. `1d_large_rule30_2049`
only moved −1 %, so its remaining cost is elsewhere; §3.7's packed-`u64` path
is still the fix for that case.

### 3.10 `pool()` registry lock removed *(done)*

`pool(n)` took a `Mutex` lock and linearly scanned the pool registry on **every
parallel step** — the same class of waste as the `thread_count()` Mutex removed
in §3.6. Pools for `n <= 64` now live in a fixed array of `OnceLock` slots
indexed by `n`, so the steady-state cost is one atomic load; larger `n` (which
no realistic host hits) falls back to the old Mutex registry, and both paths
are covered by `override_chunks_and_pool_paths_are_covered`. Per-step rather
than per-cell, so no measurable benchmark delta is expected — it is strictly
less work on every parallel step.

### 3.11 Release profile: thin LTO + one codegen unit *(done)*

Neither manifest defined a `[profile.release]`, so release builds ran with 16
codegen units and no LTO, and cross-crate inlining into the `cella` binary
relied entirely on `#[inline]` attributes. Both Cargo.tomls now set:

```toml
[profile.release]
lto = "thin"
codegen-units = 1
```

**Both** manifests need the profile because the repo is *not* a cargo
workspace: `cargo` invocations from the repo root and from `cella_lib/` resolve
different build roots, and profiles are only honored from the invoked root.
This invalidated `tests/benchmarks_last.json`; the baseline was refreshed in
the same change. Measured effect on the 40 pre-existing entries: **820.78 →
800.01 ms (−2.5 %)** — at the edge of the harness noise band but consistently
downward, with no per-case regression outside noise. Snapshots byte-identical.

### 3.12 Per-step allocations and loop-invariant dispatch *(mostly done — §8 E2)*

The first three items below shipped as experiment E2 (measured −7 to −12 %
across the threshold benches, see §8); the output-write item remains open:

- `Rule2DPlan::new` allocates `Vec<Vec<isize>>` — one malloc per subrule per
  step, and the innermost loop's `zip(lin)` chases a `&Vec` pointer per
  subrule per cell. Flatten to `lin_flat: Vec<isize>` + `spans: Vec<(u32,u32)>`
  (two allocations per step, slice indexing in the loop). This also closes
  §3.5 with no SmallVec dependency.
- `Rule2DSubrule::eval_condition` is a six-arm `match (op, limit)` per matching
  cell. Precompute an inclusive `(lo, hi)` range on the subrule (serde-skipped,
  like `early_exit`) and the test becomes two compares.
- `TypeCounter::new()` does `Vec::with_capacity(16)` per chunk per step *and*
  per rayon `reduce` identity. A lazy first-`add` allocation makes the reduce
  identity free (also a prerequisite for keeping the external-model event
  merge allocation-free).
- The six independently bounds-checked output writes per cell
  (`next_cells`/`ages`/history arrays) and the per-cell `history_limit > 0`
  test: inspect release asm first; hoist length asserts or move to zipped
  iterators only if LLVM has not already elided them.

### 3.13 Interior bounds-check elision *(open — parked)*

*Note:* this was first written as the step needed before §3.7. §3.7 shipped
without it, using separate bit-packed steppers, so this item now only matters
for large-radius scalar workloads. §8 "Not attempted" explains why it is
parked. References to §3.7 below are from the original text.

The "branchless" interior path still bounds-checks every neighbor read:
`cells[idx.wrapping_add_signed(off)]` is slice indexing with a runtime offset,
so LLVM emits a compare + panic branch per neighbor per subrule per cell —
exactly what blocks the auto-vectorization §3.7 wants, and `std::simd` is
still nightly-only, so autovectorization is the stable-Rust route.

Safe-code shape to evaluate: decompose each neighborhood into per-`dy`
contiguous runs `(row_offset, dx_lo, dx_hi)` at plan time (Moore rows are full
runs, VonNeumann rows contiguous; Langton/Knight degenerate to short runs).
Interior counting then walks one slice window per run —
`&cells[lo..=hi].iter().filter(|c| **c == crit).count()` — one bounds check per
run instead of per neighbor, in a shape LLVM auto-vectorizes over the 4-byte
`CellType`s. Early exit stays correct evaluated between runs. Measured results
decide how §3.7 proper (packed bitplanes, packed-`u64` 1D) is scoped.

### 3.14 `work_per_cell` over-estimation *(open)*

`work_per_cell` sums every subrule's full neighborhood regardless of match
rate, so a rule whose first subrule matches most cells over-estimates several
fold and can promote a grid to more chunks than the real work justifies. Only
measurable once benches exist that straddle `MIN_WORK_PER_CHUNK` (§4); revisit
then.

---

## 4. Benchmarking Notes

Two kinds of check protect the engine, and they are deliberately separate:

- **Snapshots** (correctness): after a fixed number of steps, the final grid is
  reduced to a 64-bit FNV-1a hash and compared to a golden file in
  `tests/snapshots/`. If an optimization changes even one cell, the hash changes
  and the test fails. This is cheap, exact and not noisy, and it is what gave
  confidence that all of §3 preserved behaviour exactly.
- **Timings** (speed): each scenario is run many times and the wall-clock times
  are compared with a stored baseline (`tests/benchmarks_last.json`). This is
  the noisy one, and the rest of this section is about making it trustworthy.

Both live in `cella_lib/tests/long_suite.rs`. The timing benches are
`#[ignore]`d tests (`run_benchmark_1d` / `run_benchmark_2d`), so a plain
`cargo test` skips them. They also build in the `test` profile unless told
otherwise, so **always pass `--release`**.

### Why the old harness was not enough

The first version printed mean ± standard deviation over 10 runs and nothing
else. On this WSL2 machine the run-to-run noise on the mean is roughly
**±5–8 %**, which is the same size as several of the effects worth chasing.
Two A/B rounds during the §3/§8 work produced conclusions that reversed when
re-measured. The reason is that timing noise is **one-sided**: another process,
the WSL2 host or a CPU clock dip can only make a run *slower* than the true
cost, never faster. A few slow runs drag the mean up, while the fastest run
stays close to the truth. Three missing pieces followed from that: no warm-up
(the first run is always slow), no way to see which runs were suspect, and no
way to tell "the machine was busy" from "the code got slower".

### Criterion was tried and rejected

The obvious fix was to move the timings to `criterion`, the standard Rust
benchmarking crate (it does its own outlier rejection and significance tests).
A spike on this machine measured both approaches side by side:

| | legacy harness, **min of 10** | criterion |
|---|---|---|
| Spread across 3 repeats of the *same* code | **0.4–2.5 %** | up to **70 %** under load |
| False positives ("regression" reported when nothing changed) | not applicable (no test) | **14 %** on a quiet box, **57 %** on a loaded one |
| Wall time for the same scenarios | 1× | about **15× slower** |

Criterion's statistics assume a quiet, stable machine; on a shared WSL2 box its
adaptive sampling was both slower and *less* repeatable than "run it 10 times
and take the fastest". So the decision (user, 2026-10) was to **harden the
legacy harness instead** and keep the FNV snapshots exactly as they are. There
is no `cella_lib/benches/` target and no `criterion` dependency.

### The benchmark protocol

Each `run_benchmark_*` call now does the following (all of it lives in
`long_suite.rs`, with doc comments at the top of that file):

1. **Warm-up.** One untimed run first (`CELLA_BENCH_WARMUP`, default 1; set it
   to 0 to switch it off). *Why:* the first run pays for cold CPU caches, page
   faults and clock ramp-up, so it is systematically slow. The warm-up run also
   does the ASCII dump and the snapshot check, so those never sit inside a timed
   region.
2. **Timed runs.** `CELLA_BENCH_RUNS` runs (default 10), each on a fresh clone
   of the initial grid.
3. **Statistics per bench**, all in milliseconds:
   - **mean ± std** over *all* timed runs. These are computed exactly as before
     so every row of the history table below stays comparable with older rows.
   - **min**: the fastest run. Because noise only slows runs down, this is the
     best estimate of the true cost and the number to trust for A/B decisions.
   - **median**: the middle run when sorted (average of the two middle runs if
     the count is even). Less jumpy than the mean, more representative than the
     min.
4. **Outlier report.** Runs that look unlike the rest are *listed*, never
   removed. "Outlier" here means outside the fence
   `median ± max(3 × 1.4826 × MAD, 2 % of the median)`.
   - **MAD** (median absolute deviation) is the median of `|run − median|`: the
     typical distance of a run from the middle run, computed with medians so a
     couple of wild runs cannot inflate it. The constant 1.4826 rescales it to
     be comparable to a standard deviation on bell-curve data, so "3 ×" reads
     like "3 sigma".
   - *Why MAD and not the other common rule (Tukey's fence, quartiles ± 1.5 ×
     **IQR**, where the IQR is the distance between the 25th and 75th
     percentile run):* with only ~10 runs the quartiles are interpolated between
     a handful of points and one slow run already moves them. MAD tolerates up
     to half the runs being bad.
   - The 2 % floor stops a very quiet bench (MAD close to zero) from flagging a
     run that is only 1 % off: real, but harmless. With fewer than 5 runs
     nothing is flagged.
   - Outliers never change `avg`, `std_dev`, `min` or `median`. A bench with
     **more than 20 % outliers** gets an explicit *UNTRUSTWORTHY* warning in
     the end-of-suite report.
5. **Load guard.** `/proc/loadavg` is read at suite start, before each bench's
   warm-up, and when each bench finishes. The stored and printed per-bench value
   (JSON field `load1`) is the **max of the start and end samples**, so a busy
   spell at either end shows (the *load average* is how many processes were wanting a CPU, on
   average over the last minute; this machine has 16 logical CPUs). It is
   printed, stored in the JSON, and a 1-minute load **above 4** prints a warning
   that the machine is too busy for trustworthy timings. Where the file does not
   exist (Windows and macOS CI, which only run `cargo check`) the guard simply
   stays silent.
6. **`Δ` against the baseline is printed on both min and mean.** `Δmin` is the
   one to trust, for the reason above (the spike measured 0.4–2.5 % spread on
   min-of-10 versus 5–8 % on the mean). `Δavg` is kept because every baseline
   row ever recorded has an average, and the history table is built from
   averages. A baseline entry written before this change has no `min`, so it
   prints `Δmin n/a` until the next `CELLA_UPDATE_BENCH=1` refresh.

The JSON schema grew four optional fields. Old files still load, because the
new fields are `#[serde(default)]` (a unit test parses an old-style entry):

```json
"2d_straddle_92_t4": { "avg": 47.35, "std_dev": 0.66,
                       "min": 46.83, "median": 47.05, "outliers": 2, "load1": 1.64 }
```

Example output. With `CELLA_BENCH=1` every run is printed, then one stats line
per bench:

```text
[bench] load average (1 min) at suite start: 3.26
[bench]              1d_n3_custom_t1: 1.061873 ms      (example value)
  ... (one line per timed run) ...
[bench]              1d_n3_custom_t1: min 1.056043 ms, median 1.090268 ms, mean 1.088652 ms (+/-0.027830), outliers 0, load 3.26
```

and the end-of-suite summary (`zzz_benchmark_summary`, which runs last under
`--test-threads=1`) prints, per bench, the table row, then a load report and an
outlier report listing every bench that had outliers with the flagged values:

```text
[bench]   2d_three_state_cycle_t1: avg 41.9564 (± 3.4385) min 40.3869 med 40.6554 ms | Δmin +0.00% Δavg -1.65% | out 2
[bench] Load report (1-min load average; this box has 16 logical CPUs):
[bench]   at suite start: 1.41
[bench]   no bench finished with load > 4.
[bench] Outlier report (runs outside median ± max(3·1.4826·MAD, 2 % of median)):
[bench]   45 bench(es) had outliers (they are INCLUDED in avg/std, never dropped):
[bench]       1d_large_rule30_2049_t4: 3/10 runs [21.410, 18.581, 19.153] ms  <-- more than 20 % outliers: ...
```

Full-suite refresh (run from `cella_lib/`, on a quiet machine, release mode):

```text
CELLA_UPDATE_BENCH=1 cargo test --release --test long_suite -- --ignored --test-threads=1 --nocapture
```

(`make test-update-benchmarks` also works but runs every ignored test in the
package and, with no `--release`, in the debug profile. Use the command above
for baselines.) Check `cat /proc/loadavg` first: if the first number is above
4, wait. Another process (a test run from a different project) held the load at ~3.2
during one refresh, and the small benches came out 10–30 % off their usual numbers
(one, `1d_n3_custom_t1`, nearly 2× slow). That refresh was thrown away and
redone at load 1.4.

### Comparing two builds: `make bench-ab`

Never compare two *separate* suite runs by eye. The machine drifts over minutes
(CPU clock, other processes, the WSL2 host), and drift looks exactly like a
speed change. `scripts/bench_ab.py` (Python 3 standard library only) removes it
by **interleaving**: it runs build A, then B, then A, then B, and so on, so both
builds sample the same stretches of machine weather and slow drift cancels out.

```text
make bench-ab A=<git ref> FILTER='<libtest name filter>' [ROUNDS=8] [RUNS=10]
# e.g. what did my working-tree edit do to the 2D cycle benches?
make bench-ab A=HEAD FILTER='stress_2d_three_state_cycle stress_2d_life_like'
```

What it does:

1. Builds the `long_suite` test binary for git ref **A** (in a temporary
   `git worktree`, removed afterwards) and for **B**, your current working tree
   including uncommitted edits, using `cargo test --release --no-run
   --message-format=json` to find each binary. Both builds finish *before* any
   timing starts, then it waits (up to 10 minutes) for the load to fall to 4 or
   less, since its own build raised it.
2. Runs A, B, A, B, ... for `ROUNDS` rounds with `CELLA_BENCH=1` and your name
   filter, and parses the per-run times out of the output. Benches that exist
   on only one side are listed as skipped.
3. Per bench, prints min and median for each side, `Δmin`, `Δmedian`, the number
   of outlier runs on each side, and a **Mann-Whitney U** p-value. By default
   (`--unit rounds`) the p-value is computed on **one median per round per
   side**, not on every run.
   - *Why not every run?* Runs inside one process are **correlated**: they
     happen back to back, so they share the same CPU clock, cache state and
     background load. Ten runs from one round are closer to "one measurement
     repeated" than ten independent ones. Pooling them makes the test think it
     has far more evidence than it really does, so p comes out much too small.
     One median per round is the honest unit: rounds are separated in time, so
     they are much closer to independent. `--unit runs` still exists, but its
     p-values are labelled "optimistic p-values (runs within a process are
     correlated)".
   - *Mann-Whitney U in plain words:* pool every sample of A and B, sort them, and
     give each a rank (1 = fastest). If B is really slower, B's runs collect
     the high ranks. U measures how lopsided that is. It looks at ordering
     only, not the actual values, so one wild run cannot fake a result. The
     **p-value** is the chance of ranks at least this lopsided *if A and B were
     really the same*; small p means "unlikely to be luck". It is computed by
     hand with the normal approximation (tie-corrected, with continuity
     correction), so no scipy is needed.
4. Verdict: **"significant" only if the four-part gate below passes.** The
   p-value alone is not enough: even a 1 % wobble can reach p < 0.01 when there
   are many samples, and a 1 % change is not worth acting on.

**Why the default is 8 rounds.** In rounds mode each side has only `ROUNDS`
samples. With 5 per side, the smallest p the test can ever give is about 0.012
(even if every B round beats every A round), so the `p < 0.01` rule could never
fire. With 6 per side the best p is about 0.005, and 8 leaves some margin. The
script prints a warning if you ask for fewer than 6 rounds in rounds mode.

**Warm-up asymmetry warning.** The script checks whether A's
`cella_lib/tests/long_suite.rs` contains `CELLA_BENCH_WARMUP`. If A is older than
the warm-up change, A's first run of each bench in every process is cold (slow
caches, CPU ramp-up) and B's is not, which biases the result toward "B faster".
The script prints a warning and repeats it in the report header. As a
mitigation it drops the first timed run of every bench in every process, on both
sides, so the comparison stays symmetric. If you reuse `--workdir` and a leftover
`wt-a` worktree is there, the script removes it first and prints a note.

Useful flags: `--unit runs` (pool every run: optimistic p-values, see above),
`--workdir DIR` (keeps A's build cache
between invocations), `--flock PATH` (serialize timed runs with other users of
the machine), `--json FILE` (keep the raw samples).

**Verdict gate.** A bench is called "significant" only if **all four** hold:
(1) p < 0.01; (2) the median changed by more than 2 %; (3) the min changed by
more than 2 %; (4) the min and the median moved the **same way** (both slower or
both faster). Why the min and sign rules: noise from a busy machine mostly makes
some runs slow, which drags the median up but barely moves the fastest run. A
real change in the code shows up in the min too. Each non-significant row lists
the gates it failed, e.g. `no change (failed: p,min)`.

**Validation (2026-10-01).** Every check below compares the working tree with
`HEAD` with no engine change, so the right answer is always "no significant
change". Read them in order, because the gate was changed after the second step.

1. *Pooled-runs unit (the old default), gate = p and 2 % on the median.* Subset:
   1D rule 30 center, 1D n3 custom, 2D three-state cycle, 2D life-like, 2D von
   Neumann, each at `t1/t4/t8` (15 benches), 8 rounds x 10 runs, load 3.2-3.8
   (another project was using about one CPU). Run twice, 30 comparisons in
   total: **0 of 30 significant**, but **2 of 30 would have been false positives
   on p alone** (`2d_vonneumann_threshold_t4`, p near 0.0000, Δmedian -1.38 %;
   `2d_life_like_moore_t8`, p = 0.0037, Δmedian +0.94 %). Reason: runs in one
   round are correlated, so pooled p-values are too small. That is why the
   default became one median per round.
2. *Rounds unit, 8 rounds, same gate (p and 2 % median), filter `1d_n`, 6
   benches, load 3.75 before and 3.96 at the start of timing.* **1 of 6
   significant, a false positive:** `1d_n3_custom_t8`, p = 0.0074, Δmedian
   +3.21 % but Δmin only +0.89 %. The load was right at the limit of 4 and the
   bench takes only about 1 ms, so a few slow runs inflated the median. This is
   why the Δmin and same-sign gates were added. **Honest caveat:** the new gate
   was designed after seeing this result, so this run cannot validate it.
3. *Fresh nulls for the new gate (prediction made before running: 0
   significant).* Two filters not used before, `A=HEAD`, 8 rounds, rounds unit:
   `three_state` (6 benches, load 2.64 before, 3.21 at start, 2.94 at end) and
   `randomness` (6 benches, load 2.94 before, 2.84 at start, 3.55 at end). Result:
   **0 of 12 significant**. The smallest p was 0.052 (`1d_randomness_512_t4`),
   so even the p rule alone flagged none. Largest |Δmin| was 1.43 %; the one
   |Δmedian| above 2 % (`1d_three_state_cycle_t1`, -5.25 %) failed p and min.

Total: 12 fresh comparisons with the final gate, 0 false positives. That is a
small sample, so it shows the gate behaves on quiet-ish runs, not that the
false-positive rate is zero. Other caveats: about 8-10 % of runs fall outside
the outlier fence in these runs (they are reported and kept, never dropped), and
`HEAD` predates the warm-up change, so the script drops each process's first
timed run on both sides (see the warm-up warning above); once this change is
committed both sides have the warm-up.

### Scenarios added with this change

Every scenario has a golden snapshot, and the three thread counts hash the
same. Numbers below are the **min** of 10 runs in ms, from the quiet-machine
refresh (load 1.4). `chunks = clamp(total_work / 400 000, 1, threads)` rounds
*down*, where `total_work = cells × work_per_cell` and `work_per_cell` is the sum
of neighbour offsets over all subrules (`Rule2DPlan::work_per_cell`; for 1D it is
the sum of `2n + 1` per subrule).

| Scenario | What it isolates | t1 | t4 | t8 |
|---|---|---|---|---|
| `1d_rule30_65536` | wide 1D rule 30; work = 65 536 × 6 = 393 216, **just under** 400 000 | 63.3 | 63.1 | 64.2 |
| `1d_rule30_262144` | same rule, work = 1 572 864 → 3 chunks at t4/t8 | 64.2 | 50.4 | 49.2 |
| `2d_cycle128_hist{0,1,7}` | `history_limit` sweep, 3-state cycle 128², scalar path, pinned to t1 | 35.1 / 41.6 / 42.1 | - | - |
| `2d_cyclic12_128` | 12-type cyclic rule (24 subrules, work 192/cell), 12-entry type counter | 29.0 | 19.4 | 30.5 |
| `2d_straddle_65` | 3-state cycle, work 202 800 (~0.5×) → 1 chunk | 47.4 | 47.4 | 47.8 |
| `2d_straddle_92` | work 406 272 (~1×) → still 1 chunk (floors) | 46.6 | 46.8 | 46.7 |
| `2d_straddle_130` | work 811 200 (~2×) → 2 chunks | 48.1 | 49.5 | 47.4 |
| `2d_straddle_183` | work 1 607 472 (~4×) → 4 chunks | 47.1 | 54.4 | 54.3 |

(Straddle sizes keep total cell-steps about constant, so the `t1` column is flat
by design and any difference in `t4`/`t8` is the cost or benefit of splitting.)

What they show:

- **`1d_rule30_65536` does not reach the parallel path.** The packed bit-parallel
  1D path runs only when `chunks_for_work(...) <= 1`. At width 65 536 the work
  estimate is 393 216, under the 400 000 threshold, so it is *packed at t1, t4
  and t8 alike*, which is why the three columns agree to within noise. To
  actually exercise the scalar parallel path a width of at least ~133 000 is
  needed, hence the extra `1d_rule30_262144`: it takes the packed path at `t1`
  (one chunk) and the scalar parallel path with 3 chunks at `t4` and `t8`, and
  that parallel scalar path is ~22 % *faster* than packed-serial at the same
  cell-steps. Gap 5.4 is closed by this pair.
- **`history_limit`:** 0 → 1 costs about **+19 %** (35.1 → 41.6 ms), 1 → 7
  about **+1 %**. So the history *ring buffer* machinery (any non-zero limit) is
  what costs, not the depth. The §3.8 divide removal made depth almost free; the
  fixed cost of having a history at all is the remaining target.
- **The 400 000 threshold on this box.** For this rule, splitting does *not*
  pay at 2× and loses at 4×: 2 chunks are neutral (+2.9 % at t4, −1.5 % at t8,
  within noise), 4 chunks are **~15 % slower** than serial. The 12-type case
  agrees (t8 with 7 chunks is slower than t1; t4 with 4 chunks is 33 % faster,
  because its per-chunk work is ~786 000, about double). Reading: for cheap
  per-cell rules, a chunk needs well over 400 000 *estimated* visits to
  amortize the hand-off, or the estimate over-counts (the "stay" subrules
  usually stop early, which is exactly §3.14). This is a lead from one rule on
  one machine, not yet a tuning decision; confirm with `make bench-ab` and
  `CELLA_MIN_WORK` before changing the constant.

### Baseline history (`tests/benchmarks_last.json`)

Reconstructed from every commit that touched the file: the sum of per-bench
averages per commit (the file's older schema was a flat `name -> integer ms`
map; `6e8779f` switched it to `{avg, std_dev}`). Totals are only comparable
between rows with the same entry count — rows marked *(set)* changed the
benchmark set itself.

| Commit | Date | Entries | Suite total (ms) | Reason |
|---|---|---|---|---|
| *(this change)* | 2026-10-01 | 76 | 2 292.46 | DS-004 harness hardening: 30 new entries *(set)* (6 randomness entries that had snapshots but no baseline, plus `1d_rule30_65536`/`262144`, the `history_limit` sweep, `2d_cyclic12_128` and the four `2d_straddle_*` sizes); baseline entries now also carry `min`/`median`/`outliers`/`load1`. Engine unchanged: the 46 pre-existing entries sum to **944.65** (+4.2 % vs 906.93 on means, but their sum of mins is 901.54, −0.6 %: the mean-based difference is mostly outlier noise, which is what the new protocol exists to expose) |
| *(this change)* | 2026-08-14 | 46 | 906.93 | §8 round 4: wildfire fire-front mask (E8, −49/−53 %) |
| *(this change)* | 2026-08-14 | 46 | 1 134.49 | §8 round 3: 2D bit-plane fast path (E7, life-like −58/−59 %); comparable-40 total **580.67** |
| *(this change)* | 2026-08-14 | 46 | 1 358.11 | §8 round 2: packed-u64 1D Wolfram path (E6, rule30 −34/−43 %); comparable-40 total **764.34** |
| *(this change)* | 2026-08-14 | 46 | 1 365.45 | §8 experiments E1/E2/E3c (1D code downcast, 2D plan flatten + condition ranges, Gt-0 scan skip); comparable-40 total **785.46** |
| *(this change)* | 2026-08-14 | 46 | 1 335.37 | Thin-LTO/codegen-units profile + six wildfire entries *(set)*; the 40 pre-existing entries sum to **800.01** (−2.5 % vs 820.78) |
| `b67acbf` | 2026-07-27 | 40 | 820.78 | Interior/edge fast path + persistent rayon pool + work-sized chunking (§3.1/§3.2) |
| `5dbda2c` | 2026-06-24 | 40 | 1 536.82 | Revert grid1d fixed-array experiment back to `Vec` |
| `a258d07` | 2026-06-24 | 40 | 1 850.86 | grid1d fixed-array experiment (flawed) |
| `9cee1ea` | 2026-06-24 | 40 | 2 079.09 | `TypeCounter` + integer-math rearrangement |
| `65165e6` | 2026-06-19 | 40 | 4 534.00 | Proper double buffering (both grids) |
| `b030227` | 2026-06-07 | 40 | 4 766.54 | Baseline refresh after offsets-on-init |
| `4d3828f` | 2026-06-07 | 40 | 5 477.09 | `Rule2DSubrule` offsets computed at construction |
| `b15c7d7` | 2026-06-06 | 40 | 5 396.70 | Typo fix (no perf change) |
| `781b716` | 2026-06-04 | 40 | 5 396.70 | **`CellType` `String` → interned `Spur` (~31× total)** |
| `7847860` | 2026-05-16 | 40 | 167 692.80 | Knight stress benches added *(set)*; pre-Spur string cells |
| `55a4878` | 2026-03-29 | 37 | 59 087.00 | Code review / cleanup *(set)* |
| `6e8779f` | 2026-01-29 | 36 | 176 471.80 | Thread pools replace per-step spawn; schema → `{avg, std_dev}` |
| `911a487` | 2025-09-29 | 36 | 282 986 | StraightLine neighborhood + benches *(set)* |
| `9a05a19` | 2025-09-22 | 33 | 293 453 | Benchmark update *(set)* |
| `20606d3` | 2025-08-31 | 27 | 220 516 | 2D subrules → count + op refactor |
| `d2db892` | 2025-08-31 | 27 | 255 775 | Rules removed from snapshot hash |
| `dbae5db` | 2025-08-31 | 27 | 194 888 | Benchmarks + snapshots update |
| `25ac3e6` | 2025-08-31 | 27 | 182 085 | Bench update gated behind env var |
| `535aed5` | 2025-08-31 | 27 | 195 774 | First committed baseline |

Headline arc: **~196 s → 5.4 s** (Spur interning) **→ 0.82 s** (SoA +
interior/edge + work-sized rayon) **→ 0.80 s** (thin LTO), on the 40-entry
comparable set. Keep the table alive: every `CELLA_UPDATE_BENCH=1` refresh adds
a row with the commit hash and a one-line reason. The six wildfire entries
land at 123 ms serial / 71 ms `t4` for the 256×256 200-step scenarios —
the first benchmarks whose stochastic output is snapshot-pinned (see §7).

---

## 5. Test Coverage

Current coverage:

| File | Covers |
|---|---|
| `tests/config_tests.rs` | JSON config round-trip, grid building, dimension/length errors |
| `tests/edge_cases.rs` | rule validation edges, tiny grids, history bounds, CountOp zero-neighbor cases |
| `tests/randomness.rs` | randomness 0.0 (always) and 1.0 (never) only |
| `tests/soa_robust.rs` | circular-history FIFO order, parallel history consistency, SoA serde round-trips, out-of-bounds accessors, mixed-n subrules, **parallel-vs-serial counts, counts invariant, painting bounds** |
| `tests/external_model.rs` | external-model seam (out-of-tree impl), wildfire engine paths, thread-equivalence, model round-trips, config back-compat corpus |
| `tests/wildfire_stats.rs` | `#[ignore]`d wildfire ensembles: burned-fraction band, downwind centroid bias |
| `tests/long_suite.rs` | snapshot hashing, stress runs, benchmarks, `_t1/_t4/_t8` variants (incl. the wildfire scenarios) |

*(A `tests/rule2d_countop.rs` row used to sit here; that file was deleted in
`9950a57` and its CountOp coverage now lives in `rules.rs` unit tests.)*
| `lib.rs` inline tests | validation, Knight symmetry/stress, serde, multistate rotation, **dominant-type re-election** |

### Gaps closed *(done)*

1. **Parallel-vs-serial count consistency** — `parallel_counts_match_serial_1d`
   and `_2d` step a `set_thread_override(4)`/`(8)` grid and a `t1` reference and
   assert `counts_current` and `peak_counts` match. Widths are deliberately not
   multiples of the thread count (10241, 65537; 63×63, 65×65, 257×129) and
   `history_limit` sweeps 0 / 3 / 7 so `chunk * hl` alignment varies. This is what
   bug 2.1 needed. It only works because
   `set_min_work_per_chunk_override` forces the split on small grids.
2. **Counts invariant** — `assert_counts_total_{1d,2d}` asserts
   `counts_current.values().sum() == total cells` after *every* step in those
   tests, catching any future counting regression for free.
3. **2D dominant-type switching** — `dominant_type_is_re_elected_when_majority_flips`
   floods an `Inactive` grid with `Alive` and asserts the dominant type follows the
   majority. A crate-internal test, since counts stay correct either way.
6. **`transition_state_and_buffer` boundary** — asserts `idx == len` and
   `idx > len` are both rejected and `idx == len - 1` succeeds.
7. **Chunk-boundary/odd-size cases** — folded into gap 1 above.

### Gaps still open

4. **1D parallel path in benchmarks.** *(closed by DS-004)* No 1D benchmark used
   to reach the work threshold, so the 1D `_t4/_t8` variants measured the serial
   path. Now `1d_rule30_65536` exists, but note that at width 65 536 the work
   estimate (393 216) is still just *under* the 400 000 threshold, so it is
   packed at every thread count. `1d_rule30_262144` is the one that really takes
   the scalar parallel path (3 chunks at t4/t8), and
   `2d_straddle_{65,92,130,183}` bracket the threshold in 2D at about 0.5×, 1×,
   2× and 4×. The numbers are in §4 ("Scenarios added with this change"). Open
   follow-up: at 4 chunks the 2D split is about 15 % *slower* than serial on
   this box, which feeds §3.14.
5. **Mid-range randomness.** *(partly closed)* Since `2f04706` there are unit
   tests at randomness 0.5 (for example in `grid2d.rs`). They check that the same
   seed gives the same answer and that both outcomes happen over 64 seeds. Still
   missing: a frequency test, meaning "over a large N, about half the cells
   apply, within a tolerance band". The randomness benchmarks this gap used to
   ask for now exist (`1d_randomness_512`, `2d_randomness_128`).
8. **long_suite / bench expansions.** *(mostly closed by DS-004)* Done: the
   `history_limit` sweep (`2d_cycle128_hist{0,1,7}`; going from 0 to any
   non-zero limit costs about 19 %, depth beyond 1 about 1 %), the counts-heavy
   scenario with 12 types (`2d_cyclic12_128`), and the two randomness
   benchmarks now have baseline entries. **Dropped on purpose:** the
   "`SmallRng` vs `cell_rand`" RNG micro-benchmark. `SmallRng` was removed in
   `2f04706`, so there is nothing left to compare against, and the existing
   `1d_randomness_512` / `2d_randomness_128` benchmarks already exercise
   `cell_rand` end to end. Still open: Knight at 256×256, and snapshotting final
   `counts_current` alongside the FNV cell hash so statistics regressions are
   caught by golden files too. Note that today's snapshots hash only cell types
   and ages, which is exactly why bug 2.1 went unnoticed. The 12-type result
   does not by itself settle the §3.4 decision (replacing SipHash maps); it
   gives a baseline to measure that change against.

---

## 6. Future Direction: Hashlife

Hashlife (Gosper, 1984) is the algorithm behind tools like Golly that simulate
Life patterns trillions of generations ahead. It is included here as a possible
long-term direction, not a near-term recommendation. (When this was written,
§3.7 was the cheaper next step. It has since shipped for the two-state rule
shapes; §8 E9 showed that bit-packing does *not* pay off for every workload.)

### How it works

1. **Quadtree representation.** The grid is a 2^k × 2^k quadtree: a level-*k*
   node is four level-(k−1) children; level-0 nodes are single cells.
2. **Hash-consing.** Nodes are canonicalized in a global hash table: two subtrees
   with identical contents are the *same* node object. Empty space, and any
   repeated structure, costs O(1) memory regardless of extent. (This is the same
   interning idea `cella` already applies to type names with `lasso2` — applied to
   spatial structure instead of strings.)
3. **Memoized `RESULT`.** For each level-*k* node, the algorithm computes the
   level-(k−1) *centered* node that is its state some generations in the future,
   built recursively from nine overlapping child results. Because nodes are
   hash-consed, this `RESULT` is computed **once per distinct subtree ever seen**
   and cached forever.
4. **Superspeed.** In its full form, a level-*k* node's `RESULT` jumps 2^(k−2)
   generations at once — the time step grows exponentially with the spatial scale
   of the node. Regular or periodic patterns collapse into a few cached nodes,
   which is where the "trillions of generations" headline performance comes from.

The trade-offs: memory grows with pattern *entropy* (chaotic soups memoize
poorly), results arrive in exponential time jumps rather than step-by-step, and
the hash table needs garbage collection on long runs.

### Fit with cella

Works naturally:

- Deterministic, two-state, outer-totalistic rules on `Moore` neighborhoods — the
  Life-like configs — are exactly Hashlife's home turf.
- Multi-state deterministic rules are possible too (Golly's "Super" algorithms do
  this); the leaf alphabet just grows, and with it the memo table.

Fundamental conflicts with current features:

- **`randomness` subrules** break it completely — memoization requires that
  identical subtrees always evolve identically.
- **Per-cell `ages` and history** are not derivable from cell types alone, so a
  node's future is no longer a function of its contents; either those features are
  folded into the cell state (exploding the alphabet) or unavailable in Hashlife
  mode.
- **Interactive painting and per-step observation** (the GUI's bread and butter)
  fight the exponential time jumps and force tree edits that invalidate little,
  but make the algorithm run in its slow regime.

### If it were added

The realistic shape is an alternate engine behind a common trait
(`trait Engine { fn step(&mut self); fn cell_type(&self, idx) -> CellType; ... }`),
selected when a rule set is detected to be deterministic and history/age-free —
the same detection a bit-packed SIMD fast path (§3.7) needs. A SIMD stepper is
the better first investment: it accelerates the interactive per-step use case
cella is actually built around, whereas Hashlife shines precisely when you
*don't* want to watch every step.

---

## 7. Wildfire-Driven Performance Hooks

The external-model seam (`external.rs`) and the wildfire model (`wildfire/mod.rs`)
were designed around three performance decisions worth recording here:

**Stateless counter-based RNG.** `wildfire::cell_rand(seed, step, idx, stream)`
is a SplitMix64-style hash — no RNG state object at all. Besides being cheap
(a handful of integer multiplies, no loop-carried dependency, so the ignition
loop stays vectorizable in principle), it makes stochastic output **invariant
to chunk split and thread count**, which is what lets the FNV snapshot tests
pin the two wildfire benchmark scenarios: `stress_2d_wildfire{,_spotting}`
assert the *same* snapshot at `_t1`, `_t4`, and `_t8`. The subrule engine's
`randomness` feature now uses the same hash too: commit `2f04706` (2026-09-04)
moved it off the per-chunk `SmallRng::from_entropy()`. That made seeded
stochastic rule runs reproducible and the same at every thread count. The hash
now lives in `rng.rs`, and `wildfire::cell_rand` re-exports it.

**Events through the existing map/reduce.** Long-range writes (fire spotting)
never touch other chunks during the parallel pass: each chunk returns
`(TypeCounter, Vec<ModelEvent>)`, the vectors are appended during the rayon
`reduce`, and the engine applies events serially after the join — sorted, and
gated by `event_applies` for idempotence, so the result is independent of merge
order. No locks, no crossbeam, no new dependency; the empty-events case costs
one `Vec::new()` per chunk (kept allocation-free — `Vec::new` does not
allocate until first push).

**Per-step trig hoisted out of the cell loop.** The model precomputes slope
factors per cell **once at attach** (elevation is static) and the eight wind
factors **once per chunk** (uniform wind), so the per-cell ignition math is two
multiplies per burning neighbor plus one hash draw — no `exp`/`cos`/`atan`
anywhere in the hot loop. The engine runs its bookkeeping (history/ages/counts)
as a second cache-warm pass over the chunk rather than fusing it into the
model's loop; if profiling ever shows the extra sweep mattering, fusing it via
a callback is the option, at the cost of the engine/model isolation.

Benchmarks: `2d_wildfire_256` and `2d_wildfire_spotting_256` (200 steps,
256×256, mixed fuels, elevation ramp, 8 m/s wind) landed at ~123 ms serial and
~71 ms at `t4`/`t8` — the external-model path scales on the same
work-sized chunking as the subrule engine (`work_per_cell()` = 20 for the
wildfire model: eight neighbor reads plus RNG, float math, and the bookkeeping
pass).

---

## 8. Experiment Log (2026-08-14 optimization run)

Protocol: `CELLA_BENCH=1 CELLA_BENCH_RUNS=10 cargo test --release --test
long_suite -- --ignored --test-threads=1 --nocapture <filter>`, comparing
**min-of-runs** (far more stable than the mean on this WSL2 box); FNV
snapshots byte-identical after every experiment — including the two wildfire
snapshots, which pin the stochastic path across thread counts. Numbers below
are t1 mins in ms unless noted. Kept and rejected experiments both recorded.

### E1 — 1D `Rule1DPlan` code downcast ✅ KEPT (resolves most of §3.9)

Per-step plan (`rules.rs::Rule1DPlan`) downcasts `wolfram_code` to `u64` for
`n <= 2` (window index ≤ 31) and hoists the `1 <= n <= 3` validity check out
of the cell loop; only `n = 3` still pays the 128-bit variable shift.

| Bench | before | after | Δ |
|---|---|---|---|
| `1d_three_state_cycle` | 7.42 | 6.42 | **−13.4 %** |
| `1d_n2_alt` | 1.11 | 0.96 | **−13 %** |
| `1d_rule30_center` | 1.42 | 1.31 | −8 % |
| `1d_large_rule30_2049` | 25.87 | 25.55 | −1.2 % |
| `1d_n3_custom` | 0.88 | 0.88 | flat (still u128, as expected) |

The §3.9 regression was +13–23 % on n=1 cases; this recovers most of it. The
u128 variable shift was indeed the untested suspect. `1d_large_rule30_2049`
moving least suggests its remaining cost is elsewhere (likely memory-bound;
the §3.7 packed-u64 path remains the real fix there).

### E2 — 2D plan flatten + `(lo, hi)` ranges + lazy `TypeCounter` ✅ KEPT

Three §3.12 items landed together: `Rule2DPlan.lin` flattened to
`lin_flat + spans` (2 allocations/step instead of subrules+1, slice indexing
instead of `&Vec` chase in the inner loop); `eval_condition`'s six-arm
`(op, limit)` match folded into precomputed inclusive `cond_lo..=cond_hi`
fields on `Rule2DSubrule` (serde-skipped, like `early_exit`); `TypeCounter::new`
allocation-free until first `add` (rayon reduce identity is now free).

| Bench | before | after | Δ |
|---|---|---|---|
| `2d_three_state_cycle` | 45.67 | 40.00 | **−12.4 %** |
| `2d_vonneumann_threshold` | 3.29 | 2.92 | −11 % |
| `2d_langton_diagonals` | 2.04 | 1.83 | −10 % |
| `2d_straightline_threshold` | 2.15 | 1.93 | −10 % |
| `2d_knight_neighborhood` | 3.57 | 3.24 | −9 % |
| `2d_large_moore_256` | 150.36 | 138.15 | **−8.1 %** |
| `2d_large_vn_256` | 68.26 | 63.24 | −7.4 % |
| `stress_config_100x100` (t4) | 3.00 | 2.79 | −7 % |
| `2d_life_like_moore` | 5.25 | 4.89 | −7 % |

Broad win across every threshold bench. This closes §3.5 (no SmallVec
dependency needed) and the plan/condition/counter parts of §3.12.

### E3a/E3b — hoisted early-exit loop restructure ❌ REJECTED

Splitting the neighbor loop into separate early-exit and branchless-accumulate
variants (`if early { counting loop with break } else { pure += (t==crit) }`)
helped `2d_three_state_cycle` (−5–8 %) but **regressed `2d_large_moore_256`
by 4–6 %** (143.5–146.4 vs 138.2) in both the accumulate (E3a) and branchy
(E3b) variants — a code-layout effect, not the accumulate itself. Reverted.

### E3c — `Gt 0` scan skip ✅ KEPT

Keeping the inner loop byte-identical and only adding a pre-check — a subrule
with `early_exit && count == 0` is satisfied with zero neighbors, so the scan
is skipped entirely:

| Bench | E2 | E3c | Δ |
|---|---|---|---|
| `2d_three_state_cycle` | 40.00 | 35.76 | **−10.6 %** |
| `2d_large_vn_256` | 63.24 | 60.57 | −4.2 % |
| `2d_life_like_moore` | 4.89 | 4.94 | +1 % (noise) |
| `2d_large_moore_256` | 138.15 | ~141.5 | **+2.4 %** (isolated 15-run re-check) |

The `2d_large_moore_256` cost is persistent but small and bought a 4.2 ms win
on `three_state` plus smaller wins elsewhere; net suite-positive, kept. The
three-state-cycle family (`Gt 0` transition rules) no longer scans neighbors
at all.

### Not attempted, with reasons

- **Wildfire branchless neighbor loop** (`p_no *= 1 − is_burning · p` always):
  most fuel cells have zero burning neighbors and take a well-predicted branch;
  branchless would charge 8 f32 multiplies to every fuel cell to help only the
  fire front. Expected net loss.
- **Run-based interior counting (§3.13)**: every committed bench uses range
  1–2, where per-`dy` runs are 1–3 cells long — slice-per-run overhead would
  dominate. Becomes interesting only with larger-radius workloads; left open.
- **Hashlife**: excluded by request (§6 unchanged).

### Round 2

### E5 — history-branch monomorphization ❌ REJECTED (badly)

Splitting `Grid2D::step_chunk` into `<const HIST: bool>` variants so the
per-cell `history_limit > 0` test disappears regressed **every** 2D bench by
**+30–46 %** (`large_moore` 141→185, `large_vn` 60.6→88.5, `three_state`
35.8→48.5, `life_like` 4.9→6.4). The doubled loop body blows the inliner
budget for `next_type_interior`; the branch it removed was perfectly
predicted anyway. Reverted; a warning comment now sits on `step_chunk`.
Together with the doc's earlier 1D tri-split ablation and §8 E3a/b, that is
three independent data points that this hot loop is **code-size-bound** —
any experiment that grows it needs to expect a regression. The 2D row-based
loop restructure was skipped on the same evidence.

### Probe — where does `1d_large_rule30_2049` spend its time?

Throwaway `#[ignore]` test, min-of-8, t1: `hist = 4` → 23.2 ms, `hist = 0` →
21.1 ms. History is only ~9 % — the window fold + dispatch dominates, which
green-lit E6 (a packed fast path would have been pointless if bookkeeping
dominated).

### E6 — packed-`u64` 1D Wolfram fast path ✅ KEPT (§3.7 stage 1)

`Rule1DPlan` now detects the **pure two-state Wolfram shape** (exactly two
`n = 1` subrules — `{current: active}` and `{current: inactive}` — same
criteria/code/output, no randomness; both committed rule30 scenarios match
it). Eligible serial steps run `Grid1D::step_packed`: the row packed one bit
per cell, neighbors as whole-word shifts with cross-word carries, and the
8-entry transition table evaluated for 64 cells at once by OR-ing the AND of
(possibly complemented) `l`/`m`/`r` words per set code bit. The engine
bookkeeping (next types, ages, history, counts) stays a per-cell sweep.

Exactness guarantees: a per-step scan falls back to the scalar path if any
cell is neither `active` nor inactive (so painting a foreign type mid-run
cannot diverge), parallel-width grids fall back (word carries don't cross
chunk boundaries), and a property test drives packed vs scalar grids
(detection defeated by a semantically-inert third subrule) across codes
{0, 30, 110, 129, 255} × widths {1, 63, 64, 65, 130, 2049} × `hl` {0, 2},
asserting cells, ages, history, and counts equal every step. The FNV
snapshots for both rule30 scenarios pass unchanged.

| Bench | before | after | Δ |
|---|---|---|---|
| `1d_large_rule30_2049` | 25.55 | 14.66 | **−43 %** |
| `1d_rule30_center` | 1.31 | 0.86 | **−34 %** |
| `1d_three_state_cycle` (3 types, ineligible) | 6.42 | 6.34 | unchanged ✓ |
| `1d_n2_alt` / `1d_n3_custom` (ineligible) | — | — | unchanged ✓ |

Remaining cost in the eligible benches is the per-cell bookkeeping sweep and
the bits↔cells conversions each step. Follow-on if ever needed: keep the bit
row alive across steps (invalidate on paint) and vectorize the age update —
diminishing returns until bookkeeping itself is the bottleneck.

### Round 3

### E7 — 2D bit-plane fast path ✅ KEPT (§3.7 stage 2)

`Rule2DPlan` now detects the **two-state threshold shape** (every subrule
counts the same `active` type over one shared radius-1 neighborhood, no
randomness, currents/outputs all `active`/inactive — life-like rules). For
such rules the next state is a pure function of `(current, neighbor count)`,
captured as an 18-entry table built by running the subrule chain once per
combination at plan time. Eligible steps run `Grid2D::step_packed`: one bit
per cell with rows padded to whole words, the eight neighbor planes formed by
word shifts with cross-word carries (row ends and borders shift in zeros),
counts accumulated into four bit-planes with carry-save adder steps, and the
table applied via equality masks — 64 cells per word. Applied at every thread
count (measured faster than the 8-thread scalar path). Same exactness recipe
as E6: per-step content scan with scalar fallback, and packed-vs-scalar
property tests (detection defeated by a semantically-inert ghost subrule)
across Moore/VonNeumann/Langton × six sizes straddling word boundaries × `hl`
{0, 2}, asserting cells/ages/history/counts equal every step.

| Bench | before | after | Δ |
|---|---|---|---|
| `2d_large_moore_256_t1` | 141.5 | 58.2 | **−59 %** |
| `2d_large_moore_256_t8` (was parallel scalar) | 81.1 | 59.7 | **−26 %** |
| `2d_life_like_moore` | 4.94 | 2.06 | **−58 %** |
| `2d_three_state_cycle` (3 types, ineligible) | 35.8 | 34.6 | unchanged ✓ |
| `2d_large_vn_256` (range 2, ineligible) | — | — | unchanged ✓ |
| wildfire scenarios (model path) | — | — | unchanged ✓ |

Remaining cost in eligible benches is the per-cell bookkeeping sweep
(history depth 4 on `large_moore`) and the bits↔cells conversions — same
follow-on as E6 (keep bit rows alive across steps) if ever needed.

### Round 4

### E8 — wildfire fire-front mask ✅ KEPT

Could the packed approach apply to the wildfire model? Not directly: its
ignition probability is not a function of neighbor *count* — it multiplies
per-direction wind factors and per-cell slope factors over the *specific*
burning neighbors, across multiple fuel classes, stochastically. No lookup
table can represent that. But the bitmap machinery works as a **sparsity
filter**: a wildfire cell can only change if it is Burning (ages/burns out)
or touches a Burning cell (may ignite). `WildfireModel::step_chunk` now
copies the chunk through as the default, builds a Burning bitmap for its rows
(plus a one-row halo), ORs the eight one-cell shifts into a "fire front"
mask, and runs the scalar per-cell math **only for set bits**. Fire fronts
are thin lines (~perimeter, not area), so the 8-read + RNG work drops from
every fuel cell to a few hundred cells per step.

Exactness by construction: skipped cells could not change and never consumed
randomness (the ignition draw only fires when a burning neighbor exists), so
per-cell results and RNG streams are bit-identical — confirmed by the
unchanged FNV snapshots at every thread count, the thread-equivalence tests,
and the ensemble statistics.

| Bench | before | after | Δ |
|---|---|---|---|
| `2d_wildfire_256_t1` | 125.9 | 59.4 | **−53 %** |
| `2d_wildfire_spotting_256_t1` | 116.7 | 59.6 | **−49 %** |
| `2d_wildfire_256_t4` | 71.2 | 45.4 | −36 % |

Remaining cost is the engine bookkeeping sweep, the chunk copy, and the
bitmap build — all linear passes with no per-cell branching on rule logic.

### Round 5

### E9 — generalized bitwise subrule chain ❌ REJECTED

An attempt to extend the 2D bit path beyond the two-type/table shape: one
bit-plane per cell type (up to 5 types), per-subrule neighborhoods to radius
2 (two-bit word shifts), counts to 24 via five bit-planes, ranges as 5-bit
mask comparators, and the first-match subrule chain replayed with
`undecided`/output masks — targeting `2d_large_vn_256` (mixed-radius,
three-type) and the other still-scalar benches. Implementation was correct
(property tests across mixed radii, Knight/Eq/between ranges, and the
three-state cycle all passed, snapshots byte-identical) but **slower than
the scalar path everywhere it fired**: `large_vn` +17 %, `knight` +27 %,
`langton` +36 %, `three_state` a wash. Two reasons, obvious in hindsight:

1. **E3c already removed the neighbor scan** from `Gt 0` rules, so the
   three-state family is bookkeeping-bound — a bit kernel adds plane-build
   and sweep overhead without removing any per-cell work that still exists.
2. On the remaining candidates the scalar path's per-cell work is a dozen
   early-exiting reads, while the chain pays plane construction, per-word
   count trees, and a per-type readback sweep regardless — the conversion
   overhead never amortizes at these rule sizes.

Fully reverted. Lesson recorded: the bit paths win only where the scalar
path still does *heavy uniform* per-cell rule work (life-like tables, the
1D window fold) — not where earlier optimizations already made the scalar
path cheap. Measurement note: this round was benched under background CPU
contention (load average up to 17 from unrelated builds); the *relative*
chain-vs-scalar losses were consistent enough to reject, but absolute
numbers from the contended window were discarded.

### Round 6

### E10 — GUI: type-compare painter, time-budgeted Run-to, per-frame stats ✅ KEPT

The first GUI-side measurement in this log (everything above is
`cella_lib`, the simulation engine; this one is the `cella` binary's egui
frontend, §2 of `docs/roadmap.md`). Three changes went in on reasoned
suspicion, then got measured after the fact:

- **§2.1** — `RowPainter::emit_row` used to resolve every cell's `Color32`
  and compare colours to decide whether a paint run continues; it now
  compares `CellType` (a single `u32` under the hood) and only resolves a
  colour when the type actually changes, while still merging adjacent runs
  that happen to share a colour.
- **§2.2** — "Run to +N" used to run a fixed step count per frame; it is
  now budgeted by wall-clock time (`RUN_TO_FRAME_BUDGET`, 8 ms) via a
  clock-free `run_to_batch` that advances in `RUN_TO_CHUNK`-sized (32-step)
  chunks, Pause cancels a run in progress, and `start_run_to` restores
  whatever play state was active before the button was pressed.
- **§2.3** — the per-step work is now split so a burst can skip the parts
  it doesn't need: `advance_grid` is the step itself (1D history push,
  `Grid1D::step` / `Grid2D::step`, `timed_steps`) with no clock read and no
  statistics sample; and `step_once` (the Step button, paced play) is that
  step with the play-timer refresh before it and `stats_record_step` after
  it, one sample per step.
  `run_to_batch` loops `advance_grid` directly, and `tick_play`'s burst
  branch calls `stats_record_step` once after its whole chunk loop
  finishes — so a burst now samples statistics once per frame instead of
  once per simulation step, with the play-timer refresh also hoisted to
  once per chunk instead of once per step.

**Painter (§2.1), measured with `paint_bench`:** a synthetic 2000-cell row
(3 foreground types plus background, laid out as one long run, a few
single-cell islands, and background gaps) painted 900 times per "frame" —
a stand-in for a 1600×900 window at `scale = 1`, not a number pulled from a
live GUI frame, since the GUI cannot be launched in this environment (no
GL: `winit EventLoopError` under WSLg/Mesa). Min-of-20-frames, three runs
each side:

| Bench | before | after | Δ |
|---|---|---|---|
| `paint_bench` (synthetic row, 900 rows/frame) | ~3.1–3.2 ms/frame | ~1.18–1.19 ms/frame | **~−62%** |

The baseline was well above the 1 ms mark the plan used as the cleanup/win
threshold, so §2.1 is filed as a real win, not a cleanup: comparing
`CellType` instead of resolving and comparing `Color32` cut paint time by
roughly 2.6x. The number held steady across Phase 2 — §2.2 and §2.3 don't
touch `emit_row`, and re-running `paint_bench` after all of Phase 2 landed
gave ~1.18–1.19 ms/frame, at or just below the after-§2.1 range of
~1.19–1.22 ms/frame measured right after that change landed — no further
change expected or seen, since §2.2 and §2.3 don't touch the paint path.

**Run-to (§2.2):** the number that matters is wall-clock time for "Run to
+100000" from the toolbar on `configs/2d_large_moore_256.json`, taken by
hand with a stopwatch. Before: predicted ~1000 steps/s, from the old
`RUN_TO_STEPS_PER_FRAME` constant (100 steps per frame, now removed) at the
`refresh_ms` default of 100, i.e. ~100 s / ~1.7 min for the full run — a
prediction, never measured by hand, since the GUI could not be launched
before or after the change in this environment. After: time-budgeted, to
be measured by hand on a GL machine.

Statistics sampling (§2.3) has no standalone stopwatch number of its own —
it changes how many chart samples a burst appends, not how fast the burst
runs — so it is covered by the Run-to number above and by the `cella`
binary's existing unit tests on `step_once` / `run_to_batch` / `tick_play`
(see `src/gui/sim.rs`), which assert sample counts directly.

### Net effect (four kept rounds)

Comparable-40-entry suite total (sum of avgs):
**800.01 → 785.46 → 764.34 → 580.67 ms** (−27 %), and the six wildfire
entries dropped from ~123/71 ms to ~57/46 ms in round 4 (46-entry total
1 134 → 907 ms). Per-bench mins above are the honest per-case numbers.
Baselines refreshed after each round; snapshots byte-identical throughout,
including both wildfire scenarios.

---

## Summary of Priorities

Done:

1. ✅ `TypeCounter::merge` (2.1), 2D dominant-type tracking (2.2), shared
   `apply_counts` (2.3), bounds guards (2.4), `CountOp` docs (2.5).
2. ✅ Parallel count-consistency and counts-invariant tests (5.1, 5.2), plus
   dominant-type and painting-bounds tests (5.3, 5.6) — each verified to fail
   without its fix.
3. ✅ Branchless interior stepping via precomputed linear offsets (3.1).
4. ✅ Persistent pool, work-proportional split, `hl == 0` parallel support (3.2).
5. ✅ RNG hoist (3.3), `thread_count()` atomic (3.6), history-modulo removal and
   `OutChunk` reborrow (3.8).

Also done since the original review:

6. ✅ `pool()` registry lock → `OnceLock` slot array (3.10).
7. ✅ Release profile: thin LTO + `codegen-units = 1` in both manifests (3.11),
   −2.5 % on the comparable benchmark set; baseline refreshed with the six
   wildfire entries (§4 history table).
8. ✅ Wildfire/external-model perf groundwork: stateless counter RNG, events
   through the reduce, per-chunk trig hoisting (§7); wildfire benchmarks are
   snapshot-pinned across thread counts.

Next, in order:

9. ✅ Trustworthy A/B timing (§4). Criterion was tried and rejected on this
   machine (DS-002). The existing harness was hardened instead (DS-004): it
   adds a warm-up, min/median, reported outliers, a load guard, and
   `make bench-ab` for comparing two builds by alternating their runs.
10. ✅ Bench gaps 4 and 8 mostly closed by DS-004, including a 1D case large
    enough to take the parallel path and the threshold-straddling sizes §3.14
    needs. Still open: gap 5's frequency test, Knight at 256×256, and counts
    in snapshots.
11. ✅ §3.9 via the `Rule1DPlan` downcast — done, §8 E1 (−8 to −13 % on n≤2
    1D cases); `1d_large_rule30_2049` was then fixed by §3.7 stage 1 (§8 E6).
12. ✅ §3.12 plan flatten, condition ranges, lazy counter — done, §8 E2
    (−7 to −12 % across threshold benches; subsumes §3.5). The output-write
    bounds item stays open. §8 E3c added the `Gt 0` scan skip on top.
13. §3.7 **both stages shipped** (§8 E6/E7): packed-u64 1D Wolfram path
    (−43 % rule30) and the 2D bit-plane threshold path (−58/−59 % life-like),
    each a separate stepper selected per rule shape with per-step scalar
    fallback — exactly the structure the E5/E3 code-size evidence demanded.
    Remaining headroom in eligible workloads is the bookkeeping sweep and
    per-step bits↔cells conversion (keep bit rows alive across steps if it
    ever matters). §3.13's contiguous-run idea stays parked for large-radius
    scalar workloads. (`std::simd` is still nightly-only; SWAR and
    autovectorization are the stable routes.)
14. ✅ Subrule `randomness` draw moved to the stateless hash RNG (§7). Done in
    `2f04706` (2026-09-04), with randomness benches and snapshots.
15. ❌ Spin-then-park worker pool (§3.2): re-investigated 2026-10-01 and
    closed. No safe-API crate is fast enough, and the `unsafe` version
    collapses on a busy machine. The conditions for reopening are in §3.2.
16. Hashlife (§6) as a long-term project.

---

## 9. Ensemble stepping parallelism (2026-09-12)

**The question.** `Ensemble::step` (`cella_lib/src/explore/ensemble.rs`) steps
every member of a 32-member wildfire ensemble once. There are two ways to
spend 16 threads on that: step several members *at once*, each on its own
thread (or a slice of threads), or step members *one at a time*, each one
using all 16 threads for its own grid update. Today's code picks between
these with a size heuristic. Is the heuristic right for the two grids the
validation suite actually uses?

**What "today" turned out to mean — a surprise before any timing ran.**
Reading the heuristic (`chunks_for_work(total * 12) <= 1`, where `total` is
one member's cell count) against `Bear_2020` (748×619 ≈ 463 000 cells) and
`Ferguson_2018` (1155×1316 ≈ 1 520 000 cells) at 16 threads shows it already
picks **one member at a time** for both — not "every member at once" the
way a quick read of the code suggests. Both grids are big enough that a
single member's own step would already split into more than one chunk, so
the heuristic defers to that instead of running members concurrently. This
matters a lot for what "the default" even means below: the honest baseline
for these two scenarios is *already* close to "one at a time", and the
study had to measure that baseline directly (labelled `unset` in the table)
rather than assume it equalled the (16, 400k) config, the way the original
study design did.

**The two knobs** (`cella_lib/src/threads.rs`, `cella_lib/src/explore/ensemble.rs`),
process-local, read once from the environment, no-ops unless set:

- `CELLA_MEMBER_PAR=<n>` — step at most `n` members concurrently, in
  batches, instead of letting the heuristic decide.
- `CELLA_MIN_WORK=<cells>` — a bootstrap over the pre-existing
  `set_min_work_per_chunk_override`: how many cells of estimated work a
  chunk needs before it's worth handing to another thread (the constant is
  400 000 today).

**In plain words, what these knobs trade off.** Every time a step hands
work to another thread, someone has to wake a parked worker, and that
worker has to report back when it's done — this is called a *fork-join*: one
thread forks the work out, then joins (waits for) the results. Waking a
thread costs real time (tens of microseconds), so it only pays for itself
if the thread then does enough work to be worth the wake-up. `CELLA_MIN_WORK`
controls that per-chunk threshold; `CELLA_MEMBER_PAR` controls something
similar one level up — how many members share *one* fork-join round instead
of each paying for their own. Stepping 16 members in one batch pays for one
wake-up-and-rejoin per step; stepping them one at a time pays for it 16
times. There's a second cost too: a running member's grid (its cells, its
history buffers) has to be pulled into the CPU's cache to be worked on —
its *working set*. With several members' grids being touched by different
threads at once, more total data is "hot" at any moment, which can push
older data back out to slower memory (a cache miss) more often —
*oversubscription* is the general name for asking for more concurrent work
than the hardware can actually run at once, whether that's more threads
than cores or more hot working sets than cache. The measurements below
show which of these two costs actually dominates at these grid sizes.

**Method.** `wildfire_smc <scenario> 32 open <out.json>`, truncated to the
first `SMC_MAX_DAYS=5` observation days (a new knob added alongside this
study — `SMC_MAP_DAYS` was the existing equivalent for `map` mode only), on
`Bear_2020` and `Ferguson_2018`, under the default Bernoulli rule and the
arrival rule (`SMC_SPREAD=arrival SMC_WIND_LAW=rear_focus`), `SMC_SEED=0`.
Thread count fixed at 16 via a `cella.properties` placed in each run's
working directory (never the repo's own — see
`validation/scripts/experiments/bench_ensemble_par.py`). Every configuration
run twice on an otherwise-idle 16-core/62 GB machine; the table reports the
minimum of the two (both reps are in
`validation/results/experiments/bench_ensemble_par.json`). Binary: HEAD
`ee4ae69`, confirmed clean (`binary_git` in every report JSON, no `-dirty`).

**Timing (seconds, min of repeats — 2 reps except the three †  cells, 3):**

| (member_par, min_work) | Bear bernoulli | Bear arrival | Ferguson bernoulli | Ferguson arrival |
|---|---|---|---|---|
| `unset` (today's heuristic) | 11.32 | 42.52 | 25.83 | 100.11 |
| (16, 400k) | **4.86** | 23.69 | 17.06 | **76.25** |
| (16, 50k) | 5.11 | **23.32** | 15.42 † | 78.41 |
| (8, 100k) | 5.90 | 24.41 | 16.39 | 79.57 |
| (4, 50k) | 6.66 | 26.94 | 17.11 | 83.70 |
| (2, 25k) | 8.14 | 33.33 | 18.98 | 93.60 † |
| (1, 12.5k) | 11.18 | 46.90 | 22.13 | 109.74 † |

Bold marks the fastest official config per column (the two are within a few
percent of each other in every column — `min_work` barely matters once
member-parallelism is engaged); `unset` is a bonus row, not part of the
pre-registered grid, added because of the finding above.

† **Noisy cells (fix round 1, 2026-09-12 controller review).** These three
cells' first two repeats disagreed by more than 10%: Ferguson/bernoulli/
(16,50k) (originally 15.62/17.28, 10.6%), Ferguson/arrival/(2,25k) (93.60/
103.79, 10.9%), Ferguson/arrival/(1,12.5k) (109.74/124.03, 13.0%) — this
last pair alone spans a wider range than several *other* configs' entire
min-to-min gap in the same column, so a 2-rep min was not a safe number for
comparisons this close. A third repeat was run for exactly these three
(15.42s, 95.88s, 117.19s); the min barely moved in any of the three
(15.62→15.42, 93.60 unchanged, 109.74 unchanged — the original min already
happened to be the low outlier), but the *spread* stayed wide (12.0%, 10.9%,
13.0% across 3 points), so read these four numbers as "fast, ±10-13%," not
as precise as the rest of the table. This machine's run-to-run noise on the
larger Ferguson grid is real, not a 2-rep artifact — see the Ferguson/
bernoulli/(16,50k) row of `validation/results/experiments/bench_ensemble_par.json`
for the raw numbers.

**Determinism.** Every report JSON, minus `binary_git`/`binary_built_utc`
(the `open`-mode report has no `wall_time_secs` field to strip), is
byte-identical across all seven configurations, in all four
fire × rule combinations. Confirmed both by the unit test
(`stepping_is_identical_across_member_par_and_min_work`,
`cella_lib/src/explore/ensemble.rs`) and by the benchmark script itself.
The knobs change *how* work is scheduled, never the answer.

**Why: fork-join overhead, not cache pressure, is what's driving this.**
`perf stat -e cache-misses,context-switches` on Bear/arrival, comparing
(16, 400k) against (1, 12.5k) (`context-switches` reads 0 for both — this
machine's `perf_event_paranoid=2` blocks the kernel-level counter without
root, a real gap in the evidence, not a real zero):

| Config | Wall (this perf run) | User CPU | Sys CPU | Cache misses |
|---|---|---|---|---|
| (16, 400k) | 25.14 s | 339.7 s | 6.55 s | 634 822 542 |
| (1, 12.5k) | 42.22 s | 324.4 s | 34.20 s | 974 306 027 |

Stepping one member at a time, finely chunked, costs **5.2× the system
time** (more, smaller fork-join rounds means more time in the kernel
parking and waking threads) and **54 % more cache misses** — the opposite
of what "smaller chunks fit the cache better" would predict. Batching
members amortizes the wake-up cost over more useful work per round, and
apparently does *not* trade that away for a worse cache-working-set. The
oversubscription story (many members' grids hot at once, more misses) is
not what's happening here; the fork-join-per-member-per-step story is.

**Runner-level: does this change how many concurrent processes to run?**
Total wall time for 4 `Bear_2020`/Bernoulli runs (seeds 0-3), four ways,
min of 2 repeats each (fix round 1 added the fourth arrangement and the
second repeat — both repeats' raw numbers are in
`validation/results/experiments/bench_ensemble_par.json`):

| Arrangement | Config | Total wall (min of 2) |
|---|---|---|
| 4 processes × 16 threads (oversubscribed; `run_all`'s actual shape — see caveat below) | `unset` | 16.43 s |
| **4 processes × 16 threads, `CELLA_MEMBER_PAR=16` forced in each** | `CELLA_MEMBER_PAR=16` | **14.56 s (+11.4 %)** |
| 1 process at a time × 16 threads | best single-run config, (16, 50k) | 19.75 s |
| 2 processes × 8 threads, 2 rounds | (16, 50k) | 15.28 s |

`run_all`'s own default is `workers=3`; every R6 experiment script that
actually runs 4 concurrent `wildfire_smc` processes passes `workers=4`
explicitly (`exp_r6_gated_reset.py`, `exp_r6_arrival_fires.py`,
`exp_r6_observed_immigrants.py`, `exp_r6_all_state_correction.py`) — "today's
shape" above means those call sites, not the function's own default.

Counter to the pre-registered prediction, running one process at a time —
even with the much-faster-per-run (16, 50k) config — is **20.2 % *slower***
for the batch of four than today's 4-way oversubscribed shape, and 2×8 is
a 7.0 % improvement over plain 4×16 (bigger than run-to-run noise, but
still against the *unset* 4×16 baseline, not the one that matters for the
default question below). The per-run speedup does not carry over to a
naive one-at-a-time arrangement: four processes sharing 16 cores still get
more total throughput than one process running four times in a row,
because concurrency lets the *batch* overlap work that a faster but
strictly-sequential arrangement cannot.

**The arm that actually decides the default question** is the second row:
today's process shape (4 concurrent, 16 threads) *plus* the single-run win
(`CELLA_MEMBER_PAR=16` forced in each of the 4). That beats plain 4×16 by
**11.4 %** — real (all four arms agree bit-for-bit per seed once
provenance is stripped, confirmed directly, not just inferred from the
single-run determinism test) but **below the pre-registered 20 % bar** for
changing `r5_common.BASE_ENV`. Per the controller's ruling: less than 20 %
→ document the number, change nothing. **`r5_common.BASE_ENV` is
unchanged.** (Had this arm cleared 20 %, the ruling was to add
`CELLA_MEMBER_PAR=16` to `BASE_ENV` — a runner-level config change only,
never `Ensemble::step`'s own heuristic or `MIN_WORK_PER_CHUNK`.)

**Prediction, checked line by line** (pre-registered before timing):

1. *"(16, 50k) is ≥10 % faster than today on Ferguson."* Against the
   design doc's own labelled baseline, (16, 400k): +9.6 % on Bernoulli
   (short of 10, and this exact cell is one of the three noisy ones above
   — a 12 %-wide spread across 3 repeats, so "+9.6 %" should be read as
   "roughly tied with 10 %, not reliably above or below it"), **−2.8 %**
   (slower) on arrival — fails, or is a wash. Against the *actual* default
   (`unset`): +40.3 % on Bernoulli, +21.7 % on arrival — holds clearly,
   comfortably outside the noise band either way. The prediction's truth
   depends entirely on which baseline "today" means, which is itself the
   study's headline finding.
2. *"(2, 25k)/(1, 12.5k) slower than (16, 50k) on Bernoulli, within 20 % on
   arrival."* Slower on Bernoulli: confirmed, all four cases (+21.5 % to
   +118.8 %). Within 20 % on arrival: mostly fails — three of four cases
   exceed 20 % (Bear +42.9 %/+101.1 %, Ferguson (1, 12.5k) +40.0 % —
   unchanged by the third repeat, whose 117.19 s was *higher* than the
   109.74 s already used as the min); only Ferguson (2, 25k) at +19.4 %
   narrowly complies, and that comparison is one of the three flagged
   cells above (12 %-wide spread across repeats), so "narrowly complies"
   should not be read as a confident pass.
3. *"1×16 beats 4×16 oversubscribed by ≥15 %."* Contradicted: 1×16 is
   20.2 % **slower**, not faster (see runner-level table above; this
   comparison uses the low-noise Bear scenario only, so the noise caveat
   above does not apply here).

**Recommendation, and an explicit deviation from the plan's 20 % rule.**
The pre-registered bar — "change a default only if ≥20 % faster AND
bit-identical" — **was met on the single-run axis** (24-57 % faster,
proven bit-identical) but is **deliberately not applied** to
`Ensemble::step`'s own heuristic (`chunks_for_work(total * 12) <= 1`) or to
`MIN_WORK_PER_CHUNK`. This is a considered deviation, not an oversight, for
three reasons: the heuristic is shared by every model this library can
run, not just wildfire at these two grid sizes; the evidence is two
scenarios under one rule family, not the general case the existing
40+-entry benchmark suite (§4) already tunes for, and retuning it without
rerunning that whole suite risks a regression nobody would notice until it
shipped; and the runner-level result above shows the obvious follow-on move
("since single runs are faster, run fewer of them") is actually a net loss
for a concurrent batch. `CELLA_MIN_WORK` barely moves the needle once
member-parallelism is already engaged (**−5.1 % to +9.6 %** beyond
(16, 400k) — Bear/Bernoulli is the one case where 50k is *slower*; the
range's upper end is one of the three noisy cells above), so its constant
stays put too.

**The one place the bar *was* re-applied — `r5_common.BASE_ENV`** — did not
clear it either: the 4×16+`CELLA_MEMBER_PAR=16` arm above beat plain 4×16
by 11.4 %, bit-identical, below the 20 % bar, so `BASE_ENV` is unchanged
(see the runner-level section above for the full reasoning).

**What *is* worth adopting, by hand, per workload:** a single wildfire
ensemble run at these grid sizes is genuinely 24-57 % faster
(bit-identical) with `CELLA_MEMBER_PAR=<thread_count()>` set explicitly —
but a *sweep* of several such runs launched concurrently should stay
concurrent at `workers=4`; neither switching to one-process-at-a-time nor
adding the knob to `BASE_ENV` clears the bar this study set for changing
that default. Full table, both runner-level repeats, and the noisy-cell
data are in `validation/results/experiments/bench_ensemble_par.json`
(gitignored, reproducible from `bench_ensemble_par.py`).

## 10. Bench profile study (2026-09-25)

**The question.** All of Round 7's experiment binaries
(`validation/experiments/round-7.md`) were built with the plain
`[profile.release]` every other release build in this repo uses (`lto =
"thin"`, `codegen-units = 1`) — so every number in that round is
comparable to every other, but none of them says whether a more
aggressive profile would make the *next* round's batches faster. This is
a **read-only comparison**: no baseline file changes, `tests/
benchmarks_last.json` is never written (`CELLA_UPDATE_BENCH` is never
set), and no runner's `BIN` changes.

**What was added.** `[profile.bench]` in both `Cargo.toml` (root) and
`cella_lib/Cargo.toml` — the same "two build roots, keep them in sync"
rule `[profile.release]` already follows in both files:

```toml
[profile.bench]
inherits = "release"
lto = "fat"
panic = "abort"
```

`target-cpu=native` is **not** baked into the profile — it is applied at
build time via the `RUSTFLAGS` environment variable
(`RUSTFLAGS="-C target-cpu=native" cargo build --profile bench ...`), not
`.cargo/config.toml`, so it never silently affects a plain `cargo build`
or `cargo test` and has to be asked for explicitly every time.

**Renamed after this study ran (during review).** The profile above
is what this study actually measured, under Cargo's *built-in* `bench`
name — which, per the first "build-system detail" below, defaults to the
*same* output directory as `release` (`target/release/`, no separate
`target/bench/`). That means a `cargo build --profile bench --example
wildfire_smc` could fingerprint-match and silently reuse or overwrite the
plain-release `cella_lib/target/release/examples/wildfire_smc` every
experiment runner calls — the exact build-root trap this campaign's own
tooling notes warn about elsewhere, just reachable through a profile name
instead of a stray `cargo build` in the wrong directory. The profile has
since been renamed to `[profile.bench-study]` (identical settings, in
both `Cargo.toml` files) so it gets its own `target/bench-study/`
directory and can never collide with `target/release/`. This is a
naming/safety fix, not a re-run: every number below was measured under
the old built-in `bench` name, and none of them changed. Anyone repeating
this study should substitute `--profile bench-study` for `--profile
bench` in the commands below; whether Cargo's built-in-profile quirks in
the second "build-system detail" below (the ignored `panic = "abort"`)
still apply under a custom name was not re-checked, since re-checking
would mean re-running the study, which this fix does not do.

**Method.** Five variants, each built and run once, plus two repeats of
the plain release baseline to see how much the box's own noise moves the
number on its own. Every build and run: `nice -n 10`, one at a time
(never overlapping this task's own coverage run, which held the box
first), load logged at launch. Benchmarks: the `#[ignore]`d suite in
`cella_lib/tests/long_suite.rs`, run from `cella_lib/` —

```
cargo test --release --test long_suite -- --ignored --test-threads=1 --nocapture   # release, and variant (i)
cargo test --profile bench --test long_suite -- --ignored --test-threads=1 --nocapture   # variants (ii), (iii)
RUSTFLAGS="-C target-cpu=native" cargo test --profile bench --test long_suite -- --ignored --test-threads=1 --nocapture   # (iv), (v)
```

Two numbers per run: the outer wall time (`time`, includes process
start-up and, for a variant's first run, nothing else — each variant was
built with `cargo build --profile bench --tests` *before* being timed, so
the timed run is pure test execution, not compilation) and the suite's
own internal `zzz_benchmark_summary` "TOTAL (sum of averages)" line — the
same aggregate `docs/performance.md`'s own headline figures elsewhere in
this file use, printed by the harness itself without `CELLA_UPDATE_BENCH`
set, so it reads and compares against the committed baseline but never
writes it.

**Two build-system details discovered while setting this up, neither a
mistake in the study:**

1. Cargo's built-in `bench` profile defaults to the *same output
   directory* as `release` (`target/release/`) unless a custom
   `dir-name` is set — it does not get its own `target/bench/`. So
   whenever a variant's *effective* settings happen to match plain
   release exactly, Cargo's fingerprint matches the already-built
   release artifacts and it reuses them outright, reported as `Fresh`
   for every crate (`cargo build --profile bench --test long_suite -v`),
   not a fresh compile. That is exactly variant (i) (`inherits =
   "release"`, nothing else) — it is not merely *expected* to match
   release, it is confirmed to run the identical compiled binary release
   already built. It is also, for a second reason (next point), variant
   (iii).
2. Cargo ignores `panic = "abort"` for the built-in `bench` profile when
   it is exercised through `cargo test`/`cargo bench` — printed plainly
   as `` warning: `panic` setting is ignored for `bench` profile `` —
   because the test harness needs to unwind to catch a panicking test
   and report it as a failure rather than aborting the whole run. With
   `panic` ignored, variant (iii)'s (`+ panic = "abort"`) *effective*
   settings are therefore also identical to release's, so it hits the
   same cache-reuse path as (i) — confirmed the same way, `Fresh`
   everywhere, 0.05 s. `panic = "abort"` only has a real effect outside
   a test/bench harness (a plain `--release`-style binary or example),
   which is not what this study's benchmarks run through.

Net effect: the "release" rows and the (i)/(iii) rows below are not
three independently-compiled binaries measured once each — they are
**the same compiled binary**, run at different points in the run.
That is a feature for this study, not a gap: it means the spread between
release/(i)/(iii) is a clean same-binary noise measurement (the box's
own run-to-run variance), which is exactly the yardstick every other
variant needs to be read against. Only (ii), (iv) and (v) changed the
effective profile enough to force a genuine recompile (confirmed by
their own multi-second-to-90-second compile time when built, not a
0.05 s cache hit — the exact per-variant build durations were logged
during the run but were not carried into a table in this file).

**Results — wall time and the suite's own internal total, both variants
compared against the release baseline's own two-repeat spread:**

| Run | Wall (s) | Internal TOTAL (ms) | Ratio to release mean (TOTAL) | Load at launch |
|---|---|---|---|---|
| Release, repeat 1 | 13.76 | 1210.27 | — | 5.33 |
| Release, repeat 2 | 12.14 | 1180.71 | — | 4.80 |
| **(i) `[profile.bench]` inherits release, nothing else** | 12.59 | 1223.75 | 1.024 (2.4 % slower) | 4.07 |
| **(ii) + `lto = "fat"`** | 13.32 | 1294.74 | 1.083 (8.3 % slower) | 3.69 |
| **(iii) + `panic = "abort"`** (ignored by Cargo for this profile — see above; same binary as (i)) | 12.68 | 1235.08 | 1.033 (3.3 % slower) | 4.82 |
| **(iv) + `RUSTFLAGS="-C target-cpu=native"`** | 13.42 | 1171.48 | 0.980 (2.0 % faster) | 7.67 |
| **(v) fat LTO + panic=abort + target-cpu=native, all combined** | 13.11 | 1141.17 | 0.955 (4.5 % faster) | 5.73–6.62 |

How to read it: "ratio to release mean" divides each variant's internal
TOTAL by the release baseline's own two-repeat mean (1195.49 ms) — the
internal figure is used for the ratio column rather than wall time
because it is the suite's own per-benchmark sum, less sensitive to this
shared box's process-launch jitter than an outer `time` call. The
release baseline's own two repeats already move by 2.5 % against each
other (1180.71 → 1210.27 ms) with nothing changed at all. That two-repeat
figure understates the real noise floor, though: (i) and (iii) are, per
the "build-system details" above, confirmed to be *the same compiled
binary* as release, not independent measurements of a different profile —
so the honest same-binary spread is all four numbers together (1180.71,
1210.27, 1223.75, 1235.08 ms), a 4.6 % range, not the 2.5 % the two release
repeats alone suggest. Read against the wider 4.6 % band, every variant
below sits inside or barely outside it except (ii), which in this one run
was clearly slower than release, not faster — this study built (ii) once,
so "robustly slower" would overstate what a single run can show; a repeat
build of (ii) would be needed to say more than "slower in the one run
measured here." (iv)'s run shares the box with a load spike to 7.67
(another process, not this study's own doing — see the global "never
compare wall time across batches" rule this campaign already follows);
its wall time is not trustworthy on its own, but its internal TOTAL
(measured inside one process, not affected by what else the box is doing
at launch) still lands at the fastest of the four real variants tested.
Three representative individual benchmarks, release repeat 1 vs each
variant (ms, avg of 10 runs each; the full 46-benchmark table is in each
run's own captured log under `/tmp/bench-*.log`, not committed):

| Benchmark | Release r1 | (i) | (ii) | (iii) | (iv) | (v) |
|---|---|---|---|---|---|---|
| `1d_large_rule30_2049_t1` | 17.31 | 16.98 | 18.33 | 17.67 | 18.19 | **15.95** |
| `2d_large_vn_256_t1` | 76.02 | 78.38 | **85.23** | 79.79 | 78.97 | 80.20 |
| `2d_wildfire_spotting_256_t1` | 69.88 | 69.13 | 70.37 | 66.39 | 70.49 | **60.94** |

No single variant wins on every benchmark; `2d_large_vn_256` is slower
under every variant tried, fat LTO (ii) worst of all, while the other two
shown here are fastest under (v).

**Recommendation.** Do not move the Round 7 experiment binaries (or any
runner's `BIN`) to the bench profile: the only variant that would be
worth the extra build time and the `panic = "abort"` caveat above — (v),
all combined, ≈ 4.5 % faster on the suite's own internal total — is a
real but modest gain that a second baseline repeat alone already moves
by more than half of (2.5 % of 1195 ms ≈ 30 ms noise vs. (v)'s own ≈ 54 ms
gain), and fat LTO on its own (ii) is measurably *slower*, not faster, so
"just turn on more optimization flags" is not a safe default for this
workload without re-measuring per change. `r5_common`/`r7_common`'s
runners keep building and calling `cella_lib/target/release/examples/
wildfire_smc`, unchanged.
