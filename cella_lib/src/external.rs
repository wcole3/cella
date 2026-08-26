//! External model plugin seam.
//!
//! An [`ExternalModel`] replaces the subrule engine for a [`Grid2D`]: instead of
//! evaluating `Rule2D` subrules, `step()` hands each chunk of the grid to the
//! model, which computes the next cell types with whatever logic it likes
//! (equation-based, stochastic, spatially heterogeneous, ...). The engine keeps
//! ownership of every bookkeeping invariant — ages, history, population counts,
//! double buffering, chunked parallelism — so a model cannot corrupt them.
//!
//! Models are open for downstream extension: implement [`ExternalModel`] in any
//! crate, annotate the impl with `#[typetag::serde(name = "...")]`, and the
//! model round-trips through [`crate::config::CellaConfig`] and
//! [`crate::state::GridState`] JSON alongside the grid.
//!
//! The first in-tree implementation is [`crate::wildfire::WildfireModel`].
//!
//! # Determinism contract
//!
//! `step_chunk` is called concurrently from worker threads, potentially with
//! any partition of the grid into chunks. To keep runs reproducible and
//! independent of thread count, a model must derive any randomness it needs
//! from per-cell counters (e.g. a stateless hash of `(seed, ctx.step, index)`)
//! rather than from shared mutable RNG state.

use crate::types::CellType;
use std::any::Any;

/// Read-only view of a grid handed to [`ExternalModel::attach`].
pub struct GridView<'a> {
    pub width: usize,
    pub height: usize,
    /// Current cell types, row-major, length `width * height`.
    pub cells: &'a [CellType],
    /// The engine's inactive/background type.
    pub inactive: CellType,
}

/// Per-chunk context handed to [`ExternalModel::step_chunk`].
///
/// `cells` and dimensions describe the whole grid; the chunk owns the cells at
/// flat indices `start .. start + len`, where `len` is the length of the `next`
/// slice passed alongside this context.
pub struct ChunkCtx<'a> {
    /// The whole grid's current cell types (previous generation), row-major.
    pub cells: &'a [CellType],
    /// Previous-generation ages for this chunk only: `ages[local]` is the age
    /// of cell `start + local`, i.e. how many consecutive steps it has already
    /// spent in its current type.
    pub ages: &'a [u32],
    /// Flat index of the chunk's first cell.
    pub start: usize,
    pub width: usize,
    pub height: usize,
    /// The step counter *before* this step is applied.
    pub step: u64,
    /// The engine's inactive/background type (out-of-bounds reads are inactive).
    pub inactive: CellType,
}

/// A long-range write requested by a model (e.g. fire spotting): set cell
/// `target` to `new_type` this step, bypassing neighborhood locality.
///
/// Events are collected from every chunk, merged, and applied serially by the
/// engine after the chunked pass, gated by [`ExternalModel::event_applies`].
/// Application is idempotent, so the outcome is independent of chunk count and
/// merge order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelEvent {
    /// Flat index of the cell to overwrite. Out-of-range targets are ignored.
    pub target: usize,
    pub new_type: CellType,
}

/// Errors from [`ExternalModel::attach`] validation.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ModelError {
    #[error("layer '{layer}' has length {got}, expected {expected} (width * height)")]
    LayerLength {
        layer: &'static str,
        expected: usize,
        got: usize,
    },
    #[error("invalid parameter: {0}")]
    InvalidParam(String),
    #[error("cell type name collision: {0}")]
    NameCollision(String),
}

