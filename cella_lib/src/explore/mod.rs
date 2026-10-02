//! Run many simulations instead of one: ensembles, evolution and
//! illumination for any rule or model.
//!
//! A single run is one roll of the dice with one guess at the knobs. This
//! module is for the questions a single run cannot answer:
//!
//! - **"How sure should I be?"** — run an [`ensemble`]: many copies
//!   of the same grid, each with its own seed and its own draw of the knobs,
//!   stepping together. You get a probability per cell, and members can learn
//!   from an observation (a particle filter).
//! - **"Which knobs make the simulation do X?"** — run an
//!   [`evolution`](evolve): a population of *genomes* (knob settings), each
//!   scored by an [`Objective`], bred with selection,
//!   crossover and mutation (a genetic algorithm).
//! - **"What *can* this rule family do?"** — the same evolution in
//!   MAP-Elites or novelty mode fills an [`archive`] with the most
//!   different behaviours it can find.
//!
//! Everything here is model-agnostic. Knobs are addressed by the key grammar
//! of [`crate::tunables`] (`rule.subrules[0].count`, `model.p0`), so the same
//! code drives a plain 1D rule, Life, or the wildfire model. Anything a model
//! needs beyond knob-turning (a weather schedule, a stopping rule) goes into a
//! [`MemberDriver`] that the model's crate provides;
//! the wildfire driver is the worked example.
//!
//! Start with [`Sim`], the one grid type this module works on, then
//! [`genome`] for how knobs become genes, [`metrics`] for how a run is
//! scored, and the two engines.

pub mod archive;
pub mod driver;
pub mod ensemble;
pub mod evolve;
pub mod genome;
pub mod metrics;
pub mod sim;

pub use archive::{
    Archive, ArchiveReport, ArchiveSnapshot, ArchiveStats, Descriptor, DescriptorSpec, Elite,
    SnapshotCell, Thumbnail, thumbnail_from_rows,
};
pub use driver::{Forcing, MemberDriver, MemberState};
pub use ensemble::{AssimilationReport, Ensemble, EnsembleConfig, Member, StateCorrection};
pub use evolve::{
    Evolution, EvolveConfig, GenerationReport, Individual, InitialCondition, Search, Selection,
};
pub use genome::{Gene, GeneKind, GeneSpace, GeneSpec, Genome, Scale};
pub use metrics::{Fitness, Goal, MaskScore, Metric, Objective, When};
pub use sim::Sim;
