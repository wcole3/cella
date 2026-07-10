# Cella Performance Review

A review of the `cella_lib` simulation engine as of the SoA/double-buffer refactor
(branch `qwen-test`, July 2026). It covers the current architecture and its existing
optimizations, correctness issues found during the review, ranked performance
recommendations, test-coverage gaps, and a section on the Hashlife algorithm as a
possible future direction.

**No code was changed as part of this review** — everything below is analysis and
recommendation. File/line references point at the working-tree state at review time
and will drift as the code evolves.

---

## 1. Current Architecture and Existing Optimizations

The engine is a straightforward synchronous stepper: every cell is re-evaluated
every step against an ordered list of subrules. Several significant optimizations
are already in place.

### Struct-of-arrays (SoA) grid layout

Both `Grid1D` and `Grid2D` store cell data as parallel flat vectors rather than a
`Vec<CellState>` of per-cell structs (`grid1d.rs:36-58`, `grid2d.rs:44-66`):

| Field | Type | Purpose |
|---|---|---|
| `cells` | `Vec<CellType>` | current types, row-major in 2D |
| `next_cells` | `Vec<CellType>` | double buffer for the next step |
| `ages` | `Vec<u32>` | steps spent in current state |
| `history_data` | `Vec<CellType>` | flat circular history; cell *i* owns `[i*hl .. (i+1)*hl)` |
| `history_heads` / `history_counts` | `Vec<u8>` | write head / entry count per cell (`history_limit <= 255`) |

`CellState` (with its per-cell `VecDeque` heap allocation) survives only as a
serialization intermediate (`types.rs:83-92`, used by `state.rs` and the custom
`Deserialize` impls). The live stepping path never touches it.

### Interned cell types

`CellType` is a 4-byte `Copy` wrapper around a `lasso2::Spur` handle into a global
`ThreadedRodeo` interner (`types.rs:11-29`). Type comparison in the hot loop is a
single integer compare; string resolution (`as_str`) only happens at
serialization/display boundaries.

### Double buffering

`step()` writes the next generation into `next_cells` and then `mem::swap`s the
buffers (`grid1d.rs:368`, `grid2d.rs:349`). No per-step allocation of a new grid.

### Incremental population counting

Each `step_chunk` accumulates new-cell counts into a `TypeCounter` — a linear-scan
`Vec<(CellType, u64)>` that avoids HashMap hashing for the typically <20 distinct
types (`rules.rs:21-55`). The **dominant type** (usually the background/Inactive
type, i.e. the vast majority of cells) is *skipped* during counting
(`grid1d.rs:291-293`, `grid2d.rs:288-290`) and back-filled by subtraction from the
total in `recompute_counts_from_cells` (`grid1d.rs:298-319`, `grid2d.rs:194-215`).
On a sparse grid this removes the counter update for most cells.

### Precomputed neighborhood offsets

2D neighborhood shapes are computed once per `(neighborhood, range)` pair via
`#[memoize(SharedCache)] neighborhood_offsets` (`rules.rs:113-152`) and cached on
each `Rule2DSubrule` as a sorted `offsets: Vec<(i32,i32)>` (`rules.rs:341-342`),
rederived on deserialize (`rules.rs:427-447`). The Knight neighborhood's BFS
reachability (`knight_reachable`, `rules.rs:156-180`) only runs at rule
construction.

### Early exit in neighbor counting

For the common `Gt`-without-`limit` case, counting stops as soon as the threshold
is reached (`grid2d.rs:242-244`, mirrored in `applies_and_output` at
`rules.rs:401`). Note `Gt`/`Lt` semantics are inclusive (`>=` / `<=`,
`eval_condition` at `rules.rs:415-424`), so the early exit at `neighbors >= count`
is correct.

### Inlined rule evaluation