/// A pluggable transition model for [`crate::Grid2D`].
///
/// See the [module docs](self) for the engine/model split and the determinism
/// contract. Implementations must also be annotated `#[typetag::serde]` so the
/// model serializes inside configs and snapshots.
#[typetag::serde]
pub trait ExternalModel: Send + Sync {
    /// Validate the model against the grid and (re)build any derived state.
    ///
    /// Called when the model is attached to a grid: on construction from a
    /// config, on snapshot restore, and on explicit
    /// [`crate::Grid2D::attach_model`].
    fn attach(&mut self, view: &GridView<'_>) -> Result<(), ModelError>;

    /// Estimated work per cell in nominal neighbor visits, feeding the
    /// engine's parallel chunk sizing. Defaults to a Moore-1 weight plus the
    /// bookkeeping pass.
    fn work_per_cell(&self) -> usize {
        12
    }

    /// Compute the next type for every cell of one chunk.
    ///
    /// `next[local]` must be written for all `local in 0..next.len()`; the cell's
    /// flat index is `ctx.start + local`. Returns any long-range events.
    fn step_chunk(&self, ctx: &ChunkCtx<'_>, next: &mut [CellType]) -> Vec<ModelEvent>;

    /// Whether an event may overwrite a cell whose next type is currently
    /// `current_next`. Used by the engine's serial application pass; returning
    /// `false` for already-applied targets is what makes application
    /// idempotent. Defaults to always applying.
    fn event_applies(&self, _current_next: CellType, _event: &ModelEvent) -> bool {
        true
    }

    /// Hook invoked when a cell is painted interactively, so derived per-cell
    /// state can be refreshed. Default: no-op.
    fn on_paint(&mut self, _idx: usize, _new_type: CellType) {}

    /// Cell types the application should offer for interaction (e.g. a
    /// painting palette). Default: none.
    fn declared_types(&self) -> Vec<CellType> {
        Vec::new()
    }

    /// Clone into a box; enables `Clone` for grids holding a model.
    fn boxed_clone(&self) -> Box<dyn ExternalModel>;

    /// Typed access for application-side mutation (e.g. a wind slider), via
    /// `Any::downcast_mut`.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl Clone for Box<dyn ExternalModel> {
    fn clone(&self) -> Self {
        self.boxed_clone()
    }
}

impl std::fmt::Debug for dyn ExternalModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExternalModel({})", self.typetag_name())
    }
}

use crate::chunking::{OutChunk, split_chunks};
use crate::grid2d::Grid2D;
use crate::rules::{TypeCounter, apply_counts};
use crate::threads::{chunks_for_work, pool};
use rayon::prelude::*;

impl Grid2D {
    /// Attach an external model, validating it against this grid and building
    /// its derived state. Replaces any previously attached model.
    pub fn attach_model(&mut self, mut model: Box<dyn ExternalModel>) -> Result<(), ModelError> {
        let view = GridView {
            width: self.width,
            height: self.height,
            cells: &self.cells,
            inactive: self.inactive,
        };
        model.attach(&view)?;
        self.model = Some(model);
        Ok(())
    }

    /// Mutable access to the attached model, e.g. to downcast and adjust
    /// parameters between steps.
    pub fn model_mut(&mut self) -> Option<&mut (dyn ExternalModel + 'static)> {
        self.model.as_deref_mut()
    }

    /// One step driven by the attached [`ExternalModel`]. Mirrors the subrule
    /// `step()`: chunked map/reduce over the persistent pool, then the serial
    /// event pass, buffer swap, and count bookkeeping.
    pub(crate) fn step_external(&mut self) {
        let width = self.width;
        let height = self.height;
        let total = width * height;
        let hl = self.history_limit;
        let model = self
            .model
            .as_deref()
            .expect("step_external requires an attached model");
        let nchunks = chunks_for_work(total.saturating_mul(model.work_per_cell().max(1)));
        let cells = &self.cells;
        let inactive = self.inactive;
        let dt = self.dominant_type;
        let step = self.step;

        let (mut count_map, mut events) = if nchunks <= 1 {
            let mut out = OutChunk {
                start: 0,
                next_cells: &mut self.next_cells,
                ages: &mut self.ages,
                history_data: &mut self.history_data,
                history_heads: &mut self.history_heads,
                history_counts: &mut self.history_counts,
            };
            Self::step_chunk_external(
                model, cells, &mut out, hl, width, height, step, inactive, dt,
            )
        } else {
            let chunk = total.div_ceil(nchunks);
            let mut chunks = split_chunks(
                &mut self.next_cells,
                &mut self.ages,
                &mut self.history_data,
                &mut self.history_heads,
                &mut self.history_counts,
                hl,
                chunk,
            );
            pool(nchunks).install(|| {
                chunks
                    .par_iter_mut()
                    .map(|c| {
                        Self::step_chunk_external(
                            model, cells, c, hl, width, height, step, inactive, dt,
                        )
                    })
                    .reduce(
                        || (TypeCounter::new(), Vec::new()),
                        |mut a, mut b| {
                            a.0.merge(&b.0);
                            a.1.append(&mut b.1);
                            a
                        },
                    )
            })
        };

        // Serial event pass. Sorting makes the application order independent of
        // the parallel merge order; `event_applies` makes it idempotent. Both
        // together make the outcome independent of chunk and thread count.
        events.sort_unstable_by_key(|e| (e.target, e.new_type));
        for ev in &events {
            if ev.target >= total {
                continue;
            }
            let current_next = self.next_cells[ev.target];
            if current_next == ev.new_type || !model.event_applies(current_next, ev) {
                continue;
            }
            if current_next != dt {
                count_map.sub(current_next);
            }
            if ev.new_type != dt {
                count_map.add(ev.new_type);
            }
            self.next_cells[ev.target] = ev.new_type;
            // The chunk pass has already recorded the previous state in the
            // history buffers; only the age needs fixing. An event that
            // restores a cell's previous type still resets its age.
            self.ages[ev.target] = 0;
        }

        std::mem::swap(&mut self.cells, &mut self.next_cells);
        apply_counts(
            &mut self.counts_current,
            &mut self.peak_counts,
            &mut self.dominant_type,
            total as u64,
            &count_map,
        );
        self.step = self.step.saturating_add(1);
    }

