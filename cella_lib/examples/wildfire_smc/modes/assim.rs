//! `assim` mode's own step: after each window is scored, fold the
//! observation back into the ensemble. See the top-level module doc
//! comment in `main.rs` for what `assim` mode means; the loop that calls
//! this for every scored window lives in [`super::open::run`], since
//! `open`, `assim` and the forecast half of `evolve` all step through that
//! same loop and differ only in whether this ever fires.

use cella_lib::{CellType, Ensemble};

/// After a window is scored, resample/mutate/admit immigrants into the
/// next generation from the just-seen observation via
/// [`Ensemble::assimilate`] — a no-op unless `assim` is `true` (i.e.
/// `mode == "assim"`), so `open` and the post-fit half of `evolve` step
/// through [`super::open::run`]'s loop without ever calling it.
///
/// The `obs_idx + 1 < observed_len` guard skips the very last scored
/// window: there is nothing left to forecast from an assimilation at that
/// point, so it is skipped (the run is about to end). `assim_every`
/// (`SMC_ASSIM_EVERY`) thins how often assimilation happens among the
/// remaining windows.
pub(crate) fn maybe_assimilate(
    ens: &mut Ensemble,
    assim: bool,
    obs: &[bool],
    burnt: &[CellType; 2],
    obs_idx: usize,
    observed_len: usize,
    assim_every: usize,
) {
    if assim && obs_idx + 1 < observed_len && obs_idx.is_multiple_of(assim_every) {
        ens.assimilate(obs, burnt)
            .expect("observation matches the grid");
    }
}