The grids inline neighbor lookup and counting in `next_type`
(`grid1d.rs:213-260`, `grid2d.rs:230-255`) instead of going through the
closure-based `applies_and_output` API — no closure dispatch per neighbor. 1D
builds a fixed stack-array window of 3/5/7 cells and folds it into a Wolfram-code
bit index (`applies`, `rules.rs:252-259`).

### Chunked parallelism

Above a size threshold, `step()` splits the output buffers with `chunks_mut` and
runs `step_chunk` per chunk under `std::thread::scope`
(`grid1d.rs:336-366`, `grid2d.rs:317-347`). Cell/age/history chunks are disjoint
slices, so no locking is needed; per-chunk `TypeCounter`s are merged serially
afterward. Thread count comes from `cella.properties` / `available_parallelism()`
(`threads.rs:55-63`) with a test override.

Single-thread fallback conditions:

- 1D: `threads <= 1 || width < 8192 || history_limit == 0` (`grid1d.rs:336`)
- 2D: `threads <= 1 || total < 4096 || history_limit == 0` (`grid2d.rs:317`)

### Benchmark harness

`cella_lib/tests/long_suite.rs` contains a hand-rolled harness (all `#[ignore]`,
run with `cargo test -p cella_lib -- --ignored --test-threads=1`):

- FNV-1a hashes of final grid state compared against golden files in
  `tests/snapshots/` — cheap, deterministic regression detection.
- `run_benchmark_1d/2d` time `steps` iterations with `Instant`, repeated
  `CELLA_BENCH_RUNS` times (default 10), summarized (mean ± std, delta vs the
  `tests/benchmarks_last.json` baseline) by `zzz_benchmark_summary`.
- Thread-count variants (`_t1`, `_t4`, `_t8`) via `set_thread_override`.

---

## 2. Correctness Issues Found

These were found while reading the hot path. They matter to this review because
the first two make the *parallel* path produce different statistics than the
serial path, which poisons any benchmark comparison that also validates counts.

### 2.1 `TypeCounter::merge` drops entries (parallel counts corrupted)

`rules.rs:40-50`:

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

The `return` is meant to terminate the *inner* search, but it exits `merge`
entirely: as soon as one incoming type is found in the accumulator, **all
remaining entries of `other` are silently dropped**. `add` (`rules.rs:30-38`)
has the same shape but is correct there because a single add is complete after
the early return.

Impact: whenever the multi-threaded path runs (2D `total >= 4096` / 1D
`width >= 8192`, with `history_limit > 0` and >1 thread) and chunk counters share
types — which is the normal case — `counts_current` and `peak_counts` are wrong.
Cell contents are unaffected (the merge only feeds statistics), which is why the
FNV snapshot tests never caught it.

Fix sketch: labeled loop —

```rust
'outer: for (t, c) in &other.entries {
    for e in &mut self.entries {
        if e.0 == *t { e.1 += *c; continue 'outer; }
    }
    self.entries.push((*t, *c));
}
```

### 2.2 2D dominant-type switching is dead code

`grid2d.rs:197-214`: `new_dominant_type` is declared as
`(&CellType, u64)` but the update at `grid2d.rs:204-206` only assigns `.0`:

```rust
if *v >= new_dominant_type.1 {
    new_dominant_type.0 = k
}
```

`.1` stays `0`, so the switch condition `total_count < new_dominant_type.1`
(`grid2d.rs:212`) can never be true and `dominant_type` never changes after
construction in 2D. The 1D version updates the whole tuple and is correct
(`grid1d.rs:308-310`).

Impact: correctness of counts is preserved (the dominant type is still counted by
subtraction), but the *optimization* degrades — if the initially dominant type
stops being the majority, every step counts the actual majority type cell-by-cell
in `TypeCounter`, exactly the work the skip was meant to avoid.

### 2.3 1D/2D `recompute_counts_from_cells` asymmetry

