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
use serde::{Deserialize, Serialize};
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

/// A model parameter's value, as exchanged with an application UI.
///
/// This is the "wire" value a control panel reads from
/// [`ExternalModel::get_param`] and writes back through
/// [`ExternalModel::set_param`]. Which variant is expected for a given
/// parameter is described by that parameter's [`ParamKind`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParamValue {
    Float(f64),
    Int(i64),
    Bool(bool),
    Choice(String),
    /// A string of bits packed into a `u128` (bit `i` is switch `i`), e.g. a
    /// 1D Wolfram code. Serialised as a decimal string so 128-bit values
    /// survive JSON readers that only know 64-bit numbers.
    Bits(#[serde(with = "crate::rules::serde_u128")] u128),
}

/// What kind of control a parameter wants, and its valid range.
///
/// A UI uses this to decide what widget to draw (a slider, a spinner, a
/// checkbox, a dropdown) and how to constrain user input, without knowing
/// anything about the concrete model the parameter belongs to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParamKind {
    /// Continuous value; bounds are inclusive.
    Float {
        min: f64,
        max: f64,
        step: f64,
    },
    /// Discrete value; bounds are inclusive.
    Int {
        min: i64,
        max: i64,
    },
    Bool,
    /// One of a fixed set of names.
    Choice {
        options: Vec<String>,
    },
    /// `len` independent on/off bits (1..=128) packed into a `u128`. A rule
    /// table is the typical case: a 1D subrule with radius `n` has
    /// `2^(2n+1)` bits, one per neighbourhood pattern.
    Bits {
        len: u32,
    },
}

/// Self-description of one tunable parameter.
///
/// A model returns a list of these from [`ExternalModel::params`] so a
/// generic panel can build one control per parameter without any
/// model-specific code.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

    /// Parameters this model exposes for interactive tuning. Default: none.
    ///
    /// A generic panel calls this once to learn what controls to draw, then
    /// reads and writes values through [`Self::get_param`] and
    /// [`Self::set_param`] using each [`ParamDesc::key`].
    fn params(&self) -> Vec<ParamDesc> {
        Vec::new()
    }

    /// Current value of `key`, or `None` if this model has no such
    /// parameter. Default: `None` for every key.
    ///
    /// Every key [`Self::params`] lists must return `Some` — the engine's
    /// rollback depends on it. [`crate::Grid2D::set_model_param`] restores the
    /// value this method reported when a re-attach rejects an edit, so a
    /// `reattach` key with no value here has no way back; the engine refuses
    /// to write such a key at all rather than strand the model.
    fn get_param(&self, _key: &str) -> Option<ParamValue> {
        None
    }

    /// Write `key`. Implementations need only check what `attach` does not;
    /// the engine re-runs `attach` and rolls back on failure.
    ///
    /// This method validates nothing and rebuilds nothing on its own: it does
    /// not check the descriptor's bounds, and it does not refresh derived
    /// state. Library callers should go through
    /// [`crate::Grid2D::set_model_param`], which does both. Calling it
    /// directly is for a model that is not attached to a grid — the clone
    /// inside a snapshot, say, which is re-attached when it is restored.
    ///
    /// Default: every key is rejected with [`ModelError::InvalidParam`],
    /// which is correct for a model that declares no parameters via
    /// [`Self::params`].
    fn set_param(&mut self, key: &str, _value: ParamValue) -> Result<(), ModelError> {
        Err(ModelError::InvalidParam(format!(
            "unknown parameter '{key}'"
        )))
    }

    /// Reseed the model's own randomness. Called by [`crate::Grid2D::set_seed`]
    /// and by ensembles, which give every member a different seed. A model
    /// that draws no random numbers can ignore it (the default does nothing);
    /// a model that does should store the seed and use it from the next step
    /// on — no re-attach is expected. This is separate from a `seed`
    /// parameter a model may show read-only in its [`Self::params`] list: the
    /// panel must not change the seed mid-run, an ensemble must.
    fn set_seed(&mut self, _seed: u64) {}

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