    /// Engine wrapper around [`ExternalModel::step_chunk`]: the model fills the
    /// chunk's next types, then the engine runs its own bookkeeping pass
    /// (history, ages, population counts) over the same chunk.
    #[allow(clippy::too_many_arguments)]
    fn step_chunk_external(
        model: &dyn ExternalModel,
        cells: &[CellType],
        out: &mut OutChunk<'_>,
        history_limit: usize,
        width: usize,
        height: usize,
        step: u64,
        inactive: CellType,
        dt: CellType,
    ) -> (TypeCounter, Vec<ModelEvent>) {
        let start = out.start;
        let next_cells = &mut *out.next_cells;
        let ages = &mut *out.ages;
        let history_data = &mut *out.history_data;
        let history_heads = &mut *out.history_heads;
        let history_counts = &mut *out.history_counts;

        let ctx = ChunkCtx {
            cells,
            ages: &*ages,
            start,
            width,
            height,
            step,
            inactive,
        };
        let events = model.step_chunk(&ctx, next_cells);

        let mut count_map = TypeCounter::new();
        for local in 0..next_cells.len() {
            let idx = start + local;
            let cur = cells[idx];
            let new_type = next_cells[local];
            if history_limit > 0 {
                let base = local * history_limit;
                let h = history_heads[local] as usize;
                history_data[base + h] = cur;
                history_heads[local] = if h + 1 == history_limit {
                    0
                } else {
                    (h + 1) as u8
                };
                let c = history_counts[local] as usize;
                if c < history_limit {
                    history_counts[local] = (c + 1) as u8;
                }
            }
            if cur == new_type {
                ages[local] = ages[local].saturating_add(1);
            } else {
                ages[local] = 0;
            }
            if new_type != dt {
                count_map.add(new_type);
            }
        }
        (count_map, events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CellType;
    use serde::{Deserialize, Serialize};

    /// Minimal model proving the plugin seam independently of wildfire:
    /// every cell becomes `out_type` each step, optionally emitting one event.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub(crate) struct ConstModel {
        pub out_name: String,
        /// When set, every chunk emits an event for this flat index ("Marked").
        pub event_target: Option<usize>,
        #[serde(skip)]
        pub attached: bool,
    }

    #[typetag::serde(name = "test_const")]
    impl ExternalModel for ConstModel {
        fn attach(&mut self, view: &GridView<'_>) -> Result<(), ModelError> {
            if view.width == 0 {
                return Err(ModelError::InvalidParam("zero width".into()));
            }
            self.attached = true;
            Ok(())
        }

        fn step_chunk(&self, _ctx: &ChunkCtx<'_>, next: &mut [CellType]) -> Vec<ModelEvent> {
            let t = CellType::new(&self.out_name);
            next.fill(t);
            match self.event_target {
                Some(target) => vec![ModelEvent {
                    target,
                    new_type: CellType::new("Marked"),
                }],
                None => Vec::new(),
            }
        }

        fn boxed_clone(&self) -> Box<dyn ExternalModel> {
            Box::new(self.clone())
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    fn const_model(out_name: &str, event_target: Option<usize>) -> ConstModel {
        ConstModel {
            out_name: out_name.into(),
            event_target,
            attached: false,
        }
    }

    #[test]
    fn const_model_steps_a_grid_and_oob_events_are_dropped() {
        use crate::{Grid2D, Rule2D};
        let a = CellType::new("A");
        let mut g = Grid2D::new(2, 2, 0, vec![a; 4], Rule2D { subrules: vec![] });
        // Out-of-range event target: silently dropped by the engine.
        g.attach_model(Box::new(const_model("X", Some(usize::MAX))))
            .unwrap();
        g.step();
        let x = CellType::new("X");
        assert!((0..4).all(|i| g.cell_type(i) == x));
        assert_eq!(g.counts_current.values().sum::<u64>(), 4);
        // In-range event applies (default event_applies accepts everything).
        g.attach_model(Box::new(const_model("X", Some(3)))).unwrap();
        g.step();
        assert_eq!(g.cell_type(3), CellType::new("Marked"));
    }

    #[test]
    fn grid2d_json_with_model_reattaches_on_deserialize() {
        use crate::{Grid2D, Rule2D};
        let a = CellType::new("A");
        let g = Grid2D::new(2, 1, 0, vec![a; 2], Rule2D { subrules: vec![] });
        let states = serde_json::to_value(g.to_cell_states()).unwrap();
        let model: Box<dyn ExternalModel> = Box::new(const_model("X", None));
        let json = serde_json::json!({
            "width": 2, "height": 1, "history_limit": 0,
            "cell_states": states, "step": 0,
            "rule": { "subrules": [] },
            "counts_current": {}, "peak_counts": {},
            "inactive": "Inactive",
            "model": model,
        });
        let mut back: Grid2D = serde_json::from_value(json.clone()).unwrap();
        assert!(back.model.is_some(), "model deserialized and attached");
        back.step();
        assert_eq!(back.cell_type(0), CellType::new("X"));
        // A model that fails attach turns into a deserialize error.
        let mut bad = json;
        bad["width"] = serde_json::json!(0);
        bad["height"] = serde_json::json!(0);
        bad["cell_states"] = serde_json::json!([]);
        assert!(serde_json::from_value::<Grid2D>(bad).is_err());
    }

    #[test]
    fn typetag_round_trip_preserves_model() {
        let m: Box<dyn ExternalModel> = Box::new(const_model("X", None));
        let json = serde_json::to_string(&m).unwrap();
        assert!(
            json.contains("test_const"),
            "externally tagged by typetag name: {json}"
        );
        let back: Box<dyn ExternalModel> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.typetag_name(), "test_const");
        // Untouched wire format carries the payload.
        let as_value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(as_value["test_const"]["out_name"], "X");
    }

    #[test]
    fn boxed_clone_via_clone_impl() {
        let m: Box<dyn ExternalModel> = Box::new(const_model("Y", None));
        let c = m.clone();
        assert_eq!(c.typetag_name(), m.typetag_name());
        assert_eq!(format!("{:?}", &*c), "ExternalModel(test_const)");
    }

    #[test]
    fn default_trait_methods() {
        let mut m = const_model("Z", None);
        assert_eq!(ExternalModel::work_per_cell(&m), 12);
        assert!(m.event_applies(
            CellType::new("A"),
            &ModelEvent {
                target: 0,
                new_type: CellType::new("B")
            }
        ));
        assert!(m.declared_types().is_empty());
        m.on_paint(0, CellType::new("A")); // no-op default
        assert!(m.as_any_mut().downcast_mut::<ConstModel>().is_some());
    }

    #[test]
    fn attach_validates() {
        let mut m = const_model("X", None);
        let cells = vec![CellType::inactive(); 4];
        let bad = GridView {
            width: 0,
            height: 0,
            cells: &[],
            inactive: CellType::inactive(),
        };
        assert!(matches!(m.attach(&bad), Err(ModelError::InvalidParam(_))));
        let good = GridView {
            width: 2,
            height: 2,
            cells: &cells,
            inactive: CellType::inactive(),
        };
        assert!(m.attach(&good).is_ok());
        assert!(m.attached);
    }

    #[test]
    fn model_error_display() {
        let e = ModelError::LayerLength {
            layer: "density",
            expected: 4,
            got: 3,
        };
        assert_eq!(
            e.to_string(),
            "layer 'density' has length 3, expected 4 (width * height)"
        );
        assert_eq!(
            ModelError::InvalidParam("p0".into()).to_string(),
            "invalid parameter: p0"
        );
        assert_eq!(
            ModelError::NameCollision("Burning".into()).to_string(),
            "cell type name collision: Burning"
        );
    }
}