1D inserts with overwrite semantics (`self.counts_current.insert(k.0, *v)`,
`grid1d.rs:303`) while 2D uses `entry().or_insert(*v)` (`grid2d.rs:199`). Both
work today only because the map is `.clear()`ed first; the divergence invites a
future bug when one side changes. Pick one implementation and share it (the
bodies are ~identical and could be a free function over
`(&mut HashMap, &mut HashMap, &mut CellType, total, &TypeCounter)`).

### 2.4 Off-by-one bounds guards in `transition_state_and_buffer`

- `grid1d.rs:197`: `if idx > self.width` — should be `>=`. `idx == width` passes
  the guard and panics inside `transition_cell` on `self.cells[idx]`.
- `grid2d.rs:164`: same pattern with `idx > self.width * self.height`.

These are the interactive-painting entry points, so the panic is user-reachable
from the GUI.

### 2.5 `CountOp` doc comments contradict the implementation

The enum doc comments (`rules.rs:329-334`) describe `Lt`/`Gt` as strict
(`<`, `>`), but `eval_condition` (`rules.rs:415-424`) implements inclusive
`<=` / `>=`. The implementation is what the configs and tests rely on (e.g. the
Game of Life config uses `gt 2` to mean "2 or more"); the doc comments should be
corrected to say "at least" / "at most".

---

## 3. Performance Recommendations (ranked)

Ranked by estimated impact-per-effort on the hot stepping path. Each item is a
recommendation only; measure with the bench suite (§4) before and after.

### 3.1 Halo/ghost-cell padding — remove per-neighbor bounds checks

Every neighbor access branches on bounds:

- 2D `neighbor()` (`grid2d.rs:219-225`): 4 comparisons + `isize` casts per
  neighbor, per subrule, per cell. For Life-like rules that is up to 8 branchy
  lookups per cell per step, and for `range`-2+ neighborhoods far more.
- 1D `get_type_or_inactive` (`grid1d.rs:207-209`): 2 comparisons per window slot.

For interior cells — the overwhelming majority — the branch always takes the
in-bounds path, but it still costs a compare/branch and defeats
auto-vectorization.

Recommended change: allocate the grid with a 1-cell (or `range_max()`-cell)
border of `inactive` cells ("halo"/"ghost cells"). Interior stepping then indexes
`cells[idx + precomputed_linear_offset]` unconditionally with no bounds logic;
the halo is refreshed (or simply never written) between steps since the boundary
condition is a fixed `inactive` border. The precomputed `(dx, dy)` offsets become
precomputed *linear* offsets `dy * padded_width + dx`, removing the per-neighbor
`y * width + x` arithmetic as well. This is the classic CA optimization and the
single biggest expected win for 2D stepping.

Cheaper intermediate step (no layout change): split each row/chunk into
`[left edge] [interior] [right edge]`, running a branchless fast path over the
interior and the existing checked path only on the edges.

### 3.2 Fix the parallel path's economics: thread reuse and thresholds

Three separate issues compound here:

1. **Thread spawn per step.** `std::thread::scope` spawns fresh OS threads every
   `step()` (`grid1d.rs:342`, `grid2d.rs:323`). At tens of microseconds per
   spawn/join cycle, small-to-mid grids pay more in thread churn than they gain.
   Options: `rayon` (`par_chunks_mut` is nearly a drop-in for the current
   structure) or a small persistent worker pool owned by the grid.
2. **`history_limit == 0` disables parallelism entirely** (`grid1d.rs:336`,
   `grid2d.rs:317`) — presumably because `chunks_mut(chunk * 0)` is invalid. But
   `hl == 0` is the *cheapest* configuration and would benefit most from threads
   on large grids. Special-case the history slices (e.g. hand three empty slices
   to every chunk) instead of falling back to serial.
3. **Thresholds are untuned.** 2D parallelizes at 4096 cells (64×64), where
   chunk work is likely too small to beat spawn overhead; 1D requires
   width ≥ 8192, which no current benchmark reaches (§5, gap 4). Once threads are
   reused (1), re-tune both thresholds empirically with the bench suite.