/// Check a value against a parameter's declared kind and bounds.
///
/// This is the generic validation the engine runs before any model code sees
/// the value, which is why a model author writes no range checks at all: the
/// value must be the variant the kind asks for, a `Float` must be a real
/// number inside `[min, max]`, an `Int` must be inside `[min, max]`, and a
/// `Choice` must be one of the declared options.
///
/// A rejected value comes back as [`ModelError::InvalidParam`] saying what was
/// wrong with it. The message does not name the parameter; callers add that
/// with [`with_param_key`].
fn check_value_against_kind(kind: &ParamKind, value: &ParamValue) -> Result<(), ModelError> {
    let reject = |msg: String| Err(ModelError::InvalidParam(msg));
    match (kind, value) {
        (ParamKind::Float { min, max, .. }, ParamValue::Float(v)) => {
            if v.is_nan() {
                return reject("value is not a number".into());
            }
            if v < min || v > max {
                return reject(format!("{v} is outside the allowed range {min} to {max}"));
            }
            Ok(())
        }
        (ParamKind::Int { min, max }, ParamValue::Int(v)) => {
            if v < min || v > max {
                return reject(format!("{v} is outside the allowed range {min} to {max}"));
            }
            Ok(())
        }
        (ParamKind::Bool, ParamValue::Bool(_)) => Ok(()),
        (ParamKind::Choice { options }, ParamValue::Choice(v)) => {
            if options.iter().any(|o| o == v) {
                Ok(())
            } else {
                reject(format!("'{v}' is not one of: {}", options.join(", ")))
            }
        }
        (ParamKind::Bits { len }, ParamValue::Bits(v)) => {
            if *len < 128 && (v >> len) != 0 {
                reject(format!("{v:#x} does not fit in {len} bits"))
            } else {
                Ok(())
            }
        }
        // Every other pairing is a value of the wrong shape for this control.
        _ => reject(format!("expected {kind:?}, got {value:?}")),
    }
}

