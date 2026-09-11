//! [`MemberDriver`]: the hook a model's crate implements so an ensemble or an
//! evolution can do model-specific things without the engine knowing them.
//!
//! The engines know how to clone a grid, reseed it, write genes into knobs
//! and step it. They do not know that a wildfire needs today's wind, that a
//! "wind multiplier" gene should scale that wind, or that a fire may be
//! declared contained at the end of a day. Those are *driver* jobs. A driver
//! is a small serializable object named in the config next to the model:
//!
//! ```json
//! "driver": {"wildfire": {"steps_per_day": 50}}
//! ```
//!
//! It is registered with `#[typetag::serde(name = "...")]` exactly like an
//! [`ExternalModel`](crate::external::ExternalModel), so a model shipped in
//! another crate can ship its driver too. The wildfire driver
//! ([`crate::wildfire::WildfireDriver`]) is the worked example: read it when
//! writing your own.
//!
//! Three ideas to know:
//!
//! - **Forcing** — external inputs for the coming steps, as a `name -> number`
//!   map (`hours`, `wind_speed_ms`, …). The engine stores it and hands it to
//!   [`MemberDriver::apply`] for every member; the driver decides what the
//!   names mean. Omitted names fall back to whatever the driver defaults.
//! - **Free genes** — genes the grid has no knob for (`wind_scale`,
//!   `tau_days`). The driver lists them in [`MemberDriver::free_genes`] with
//!   default ranges and reads them from the genome in `apply`.
//! - **Owned keys** — knobs the driver prefers to write itself, usually
//!   because it has a cheaper or smarter way than the generic setter (the
//!   wildfire driver owns `model.p0` so it can fold in a decay). The engine
//!   skips those when it applies a genome.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::genome::{Gene, GeneSpace, Genome};
use super::sim::Sim;
use crate::external::ModelError;
use crate::rng::Rng;

/// External inputs for the coming steps, by name. See the module docs.
pub type Forcing = BTreeMap<String, f64>;

/// Per-member scratch a driver keeps between calls (a captured base value, a
/// "contained" flag, yesterday's burned count). Numbers only, by name, so
/// the engine can clone, serialize and report it without knowing the keys.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MemberState(pub BTreeMap<String, f64>);

impl MemberState {
    /// Read a value.
    pub fn get(&self, key: &str) -> Option<f64> {
        self.0.get(key).copied()
    }

    /// Write a value.
    pub fn set(&mut self, key: &str, value: f64) {
        self.0.insert(key.to_string(), value);
    }

    /// Read a value as a flag: set and at least 0.5.
    pub fn flag(&self, key: &str) -> bool {
        self.get(key).is_some_and(|v| v >= 0.5)
    }
}

/// Model-specific behaviour for the members of an ensemble or the individuals
/// of an evolution. Every method has a default except `apply` and
/// `boxed_clone`, so a minimal driver is a few lines.
#[typetag::serde]
pub trait MemberDriver: Send + Sync + std::fmt::Debug {
    /// Prepare `sim` for the coming steps: read the free genes and the
    /// forcing, write what they imply into the model or rule. Called once
    /// when a member is created and again whenever the forcing changes.
    fn apply(
        &self,
        sim: &mut Sim,
        genome: &Genome,
        space: &GeneSpace,
        forcing: &Forcing,
        state: &mut MemberState,
    ) -> Result<(), ModelError>;

    /// If `Some(n)`, the engine calls [`Self::period_end`] on every member
    /// each time the step count reaches a multiple of `n` (e.g. once a
    /// simulated day). Default: never.
    fn period_steps(&self) -> Option<u64> {
        None
    }

    /// Runs at each period boundary; may change the model (stop a fire),
    /// draw random numbers from `rng`, and remember things in `state`.
    /// Default: nothing.
    fn period_end(
        &self,
        _sim: &mut Sim,
        _genome: &Genome,
        _space: &GeneSpace,
        _state: &mut MemberState,
        _rng: &mut Rng,
    ) -> Result<(), ModelError> {
        Ok(())
    }

    /// Free genes this driver understands, with their default kinds. A config
    /// may list any of them by bare name without a range. Default: none.
    fn free_genes(&self) -> Vec<Gene> {
        Vec::new()
    }

    /// Knob keys this driver writes itself in `apply`; the engine skips them
    /// when applying a genome. Default: none.
    fn owned_keys(&self) -> Vec<String> {
        Vec::new()
    }

    /// Rebuild an immigrant's grid from the observation just scored, rather
    /// than letting it inherit a parent's grid (state correction: Rochoux et
    /// al. 2014; Xue, Gu & Hu 2012). Called only when
    /// [`super::ensemble::EnsembleConfig::immigrant_source`] is
    /// `Observed`, once per immigrant, on a `sim` that already has its
    /// fresh genome and driver state applied.
    ///
    /// `observed` is a throwaway grid the engine builds purely to carry the
    /// observation's *shape* in this ensemble's own cell types: cell `i` is
    /// painted with the first tracked type when observed there, and with
    /// this ensemble's own inactive (background) type otherwise — so
    /// `observed.cells()[i] != observed.inactive()` is exactly "cell `i` was
    /// observed on", model or not. The engine builds it by cloning a
    /// member, so it has the right dimensions and an attached model, but it
    /// is never stepped and no driver should try to.
    ///
    /// Default: copy `observed`'s cells onto `sim` verbatim, cell for cell,
    /// with [`Sim::paint`] (which is what a driver should use here too —
    /// see its docs for why not [`Sim::reset_cells`]). That is the honest,
    /// model-agnostic thing to do when a driver has nothing smarter to say.
    /// A model that can tell "this observed cell is still on fire" from
    /// "this one has already burned out" (the wildfire driver) overrides
    /// this to say so.
    fn seed_from_observation(&self, sim: &mut Sim, observed: &Sim) -> Result<(), ModelError> {
        for (idx, &t) in observed.cells().iter().enumerate() {
            sim.paint(idx, t)?;
        }
        Ok(())
    }

    /// Clone into a box; lets configs holding a driver be cloned.
    fn boxed_clone(&self) -> Box<dyn MemberDriver>;
}

impl Clone for Box<dyn MemberDriver> {
    fn clone(&self) -> Self {
        self.boxed_clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_state_reads_writes_and_flags() {
        let mut s = MemberState::default();
        assert_eq!(s.get("x"), None);
        assert!(!s.flag("done"));
        s.set("x", 2.5);
        s.set("done", 1.0);
        assert_eq!(s.get("x"), Some(2.5));
        assert!(s.flag("done"));
        s.set("done", 0.0);
        assert!(!s.flag("done"));
        let back: MemberState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