### 3.3 Hoist RNG acquisition out of the per-cell loop

For subrules with `randomness`, `rand::thread_rng()` is fetched inside
`next_type` per firing cell (`grid1d.rs:254`, `grid2d.rs:248`; also
`rules.rs:280,405`). `thread_rng()` is a TLS lookup plus `Rc` refcount traffic on
every call. Create one `SmallRng` per `step_chunk` invocation and pass
`&mut impl Rng` down into `next_type`. Also makes seeding for reproducible
randomized runs possible later.

### 3.4 Replace SipHash maps for counts

`counts_current` / `peak_counts` are `HashMap<Spur, u64>` with the default
SipHash hasher (`grid1d.rs:52-54`, `grid2d.rs:60-62`), fully cleared and rebuilt
every step by `recompute_counts_from_cells`. `Spur` is a `NonZeroU32`; SipHash is
overkill for it. Options, in increasing order of change:

- Swap in `rustc-hash`/`ahash` (`FxHashMap<Spur, u64>`): one-line type change,
  removes most hashing cost.
- Keep the per-step data in `TypeCounter` form and only sync the public HashMaps
  on demand (accessor or dirty flag) instead of every step — the GUI reads them
  once per frame, not per step.
- Dense `Vec<u64>` indexed by `Spur::into_inner()` — fastest, viable because the
  interner only ever grows and type counts are tiny.

### 3.5 Small-buffer offsets

`sr.offsets` is a heap `Vec<(i32,i32)>` chased per subrule per cell
(`grid2d.rs:238`). Common shapes are tiny (Moore n=1 → 8, VonNeumann n=1 → 4).
A `SmallVec<[(i32,i32); 8]>` (or `[(i8,i8); N]`) keeps them on the subrule
inline, improving locality. Combined with 3.1 this becomes a small array of
precomputed linear `isize` offsets — the ideal inner loop.

### 3.6 Cache `thread_count()` outside the step loop

The override path takes a `Mutex` lock on **every** `thread_count()` call
(`threads.rs:56`), and `step()` calls it every step (`grid1d.rs:323`,
`grid2d.rs:302`). One lock per step is minor but pure waste: use an
`AtomicUsize` (0 = no override) for the override slot, or read the value once
per grid and refresh on an explicit API call.

### 3.7 Bit-packing and SIMD (larger project)

For the dominant two-type workloads (Life-like rules), the current
4-bytes-per-cell layout leaves a lot on the table:

- Pack "is `criteria_type`" as 1 bit per cell per relevant type; neighbor counts
  become shifts + adds (SWAR) or `popcount` over adjacent words. This routinely
  yields 10–50× over scalar per-cell counting.
- Alternatively keep bytes but process rows with `std::simd` / autovectorizable
  interior loops (needs 3.1 first — bounds branches block vectorization).
- The 1D Wolfram path is even easier: a packed `u64` row implements any n=1 rule
  with three shifts and a table lookup per 64 cells.

This conflicts with the fully generic `CellType`-per-cell model, so it fits best
as a specialized fast path chosen when a rule set is detected to be two-state and
deterministic (same detection Hashlife would need, §6).

### 3.8 Minor items

- `Grid2D::new` does two `entry()` lookups per initial cell
  (`grid2d.rs:148-150`); collapse to one `entry().and_modify().or_insert()` or
  match the simpler `Grid1D::new` loop (`grid1d.rs:181`). Init-time only.
- `TypeCounter::new()` heap-allocates 16 slots per chunk per step
  (`rules.rs:27`). Could be reused per-grid scratch or a fixed-size array; only
  worth it after the bigger items land.
- 1D `next_type` `clone()`s `CellType` into the window arrays
  (`grid1d.rs:222-248`) — free since `CellType: Copy`, but the `.clone()` calls
  and `n > 3 => continue` silent skip deserve a tidy-up: an unsupported `n`
  currently just never matches, with no error at step time (validation catches
  it only if `validate()` was called).