/// Put the parameter key in front of a validation message, so an error shown
/// in a status bar says which control was refused and why. Errors that are not
/// about a parameter value pass through unchanged.
fn with_param_key(key: &str, err: ModelError) -> ModelError {
    match err {
        ModelError::InvalidParam(why) => ModelError::InvalidParam(format!("'{key}': {why}")),
        other => other,
    }
}

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

    /// Set one parameter on the attached model, re-validating through
    /// [`ExternalModel::attach`].
    ///
    /// The engine does the generic work so a model does not have to. The value
    /// is checked against the parameter's own [`ParamDesc`] — its kind and its
    /// bounds — before the model is touched. Only if the descriptor says
    /// [`ParamDesc::reattach`] does `attach` run afterwards, rebuilding any
    /// derived state and applying whatever deeper checks the model makes.
    ///
    /// On a failed re-attach the previous value is written back and `attach` is
    /// run again. That is what keeps a rejected edit from leaving the model in
    /// a state `attach` would not accept — and it rests on one contract the
    /// model has to keep: every key [`ExternalModel::params`] lists must have a
    /// value [`ExternalModel::get_param`] returns, because that value is the
    /// only thing there is to put back. A `reattach` key that breaks the
    /// contract is refused before the model is written, since a rollback for it
    /// would have nowhere to go.
    ///
    /// The call fails with [`ModelError::InvalidParam`] when no model is
    /// attached, the key is unknown, the parameter is read-only, the value does
    /// not fit the parameter's [`ParamKind`], or the parameter says `reattach`
    /// while `get_param` has no value for it. An error from the model's own
    /// [`ExternalModel::set_param`], or from a rejected `attach`, is returned
    /// unchanged.
    pub fn set_model_param(&mut self, key: &str, value: ParamValue) -> Result<(), ModelError> {
        // Borrow the fields separately so a read-only view of the grid can be
        // built while the model is mutably borrowed.
        let Grid2D {
            width,
            height,
            cells,
            inactive,
            model,
            ..
        } = self;
        let model = model.as_deref_mut().ok_or_else(|| {
            ModelError::InvalidParam(format!("no model is attached, cannot set '{key}'"))
        })?;

        // The model's own description of the parameter decides everything
        // below: whether it may be written, what values are legal, and whether
        // derived state has to be rebuilt.
        let desc = model
            .params()
            .into_iter()
            .find(|d| d.key == key)
            .ok_or_else(|| ModelError::InvalidParam(format!("unknown parameter '{key}'")))?;
        if desc.read_only {
            return Err(ModelError::InvalidParam(format!(
                "parameter '{key}' is read-only"
            )));
        }
        check_value_against_kind(&desc.kind, &value).map_err(|e| with_param_key(key, e))?;

        // Read the old value before anything is written: it is what a failed
        // re-attach has to put back. A `reattach` parameter the model will not
        // report a value for has no such fallback, so the write is refused
        // here, with the model untouched, rather than risking a rollback that
        // cannot restore anything.
        let prev = model.get_param(key);
        if desc.reattach && prev.is_none() {
            return Err(ModelError::InvalidParam(format!(
                "'{key}' is listed by params() but get_param returns no value for it"
            )));
        }
        model.set_param(key, value)?;

        if desc.reattach {
            let view = GridView {
                width: *width,
                height: *height,
                cells: cells.as_slice(),
                inactive: *inactive,
            };
            if let Err(e) = model.attach(&view) {
                // Roll back: the old value is one the model already accepted,
                // so both calls here are expected to succeed and their results
                // carry no new information — the original error is what the
                // caller needs. `prev` is always `Some` on this path, because
                // the guard above refuses a `reattach` key without one; the
                // `if let` is only how that is unwrapped without a panic.
                if let Some(prev) = prev {
                    let _ = model.set_param(key, prev);
                }
                let _ = model.attach(&view);
                return Err(e);
            }
        }
        Ok(())
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
        /// Tunable parameter used to exercise `Grid2D::set_model_param`. Its
        /// descriptor allows `0.0..=10.0`, but `attach` refuses anything above
        /// `5.0` — a range deliberately wider than `attach` accepts, so the
        /// rollback path is reachable.
        pub threshold: f64,
    }

    #[typetag::serde(name = "test_const")]
    impl ExternalModel for ConstModel {
        fn attach(&mut self, view: &GridView<'_>) -> Result<(), ModelError> {
            if view.width == 0 {
                return Err(ModelError::InvalidParam("zero width".into()));
            }
            if self.threshold > 5.0 {
                return Err(ModelError::InvalidParam(
                    "threshold above 5 is rejected by attach".into(),
                ));
            }
            self.attached = true;
            Ok(())
        }

        fn params(&self) -> Vec<ParamDesc> {
            vec![
                ParamDesc {
                    key: "threshold".into(),
                    label: "Threshold".into(),
                    group: None,
                    help: None,
                    unit: None,
                    kind: ParamKind::Float {
                        min: 0.0,
                        max: 10.0,
                        step: 0.5,
                    },
                    reattach: true,
                    read_only: false,
                },
                ParamDesc {
                    key: "out_name".into(),
                    label: "Output type".into(),
                    group: None,
                    help: None,
                    unit: None,
                    kind: ParamKind::Choice {
                        options: vec!["X".into(), "Y".into()],
                    },
                    reattach: false,
                    read_only: true,
                },
            ]
        }

        fn get_param(&self, key: &str) -> Option<ParamValue> {
            match key {
                "threshold" => Some(ParamValue::Float(self.threshold)),
                "out_name" => Some(ParamValue::Choice(self.out_name.clone())),
                _ => None,
            }
        }

        fn set_param(&mut self, key: &str, value: ParamValue) -> Result<(), ModelError> {
            match (key, value) {
                ("threshold", ParamValue::Float(v)) => {
                    self.threshold = v;
                    Ok(())
                }
                _ => Err(ModelError::InvalidParam(format!(
                    "unknown parameter '{key}'"
                ))),
            }
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
            threshold: 1.0,
        }
    }

    /// A model that overrides nothing beyond the four required methods, so the
    /// trait's default implementations are the ones under test.
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub(crate) struct BareModel;

    #[typetag::serde(name = "test_bare")]
    impl ExternalModel for BareModel {
        fn attach(&mut self, _view: &GridView<'_>) -> Result<(), ModelError> {
            Ok(())
        }

        fn step_chunk(&self, ctx: &ChunkCtx<'_>, next: &mut [CellType]) -> Vec<ModelEvent> {
            next.fill(ctx.inactive);
            Vec::new()
        }

        fn boxed_clone(&self) -> Box<dyn ExternalModel> {
            Box::new(self.clone())
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    /// A model that breaks the one promise `params()` makes: it advertises two
    /// keys — one `reattach: true`, one not — that its `get_param` will not
    /// answer.
    ///
    /// `get_param` is left at the trait default (`None` for everything), which
    /// is exactly the mistake a model author makes by forgetting to extend
    /// `get_param` after adding a descriptor. `set_param` records that it was
    /// called, so a test can prove the engine refused the write before the
    /// model was touched.
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub(crate) struct ForgetfulModel {
        #[serde(skip)]
        pub written: bool,
    }

    #[typetag::serde(name = "test_forgetful")]
    impl ExternalModel for ForgetfulModel {
        fn attach(&mut self, _view: &GridView<'_>) -> Result<(), ModelError> {
            Ok(())
        }

        fn params(&self) -> Vec<ParamDesc> {
            // The same omission twice, once on each side of the guard:
            // `ghost` needs a rollback value and has none, `cheap` never
            // rebuilds anything and so never needs one.
            ["ghost", "cheap"]
                .into_iter()
                .map(|key| ParamDesc {
                    key: key.into(),
                    label: key.into(),
                    group: None,
                    help: None,
                    unit: None,
                    kind: ParamKind::Float {
                        min: 0.0,
                        max: 1.0,
                        step: 0.1,
                    },
                    reattach: key == "ghost",
                    read_only: false,
                })
                .collect()
        }

        fn set_param(&mut self, _key: &str, _value: ParamValue) -> Result<(), ModelError> {
            self.written = true;
            Ok(())
        }

        fn step_chunk(&self, ctx: &ChunkCtx<'_>, next: &mut [CellType]) -> Vec<ModelEvent> {
            next.fill(ctx.inactive);
            Vec::new()
        }

        fn boxed_clone(&self) -> Box<dyn ExternalModel> {
            Box::new(self.clone())
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    /// A 2x2 grid with a `ConstModel` attached, ready for parameter edits.
    fn param_grid() -> crate::Grid2D {
        use crate::{Grid2D, Rule2D};
        let a = CellType::new("A");
        let mut g = Grid2D::new(2, 2, 0, vec![a; 4], Rule2D { subrules: vec![] });
        g.attach_model(Box::new(const_model("X", None))).unwrap();
        g
    }

    /// Whether `ConstModel::attach` has run since `clear_attached` last cleared
    /// the flag — this is how the tests below see if a re-attach happened.
    fn attached_flag(g: &mut crate::Grid2D) -> bool {
        g.model_mut()
            .unwrap()
            .as_any_mut()
            .downcast_mut::<ConstModel>()
            .unwrap()
            .attached
    }

    /// Clear the attach marker so the next check reports only new attaches.
    fn clear_attached(g: &mut crate::Grid2D) {
        g.model_mut()
            .unwrap()
            .as_any_mut()
            .downcast_mut::<ConstModel>()
            .unwrap()
            .attached = false;
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
        use crate::{Grid2D, Rule2D};
        let mut m = BareModel;
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
        assert!(m.as_any_mut().downcast_mut::<BareModel>().is_some());
        // A model with no overrides declares no parameters, ...
        assert!(m.params().is_empty());
        // ... reports no value for any key, ...
        assert!(m.get_param("anything").is_none());
        // ... and rejects every write, naming the offending key.
        let err = m
            .set_param("wind_speed", ParamValue::Float(1.0))
            .unwrap_err();
        assert!(matches!(err, ModelError::InvalidParam(_)));
        assert!(
            err.to_string().contains("wind_speed"),
            "message names the key: {err}"
        );
        // The same model still clones through the trait object, round-trips,
        // and drives a grid.
        let boxed: Box<dyn ExternalModel> = Box::new(m);
        let boxed = boxed.clone();
        let back: Box<dyn ExternalModel> =
            serde_json::from_str(&serde_json::to_string(&boxed).unwrap()).unwrap();
        assert_eq!(back.typetag_name(), "test_bare");
        let mut g = Grid2D::new(
            1,
            1,
            0,
            vec![CellType::new("A")],
            Rule2D { subrules: vec![] },
        );
        g.attach_model(back).unwrap();
        g.step();
        assert_eq!(g.cell_type(0), CellType::inactive());
    }

    #[test]
    fn param_vocabulary_round_trips_through_json() {
        let desc = ParamDesc {
            key: "wind_speed".into(),
            label: "Wind speed".into(),
            group: Some("Weather".into()),
            help: Some("Sustained wind speed".into()),
            unit: Some("m/s".into()),
            kind: ParamKind::Float {
                min: 0.0,
                max: 40.0,
                step: 0.5,
            },
            reattach: true,
            read_only: false,
        };
        let json = serde_json::to_string(&desc).unwrap();
        let back: ParamDesc = serde_json::from_str(&json).unwrap();
        assert_eq!(back, desc);

        let choice = ParamKind::Choice {
            options: vec!["Light".into(), "Heavy".into()],
        };
        let back: ParamKind =
            serde_json::from_str(&serde_json::to_string(&choice).unwrap()).unwrap();
        assert_eq!(back, choice);
        let int_kind = ParamKind::Int { min: 0, max: 10 };
        let back: ParamKind =
            serde_json::from_str(&serde_json::to_string(&int_kind).unwrap()).unwrap();
        assert_eq!(back, int_kind);
        let bool_kind = ParamKind::Bool;
        let back: ParamKind =
            serde_json::from_str(&serde_json::to_string(&bool_kind).unwrap()).unwrap();
        assert_eq!(back, bool_kind);

        for value in [
            ParamValue::Float(1.5),
            ParamValue::Int(3),
            ParamValue::Bool(true),
            ParamValue::Choice("Heavy".into()),
        ] {
            let back: ParamValue =
                serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
            assert_eq!(back, value);
        }
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

    #[test]
    fn check_value_against_kind_accepts_and_rejects() {
        let float = ParamKind::Float {
            min: 0.0,
            max: 10.0,
            step: 0.5,
        };
        // Inside the inclusive bounds, including both endpoints.
        assert!(check_value_against_kind(&float, &ParamValue::Float(5.0)).is_ok());
        assert!(check_value_against_kind(&float, &ParamValue::Float(0.0)).is_ok());
        assert!(check_value_against_kind(&float, &ParamValue::Float(10.0)).is_ok());
        let low = check_value_against_kind(&float, &ParamValue::Float(-0.5)).unwrap_err();
        assert!(low.to_string().contains("-0.5"), "names the value: {low}");
        let high = check_value_against_kind(&float, &ParamValue::Float(10.5)).unwrap_err();
        assert!(high.to_string().contains("10.5"), "names the value: {high}");
        let nan = check_value_against_kind(&float, &ParamValue::Float(f64::NAN)).unwrap_err();
        assert!(nan.to_string().contains("not a number"), "{nan}");

        let int = ParamKind::Int { min: 1, max: 4 };
        assert!(check_value_against_kind(&int, &ParamValue::Int(1)).is_ok());
        assert!(check_value_against_kind(&int, &ParamValue::Int(4)).is_ok());
        assert!(check_value_against_kind(&int, &ParamValue::Int(0)).is_err());
        assert!(check_value_against_kind(&int, &ParamValue::Int(5)).is_err());

        // A bool has no range to check.
        assert!(check_value_against_kind(&ParamKind::Bool, &ParamValue::Bool(true)).is_ok());

        let choice = ParamKind::Choice {
            options: vec!["Light".into(), "Heavy".into()],
        };
        assert!(check_value_against_kind(&choice, &ParamValue::Choice("Heavy".into())).is_ok());
        let unknown =
            check_value_against_kind(&choice, &ParamValue::Choice("Medium".into())).unwrap_err();
        assert!(
            unknown.to_string().contains("Light, Heavy"),
            "lists the options: {unknown}"
        );

        // A value of the wrong variant for the kind is rejected too.
        let mismatch = check_value_against_kind(&float, &ParamValue::Int(3)).unwrap_err();
        assert!(matches!(mismatch, ModelError::InvalidParam(_)));
        assert!(mismatch.to_string().contains("Int(3)"), "{mismatch}");
        assert!(check_value_against_kind(&int, &ParamValue::Float(3.0)).is_err());
        assert!(
            check_value_against_kind(&ParamKind::Bool, &ParamValue::Choice("x".into())).is_err()
        );
        assert!(check_value_against_kind(&choice, &ParamValue::Bool(false)).is_err());

        // Bits: any value fits in 128 bits; a shorter table rejects high bits.
        let byte = ParamKind::Bits { len: 8 };
        assert!(check_value_against_kind(&byte, &ParamValue::Bits(0)).is_ok());
        assert!(check_value_against_kind(&byte, &ParamValue::Bits(255)).is_ok());
        let too_wide = check_value_against_kind(&byte, &ParamValue::Bits(256)).unwrap_err();
        assert!(
            format!("{too_wide}").contains("does not fit in 8 bits"),
            "{too_wide}"
        );
        let full = ParamKind::Bits { len: 128 };
        assert!(check_value_against_kind(&full, &ParamValue::Bits(u128::MAX)).is_ok());
        assert!(check_value_against_kind(&byte, &ParamValue::Int(3)).is_err());
        assert!(check_value_against_kind(&int, &ParamValue::Bits(3)).is_err());
    }

    #[test]
    fn with_param_key_names_the_parameter() {
        let e = with_param_key("threshold", ModelError::InvalidParam("too big".into()));
        assert_eq!(e.to_string(), "invalid parameter: 'threshold': too big");
        // Any other kind of error passes through untouched.
        let other = with_param_key("threshold", ModelError::NameCollision("Burning".into()));
        assert_eq!(other, ModelError::NameCollision("Burning".into()));
    }

    #[test]
    fn set_model_param_applies_within_bounds_and_reattaches() {
        let mut g = param_grid();
        clear_attached(&mut g);
        g.set_model_param("threshold", ParamValue::Float(0.0))
            .unwrap();
        assert_eq!(
            g.model_mut().unwrap().get_param("threshold"),
            Some(ParamValue::Float(0.0)),
            "the lower bound is accepted and readable back"
        );
        assert!(
            attached_flag(&mut g),
            "a reattach: true parameter re-runs attach"
        );
        // The largest value attach accepts.
        g.set_model_param("threshold", ParamValue::Float(5.0))
            .unwrap();
        assert_eq!(
            g.model_mut().unwrap().get_param("threshold"),
            Some(ParamValue::Float(5.0))
        );
    }

    #[test]
    fn set_model_param_outside_descriptor_bounds_leaves_model_untouched() {
        let mut g = param_grid();
        g.set_model_param("threshold", ParamValue::Float(2.0))
            .unwrap();
        clear_attached(&mut g);
        let err = g
            .set_model_param("threshold", ParamValue::Float(11.0))
            .unwrap_err();
        assert!(matches!(err, ModelError::InvalidParam(_)));
        assert!(
            err.to_string().contains("threshold"),
            "names the key: {err}"
        );
        assert_eq!(
            g.model_mut().unwrap().get_param("threshold"),
            Some(ParamValue::Float(2.0)),
            "the model never saw the rejected value"
        );
        assert!(
            !attached_flag(&mut g),
            "a value the descriptor rejects never reaches attach"
        );
        // A value of the wrong kind is refused the same way.
        assert!(g.set_model_param("threshold", ParamValue::Int(3)).is_err());
        assert!(!attached_flag(&mut g));
    }

    #[test]
    fn set_model_param_rolls_back_when_attach_rejects() {
        let mut g = param_grid();
        g.set_model_param("threshold", ParamValue::Float(2.0))
            .unwrap();
        clear_attached(&mut g);
        // 7.0 is inside the descriptor's 0..=10, so the bounds check lets it
        // through, but ConstModel::attach refuses anything above 5.
        let err = g
            .set_model_param("threshold", ParamValue::Float(7.0))
            .unwrap_err();
        assert!(matches!(err, ModelError::InvalidParam(_)));
        assert_eq!(
            g.model_mut().unwrap().get_param("threshold"),
            Some(ParamValue::Float(2.0)),
            "the previous value was put back"
        );
        assert!(
            attached_flag(&mut g),
            "the rollback re-attached, so derived state matches the value"
        );
        // The model is still attached and the grid still steps.
        g.step();
        assert_eq!(g.cell_type(0), CellType::new("X"));
    }

    #[test]
    fn set_model_param_refuses_a_reattach_key_get_param_will_not_answer() {
        // Rollback after a rejected attach can only put back a value
        // `get_param` handed out. For a `reattach` parameter with no such
        // value the engine would have no way home, so it refuses the write
        // outright rather than risk stranding the model in a state its own
        // attach would not accept.
        use crate::{Grid2D, Rule2D};
        let a = CellType::new("A");
        let mut g = Grid2D::new(2, 2, 0, vec![a; 4], Rule2D { subrules: vec![] });
        g.attach_model(Box::new(ForgetfulModel { written: false }))
            .unwrap();

        let err = g
            .set_model_param("ghost", ParamValue::Float(0.5))
            .unwrap_err();

        assert!(matches!(err, ModelError::InvalidParam(_)));
        let msg = err.to_string();
        assert!(msg.contains("ghost"), "names the key: {msg}");
        assert!(
            msg.contains("get_param"),
            "says which half of the model is at fault: {msg}"
        );
        let written = g
            .model_mut()
            .unwrap()
            .as_any_mut()
            .downcast_mut::<ForgetfulModel>()
            .unwrap()
            .written;
        assert!(!written, "the model must never have been written");

        // The guard is exactly as wide as the rollback that needs it. The same
        // model forgets `cheap` too, but nothing re-attaches for a
        // `reattach: false` parameter, so there is no rollback to strand and
        // the write goes through.
        g.set_model_param("cheap", ParamValue::Float(0.5))
            .expect("a cheap parameter needs no rollback value");
        let written = g
            .model_mut()
            .unwrap()
            .as_any_mut()
            .downcast_mut::<ForgetfulModel>()
            .unwrap()
            .written;
        assert!(written, "the cheap write reached the model");

        // And nothing else about it was disturbed: the grid it is attached to
        // still clones and still steps.
        let mut copy = g.clone();
        copy.step();
        assert_eq!(copy.step, 1, "a refused parameter does not stop the model");
    }

    #[test]
    fn set_model_param_rejects_unknown_read_only_and_missing_model() {
        use crate::{Grid2D, Rule2D};
        let mut g = param_grid();
        let unknown = g
            .set_model_param("nope", ParamValue::Float(1.0))
            .unwrap_err();
        assert!(matches!(unknown, ModelError::InvalidParam(_)));
        assert!(
            unknown.to_string().contains("nope"),
            "names the key: {unknown}"
        );
        let read_only = g
            .set_model_param("out_name", ParamValue::Choice("Y".into()))
            .unwrap_err();
        assert!(
            read_only.to_string().contains("read-only"),
            "says why: {read_only}"
        );
        assert_eq!(
            g.model_mut().unwrap().get_param("out_name"),
            Some(ParamValue::Choice("X".into())),
            "a read-only parameter is never written"
        );
        // The model's own get_param and set_param still ignore keys it does
        // not have.
        assert!(g.model_mut().unwrap().get_param("nope").is_none());
        let direct = g
            .model_mut()
            .unwrap()
            .set_param("nope", ParamValue::Float(1.0))
            .unwrap_err();
        assert!(matches!(direct, ModelError::InvalidParam(_)));
        // With no model attached there is nothing to set.
        let mut bare = Grid2D::new(
            1,
            1,
            0,
            vec![CellType::new("A")],
            Rule2D { subrules: vec![] },
        );
        let none = bare
            .set_model_param("threshold", ParamValue::Float(1.0))
            .unwrap_err();
        assert!(matches!(none, ModelError::InvalidParam(_)));
        assert!(
            none.to_string().contains("threshold"),
            "names the key: {none}"
        );
    }
}
