# Cella Performance Review

A review of the `cella_lib` simulation engine, originally written against the
SoA/double-buffer refactor (branch `qwen-test`, July 2026) and **updated after
acting on it**. It covers the current architecture and its optimizations, the
correctness issues found, what was implemented and measured, and what remains.

**Status: the §2 correctness fixes and the §3.1–§3.6, §3.10, and §3.11
performance items have been implemented and measured.** Sections marked *(done)*
describe shipped code; sections marked *(open)* are still recommendations. §7
records the performance decisions baked into the wildfire/external-model work.
Line references are omitted in favour of naming functions, since line numbers
drift.

Headline: the benchmark suite total went from **1329 ms to 840 ms (−37 %)** on the
reference machine, with cell-for-cell identical output (the FNV snapshot tests in
`tests/snapshots/` pass unchanged throughout).

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

The two 1D regressions are a known open item — see §3.9.

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

Still open: no explicit SIMD. See §3.7.

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

### 3.3 RNG acquisition hoisted out of the per-cell loop *(done)*

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

### 3.5 Small-buffer offsets *(open)*

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

### 3.7 Bit-packing and SIMD *(open — largest remaining win)*

For the dominant two-type workloads (Life-like rules), the 4-bytes-per-cell
layout still leaves a lot on the table:

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

### 3.9 Open regression: 1D `n = 1` rules *(open)*

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
validity check out of the cell loop. Sequenced after the criterion migration so
the result is attributable; §3.7's packed-`u64` path would subsume it entirely.

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

### 3.12 Per-step allocations and loop-invariant dispatch *(open)*

Fresh findings, unmeasurable under the old ±5–8 % noise floor; queued behind
the §4 criterion migration:

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

### 3.13 Interior bounds-check elision — the §3.7 gateway *(open)*

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

The harness is serviceable but its limits were the binding constraint on the last
20 % of this work:

- `Instant`-based timing with mean ± std over `CELLA_BENCH_RUNS` runs; no warmup
  discard, no outlier rejection, no significance test. On this machine the noise
  band is roughly **±5–8 %**, which is the same size as several of the effects
  worth chasing. Two A/B rounds during this work produced conclusions that
  reversed on re-measurement.
- Practical workaround used here: `CELLA_BENCH=1` with a test-name filter prints
  every run, and taking the **minimum** across runs is far more stable than the
  mean for comparing two builds. A filtered 1D-only run completes in ~25 s, which
  makes real A/B iteration possible.
- Benchmarks are `#[ignore]`d tests, so they build in the `test` profile unless
  the invocation overrides it. **Always pass `--release`.**
- `tests/benchmarks_last.json` was refreshed as part of this work, so the `Δ`
  column is once again meaningful. It had been stale enough that every delta read
  as a large improvement regardless of the change.

Recommendation, now stronger than in the original review: move the timing
benchmarks to `criterion` or `divan` benches (`cella_lib/benches/`), keeping the
FNV snapshot tests exactly as they are — the snapshot mechanism is genuinely good
regression armor, is orthogonal to timing, and was the thing that gave confidence
that all of §3 preserved behaviour exactly.

### Criterion migration plan

Criterion over divan: its `--save-baseline` / `--baseline` comparison with
outlier rejection and significance testing is precisely the cure for the
"two A/B rounds reversed on re-measurement" problem above. (A divan bench
target with `AllocProfiler` is a worthwhile follow-on for counting the §3.12
per-step allocations exactly; the `unsafe` involved lives inside divan, not
this crate.) Shape:

- `[dev-dependencies] criterion` + a `[[bench]] name = "engine"` target with
  `harness = false`; port the grid builders from `long_suite.rs` (timing only —
  snapshots stay where they are).
- New scenarios closing gaps 5.4/5.5/5.8 while we are there: a 1D case wide
  enough to reach the work threshold (`1d_rule30_65536`), a randomness-0.5
  subrule case (finally measures §3.3), a `history_limit` 0/1/7 sweep
  (measures the §3.8 divide removal), a 12-type counts-heavy case (tests the
  §3.4 decision), sizes straddling `MIN_WORK_PER_CHUNK` (feeds §3.14), and an
  RNG micro-bench (`SmallRng` sequential draws vs the stateless
  `wildfire::cell_rand` hash).
- A/B protocol: `cargo bench -p cella_lib -- --save-baseline main` on HEAD,
  apply the change, `cargo bench -p cella_lib -- --baseline main <filter>`;
  accept on a significant improvement in the target benches with no
  significant regression elsewhere, and gate every engine change on the FNV
  snapshots staying byte-identical. The legacy `CELLA_BENCH=1` min-of-runs
  protocol remains as a cross-check.