---

## 4. Benchmarking Notes

The current harness (§1) is serviceable but has known limits:

- `Instant`-based timing with mean ± std over `CELLA_BENCH_RUNS` runs; no warmup
  discard, no outlier rejection, no statistical significance test. Deltas vs
  `benchmarks_last.json` of a few percent are noise.
- Benchmarks run as `#[ignore]`d tests, so they build in the `test` profile
  unless the invocation overrides it; ensure `--release` when comparing numbers.
- Thread-variant results are misleading where the size thresholds silently force
  the serial path (see §5 gap 4).

Recommendation: move the timing benchmarks to `criterion` or `divan` benches
(`cella_lib/benches/`), keeping the FNV snapshot tests exactly as they are —
the snapshot mechanism is genuinely good regression armor and orthogonal to
timing. Criterion adds warmup, outlier handling, and HTML reports for free.

---

## 5. Test Coverage Review and Suggested Additions

Current coverage:

| File | Covers |
|---|---|
| `tests/config_tests.rs` | JSON config round-trip, grid building, dimension/length errors |
| `tests/edge_cases.rs` | rule validation edges, tiny grids, history bounds, CountOp zero-neighbor cases |
| `tests/randomness.rs` | randomness 0.0 (always) and 1.0 (never) only |
| `tests/soa_robust.rs` | circular-history FIFO order, parallel history consistency, SoA serde round-trips, out-of-bounds accessors, mixed-n subrules |
| `tests/rule2d_countop.rs` | 2D threshold/CountOp logic |
| `tests/long_suite.rs` | snapshot hashing, stress runs, benchmarks, `_t1/_t4/_t8` variants |
| `lib.rs` inline tests | validation, Knight symmetry/stress, serde, multistate rotation |

Gaps, each tied to the issue it would have caught or the optimization it guards:

1. **Parallel-vs-serial count consistency** — nothing exercises
   `TypeCounter::merge` with ≥2 chunks sharing types, which is why bug 2.1
   survived. Add a `soa_robust`-style test: 3+ cell types, grid above the
   parallel threshold, step both a `set_thread_override(8)` grid and a `_t1`
   reference, assert equal `counts_current`.
2. **Counts invariant** — add a helper asserting
   `counts_current.values().sum::<u64>() == total cells` after each step; call it
   in soa_robust and the long_suite stress runs. Catches any future counting
   regression (including 2.1) for free.
3. **2D dominant-type switching** — no test flips the majority type mid-run
   (bug 2.2). Add a scenario where a growing type overtakes `Inactive` and assert
   `counts_current` stays correct throughout.
4. **1D parallel path is never actually exercised in benchmarks** — the widest
   1D case is 2049 < 8192, so the 1D `_t4/_t8` bench variants silently run
   serial. Add a width ≥ 16384 1D case, and a 2D case straddling the 4096
   threshold (e.g. 63×63 vs 65×65) to make the threshold's effect visible.
5. **Mid-range randomness** — only the 0.0/1.0 extremes are tested. Add a
   statistical test (randomness = 0.5, large N, tolerance band) documenting that
   it is tolerance-based, not exact. Also becomes the regression test for the
   RNG hoist (3.3).
6. **`transition_state_and_buffer` boundary** — add a test calling it with
   `idx == width` (resp. `width*height`), which today passes the `>` guard and
   panics (bug 2.4). Written as `#[should_panic]` now, it flips to a proper
   error assertion once the guard is fixed.
7. **Chunk-boundary/odd-size cases** — extend soa_robust's parallel-consistency
   tests to widths not divisible by the thread count (e.g. 10241) and
   `history_limit` values that make `chunk * hl` alignment interesting (hl 3, 7).
8. **long_suite / bench expansions** — a randomness-rule benchmark (quantifies
   3.3), Knight at 256×256, a `history_limit` sweep (0 / 1 / 7 — hl gates
   threading, see 3.2), and a counts-heavy scenario with 10+ types (quantifies
   3.4). Consider snapshotting final `counts_current` alongside the FNV cell
   hash so statistics regressions are caught by golden files too.

---

## 6. Future Direction: Hashlife

Hashlife (Gosper, 1984) is the algorithm behind tools like Golly that simulate
Life patterns trillions of generations ahead. It is included here as a
possible long-term direction — not a near-term recommendation; the items in §3
are far cheaper and benefit every workload.

### How it works

1. **Quadtree representation.** The grid is a 2^k × 2^k quadtree: a level-*k*
   node is four level-(k−1) children; level-0 nodes are single cells.
2. **Hash-consing.** Nodes are canonicalized in a global hash table: two
   subtrees with identical contents are the *same* node object. Empty space, and
   any repeated structure, costs O(1) memory regardless of extent. (This is the
   same interning idea `cella` already applies to type names with `lasso2` —
   applied to spatial structure instead of strings.)
3. **Memoized `RESULT`.** For each level-*k* node, the algorithm computes the
   level-(k−1) *centered* node that is its state some generations in the future,
   built recursively from nine overlapping child results. Because nodes are
   hash-consed, this `RESULT` is computed **once per distinct subtree ever seen**
   and cached forever.
4. **Superspeed.** In its full form, a level-*k* node's `RESULT` jumps
   2^(k−2) generations at once — the time step grows exponentially with the
   spatial scale of the node. Regular or periodic patterns collapse into a few
   cached nodes, which is where the "trillions of generations" headline
   performance comes from.

The trade-offs: memory grows with pattern *entropy* (chaotic soups memoize
poorly), results arrive in exponential time jumps rather than step-by-step, and
the hash table needs garbage collection on long runs.

### Fit with cella

Works naturally:

- Deterministic, two-state, outer-totalistic rules on `Moore` neighborhoods —
  the Life-like configs — are exactly Hashlife's home turf.
- Multi-state deterministic rules are possible too (Golly's "Super" algorithms
  do this); the leaf alphabet just grows, and with it the memo table.

Fundamental conflicts with current features:

- **`randomness` subrules** break it completely — memoization requires that
  identical subtrees always evolve identically.
- **Per-cell `ages` and history** are not derivable from cell types alone, so a
  node's future is no longer a function of its contents; either those features
  are folded into the cell state (exploding the alphabet) or unavailable in
  Hashlife mode.
- **Interactive painting and per-step observation** (the GUI's bread and butter)
  fight the exponential time jumps and force tree edits that invalidate little,
  but make the algorithm run in its slow regime.

### If it were added

The realistic shape is an alternate engine behind a common trait
(`trait Engine { fn step(&mut self); fn cell_type(&self, idx) -> CellType; ... }`),
selected when a rule set is detected to be deterministic and
history/age-free — the same detection a bit-packed SIMD fast path (§3.7) needs.
A SIMD stepper is the better first investment: it accelerates the interactive
per-step use case cella is actually built around, whereas Hashlife shines
precisely when you *don't* want to watch every step.

---

## Summary of Priorities

1. Fix `TypeCounter::merge` (2.1) — correctness, one line.
2. Fix 2D dominant-type tracking (2.2) and the bounds guards (2.4) — small.
3. Add parallel count-consistency + counts-invariant tests (5.1, 5.2) before
   touching the hot path further.
4. Halo padding (3.1) — biggest stepping win.
5. Thread reuse + threshold tuning + `hl == 0` parallel support (3.2).
6. RNG hoist (3.3), fast-hash counts (3.4), SmallVec offsets (3.5) — small,
   independent wins.
7. Criterion migration (§4) when convenient; bit-packed/SIMD fast path (3.7) and
   Hashlife (§6) as long-term projects, in that order.