### Baseline history (`tests/benchmarks_last.json`)

Reconstructed from every commit that touched the file: the sum of per-bench
averages per commit (the file's older schema was a flat `name -> integer ms`
map; `6e8779f` switched it to `{avg, std_dev}`). Totals are only comparable
between rows with the same entry count — rows marked *(set)* changed the
benchmark set itself.

| Commit | Date | Entries | Suite total (ms) | Reason |
|---|---|---|---|---|
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

4. **1D parallel path in benchmarks.** No 1D benchmark reaches the work threshold,
   so the 1D `_t4/_t8` variants still measure the serial path (visible in the
   results: the three thread variants agree to within noise). Add a
   width ≥ 65536 1D case, and 2D cases straddling the work threshold, to make the
   heuristic's effect visible.
5. **Mid-range randomness.** Only the 0.0/1.0 extremes are tested. Add a
   statistical test (randomness = 0.5, large N, tolerance band) documenting that
   it is tolerance-based, not exact. This is also the missing regression test for
   §3.3, whose effect is currently unmeasurable for want of a benchmark that uses
   randomness at all.
8. **long_suite / bench expansions.** A randomness-rule benchmark (would quantify
   §3.3), Knight at 256×256, a `history_limit` sweep (0 / 1 / 7 — now that `hl`
   no longer gates threading, this measures the §3.8 divide removal), and a
   counts-heavy scenario with 10+ types (would confirm or overturn the §3.4
   decision). Consider snapshotting final `counts_current` alongside the FNV cell
   hash so statistics regressions are caught by golden files too — note that
   today's snapshots hash only cell types and ages, which is exactly why bug 2.1
   went unnoticed.

---

## 6. Future Direction: Hashlife

Hashlife (Gosper, 1984) is the algorithm behind tools like Golly that simulate
Life patterns trillions of generations ahead. It is included here as a possible
long-term direction — not a near-term recommendation; §3.7 is far cheaper and
benefits every workload.

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

The external-model seam (`external.rs`) and the wildfire model (`wildfire.rs`)
were designed around three performance decisions worth recording here:

**Stateless counter-based RNG.** `wildfire::cell_rand(seed, step, idx, stream)`
is a SplitMix64-style hash — no RNG state object at all. Besides being cheap
(a handful of integer multiplies, no loop-carried dependency, so the ignition
loop stays vectorizable in principle), it makes stochastic output **invariant
to chunk split and thread count**, which is what lets the FNV snapshot tests
pin the two wildfire benchmark scenarios: `stress_2d_wildfire{,_spotting}`
assert the *same* snapshot at `_t1`, `_t4`, and `_t8`. The subrule engine's
`randomness` feature still draws from a per-chunk `SmallRng::from_entropy()`
(grid2d/grid1d `step_chunk`) and is therefore neither reproducible nor
split-independent — migrating it to the hash draw is an open item, and a
behavior change that needs its own decision (existing stochastic runs would
change output; snapshots do not currently cover them, so the blast radius is
configs in the wild, not the test suite). The criterion RNG micro-bench (§4)
will quantify the cost side.

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

9. Move timing benchmarks to `criterion` (§4, migration plan there). This is
   now the blocker on further micro-optimization, not a nicety — the current
   ±5–8 % noise band is wider than the remaining effects, including the 1D
   regression in §3.9.
10. Close bench gaps 5.4, 5.5, 5.8 as part of the criterion port — in
    particular a randomness benchmark, without which §3.3 cannot be measured,
    a 1D case large enough to exercise the parallel path, and the
    threshold-straddling sizes §3.14 needs.
11. §3.9 via the `Rule1DPlan` downcast (addendum there) — the one item above
    today's noise floor, and the last untested suspect.
12. §3.12 per-step allocation and dispatch cleanups (subsumes §3.5, no
    SmallVec dependency).
13. §3.13 interior bounds-check elision via contiguous-run slices — the safe
    stepping stone to the bit-packed / SIMD fast path (3.7), which remains the
    largest projected win. (`std::simd` is still nightly-only; SWAR
    bit-packing and autovectorization are the stable routes.)
14. Migrate the subrule `randomness` draw to the stateless hash RNG (§7) —
    a deliberate behavior change to schedule, not sneak in.
15. Revisit a spin-then-park worker pool (§3.2) only after measuring fork/join
    latency on the target platform; it would need `unsafe`, and the payoff
    depends entirely on that number.
16. Hashlife (§6) as a long-term project.
