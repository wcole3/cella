//! [`Sim`]: one type for "a grid I can step", 1D or 2D.
//!
//! The engines in this module do the same things to every member — clone it,
//! reseed it, step it, read its cells, turn a knob — and should not care
//! whether it is a [`Grid1D`] or a [`Grid2D`]. `Sim` is that one type: an
//! enum with a method for each thing the engines need, forwarding to the grid
//! inside. Nothing here is new behaviour; it is the grids' existing API with
//! the dimension folded away.

use std::collections::HashMap;

use lasso2::Spur;

use crate::external::{ExternalModel, ModelError, ParamDesc, ParamValue};
use crate::grid1d::Grid1D;
use crate::grid2d::Grid2D;
use crate::state::GridState;
use crate::types::CellType;

/// A 1D or 2D grid behind one interface.
#[derive(Clone)]
pub enum Sim {
    /// A one-dimensional grid (a row of cells).
    D1(Grid1D),
    /// A two-dimensional grid.
    D2(Grid2D),
}

impl std::fmt::Debug for Sim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Sim::D1(g) => f.debug_tuple("Sim::D1").field(g).finish(),
            Sim::D2(g) => f.debug_tuple("Sim::D2").field(g).finish(),
        }
    }
}

impl From<Grid1D> for Sim {
    fn from(g: Grid1D) -> Self {
        Sim::D1(g)
    }
}

impl From<Grid2D> for Sim {
    fn from(g: Grid2D) -> Self {
        Sim::D2(g)
    }
}

impl Sim {
    /// Advance one step.
    pub fn step(&mut self) {
        match self {
            Sim::D1(g) => g.step(),
            Sim::D2(g) => g.step(),
        }
    }

    /// Advance `n` steps.
    pub fn step_n(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    /// Number of cells.
    pub fn len(&self) -> usize {
        self.cells().len()
    }

    /// True for a grid with no cells.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `(width, height)`; a 1D grid reports height 1.
    pub fn dims(&self) -> (usize, usize) {
        match self {
            Sim::D1(g) => (g.width, 1),
            Sim::D2(g) => (g.width, g.height),
        }
    }

    /// Grid width in cells.
    pub fn width(&self) -> usize {
        self.dims().0
    }

    /// Steps taken so far.
    pub fn step_count(&self) -> u64 {
        match self {
            Sim::D1(g) => g.step,
            Sim::D2(g) => g.step,
        }
    }

    /// Current cell types, row-major.
    pub fn cells(&self) -> &[CellType] {
        match self {
            Sim::D1(g) => g.cells(),
            Sim::D2(g) => g.cells(),
        }
    }

    /// The cells as they were *before* the last step.
    ///
    /// Every step ends by swapping the current and next buffers, so the
    /// buffer that was "next" now holds the previous state for free. Before
    /// the first step there is no previous state and the current cells are
    /// returned. Painting a cell does not update this view; only stepping does.
    pub fn prev_cells(&self) -> &[CellType] {
        if self.step_count() == 0 {
            return self.cells();
        }
        match self {
            Sim::D1(g) => &g.next_cells,
            Sim::D2(g) => &g.next_cells,
        }
    }

    /// Steps each cell has spent in its current type; `0` means it changed
    /// on the last step.
    pub fn ages(&self) -> &[u32] {
        match self {
            Sim::D1(g) => &g.ages,
            Sim::D2(g) => &g.ages,
        }
    }

    /// Population count per type, kept up to date by the stepper.
    pub fn counts(&self) -> &HashMap<Spur, u64> {
        match self {
            Sim::D1(g) => &g.counts_current,
            Sim::D2(g) => &g.counts_current,
        }
    }

    /// The background type.
    pub fn inactive(&self) -> CellType {
        match self {
            Sim::D1(g) => g.inactive,
            Sim::D2(g) => g.inactive,
        }
    }

    /// The seed behind the rule's `randomness` draws.
    pub fn seed(&self) -> u64 {
        match self {
            Sim::D1(g) => g.seed,
            Sim::D2(g) => g.seed,
        }
    }

    /// Reseed the grid, and its model if it has one (see
    /// [`Grid2D::set_seed`]).
    pub fn set_seed(&mut self, seed: u64) {
        match self {
            Sim::D1(g) => g.set_seed(seed),
            Sim::D2(g) => g.set_seed(seed),
        }
    }

    /// Every knob, rule and model alike (see [`crate::tunables`]).
    pub fn params(&self) -> Vec<ParamDesc> {
        match self {
            Sim::D1(g) => g.params(),
            Sim::D2(g) => g.params(),
        }
    }

    /// Current value of one knob.
    pub fn get_param(&self, key: &str) -> Option<ParamValue> {
        match self {
            Sim::D1(g) => g.get_param(key),
            Sim::D2(g) => g.get_param(key),
        }
    }

    /// Set one knob; refusals leave the grid untouched.
    pub fn set_param(&mut self, key: &str, value: ParamValue) -> Result<(), ModelError> {
        match self {
            Sim::D1(g) => g.set_param(key, value),
            Sim::D2(g) => g.set_param(key, value),
        }
    }

    /// The attached model, if this is a 2D grid with one. Drivers use this to
    /// downcast to their own model type.
    pub fn model_mut(&mut self) -> Option<&mut (dyn ExternalModel + 'static)> {
        match self {
            Sim::D1(_) => None,
            Sim::D2(g) => g.model_mut(),
        }
    }

    /// One flag per cell: is the cell in any of `types`?
    pub fn mask(&self, types: &[CellType]) -> Vec<bool> {
        self.cells().iter().map(|c| types.contains(c)).collect()
    }

    /// Replace every cell and start over: ages and history are cleared and the
    /// step counter goes back to 0 (see [`Grid2D::reset_cells`]).
    pub fn reset_cells(&mut self, cells: Vec<CellType>) -> Result<(), ModelError> {
        match self {
            Sim::D1(g) => g.reset_cells(cells),
            Sim::D2(g) => g.reset_cells(cells),
        }
    }

    /// Change the grid's size mid-run; see [`Grid2D::resize`] and
    /// [`Grid1D::resize`]. A 1D grid ignores `height`.
    pub fn resize(&mut self, width: usize, height: usize) -> Result<(), crate::resize::ResizeError> {
        match self {
            Sim::D1(g) => g.resize(width),
            Sim::D2(g) => g.resize(width, height),
        }
    }

    /// Set one cell's type in place — the same move a live paint tool makes
    /// (see [`Grid2D::transition_state_and_buffer`]): its age resets to 0 if
    /// the type actually changed (the same type just ages it by one), the
    /// cell's history is advanced, and an attached model is told via
    /// `on_paint`; nothing else about the grid moves. Unlike
    /// [`Self::reset_cells`] this does **not** touch the step counter,
    /// which is exactly why a [`super::driver::MemberDriver`] rebuilding a
    /// child's grid from an observation
    /// ([`super::driver::MemberDriver::seed_from_observation`]) must use
    /// this instead: the ensemble relies on every member reporting the same
    /// step count, and a driver has no business changing that. Population
    /// counts are not updated here (a live paint tool already leaves them
    /// stale the same way); they catch up at the member's next step.
    /// Fails if `idx` is outside the grid.
    pub fn paint(&mut self, idx: usize, new_type: CellType) -> Result<(), ModelError> {
        let refused = match self {
            Sim::D1(g) => g.transition_state_and_buffer(idx, &new_type),
            Sim::D2(g) => g.transition_state_and_buffer(idx, &new_type),
        };
        match refused {
            None => Ok(()),
            Some(_) => Err(ModelError::InvalidParam(format!(
                "paint: cell {idx} is outside the grid of {} cells",
                self.len()
            ))),
        }
    }

    /// Every type this simulation can show: the ones on the grid now, the
    /// ones its rule can produce or looks for, and the ones its model
    /// declares. Used to check that a type name in a config is real.
    pub fn declared_types(&self) -> Vec<CellType> {
        let mut out: Vec<CellType> = Vec::new();
        let mut push = |t: CellType| {
            if !out.contains(&t) {
                out.push(t);
            }
        };
        push(self.inactive());
        for c in self.cells() {
            push(*c);
        }
        match self {
            Sim::D1(g) => {
                for s in &g.rule.subrules {
                    push(s.current_type);
                    push(s.criteria_type);
                    push(s.output_type);
                }
            }
            Sim::D2(g) => {
                for s in &g.rule.subrules {
                    push(s.current_type);
                    push(s.criteria_type);
                    push(s.output_type);
                }
                if let Some(m) = &g.model {
                    for t in m.declared_types() {
                        push(t);
                    }
                }
            }
        }
        out
    }

    /// Serializable snapshot.
    pub fn to_state(&self) -> GridState {
        match self {
            Sim::D1(g) => GridState::from_grid1d(g),
            Sim::D2(g) => GridState::from_grid2d(g),
        }
    }

    /// Rebuild from a snapshot; `None` if the snapshot is malformed.
    pub fn from_state(state: &GridState) -> Option<Sim> {
        match state {
            GridState::D1 { .. } => Grid1D::from_state(state).map(Sim::D1),
            GridState::D2 { .. } => Grid2D::from_state(state).map(Sim::D2),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{CountOp, Neighborhood2D, Rule1D, Rule1DSubrule, Rule2D, Rule2DSubrule};

    fn blinker() -> Sim {
        let alive = CellType::from("Alive");
        let dead = CellType::inactive();
        let sub = |cur, count, op, limit, out| {
            Rule2DSubrule::new(
                cur,
                alive,
                count,
                op,
                1,
                Neighborhood2D::Moore,
                out,
                None,
                limit,
            )
        };
        let rule = Rule2D {
            subrules: vec![
                sub(alive, 4, CountOp::Gt, None, dead),
                sub(alive, 2, CountOp::Gt, Some(3), alive),
                sub(dead, 3, CountOp::Eq, None, alive),
            ],
        };
        let mut cells = vec![dead; 25];
        for x in 1..4 {
            cells[2 * 5 + x] = alive;
        }
        Sim::D2(Grid2D::new(5, 5, 0, cells, rule))
    }

    fn row() -> Sim {
        let x = CellType::from("X");
        let mk = |cur| Rule1DSubrule {
            current_type: cur,
            criteria_type: x,
            wolfram_code: 30,
            n: 1,
            randomness: None,
            output_type: x,
        };
        let rule = Rule1D {
            subrules: vec![mk(x), mk(CellType::inactive())],
        };
        let mut cells = vec![CellType::inactive(); 9];
        cells[4] = x;
        Sim::D1(Grid1D::new(9, 0, cells, rule))
    }

    #[test]
    fn dims_len_and_stepping_forward_to_the_grid() {
        let mut s = blinker();
        assert_eq!(s.dims(), (5, 5));
        assert_eq!(s.len(), 25);
        assert!(!s.is_empty());
        assert_eq!(s.step_count(), 0);
        s.step_n(3);
        assert_eq!(s.step_count(), 3);
        let mut r = row();
        assert_eq!(r.dims(), (9, 1));
        assert_eq!(r.width(), 9);
        r.step();
        assert_eq!(r.step_count(), 1);
    }

    #[test]
    fn prev_cells_is_the_state_before_the_last_step() {
        let mut s = blinker();
        assert_eq!(
            s.prev_cells(),
            s.cells(),
            "no previous state before stepping"
        );
        let before: Vec<CellType> = s.cells().to_vec();
        s.step();
        assert_eq!(s.prev_cells(), &before[..]);
        assert_ne!(s.prev_cells(), s.cells(), "a blinker changes every step");
        let alive = CellType::from("Alive");
        // Blinker: horizontal bar becomes vertical bar, mass stays 3.
        assert_eq!(s.mask(&[alive]).iter().filter(|b| **b).count(), 3);
        assert_eq!(
            s.ages().iter().filter(|a| **a == 0).count(),
            4,
            "two cells died, two were born"
        );
        let mut r = row();
        let before: Vec<CellType> = r.cells().to_vec();
        r.step();
        assert_eq!(r.prev_cells(), &before[..]);
    }

    #[test]
    fn counts_mask_inactive_and_declared_types() {
        let s = blinker();
        let alive = CellType::from("Alive");
        assert_eq!(s.counts().get(&alive.0), Some(&3));
        assert_eq!(s.inactive(), CellType::inactive());
        let m = s.mask(&[alive]);
        assert_eq!(m.iter().filter(|b| **b).count(), 3);
        let declared = s.declared_types();
        assert!(declared.contains(&alive) && declared.contains(&CellType::inactive()));
        assert_eq!(declared.len(), 2);
        let r = row();
        assert!(r.declared_types().contains(&CellType::from("X")));
    }

    #[test]
    fn seed_params_and_reset_round_trip() {
        let mut s = row();
        assert_eq!(s.seed(), 0);
        s.set_seed(9);
        assert_eq!(s.seed(), 9);
        assert_eq!(s.params().len(), 2);
        assert_eq!(
            s.get_param("rule.subrules[0].wolfram_code"),
            Some(ParamValue::Bits(30))
        );
        s.set_param("rule.subrules[0].wolfram_code", ParamValue::Bits(90))
            .unwrap();
        assert_eq!(
            s.get_param("rule.subrules[0].wolfram_code"),
            Some(ParamValue::Bits(90))
        );
        assert!(s.set_param("model.p0", ParamValue::Float(0.1)).is_err());
        assert!(s.model_mut().is_none(), "1D grids carry no model");
        s.step_n(2);
        let x = CellType::from("X");
        s.reset_cells(vec![x; 9]).unwrap();
        assert_eq!(s.step_count(), 0);
        assert_eq!(s.counts().get(&x.0), Some(&9));
        assert!(s.reset_cells(vec![x; 3]).is_err());

        let mut b = blinker();
        b.step();
        let state = b.to_state();
        let back = Sim::from_state(&state).unwrap();
        assert_eq!(back.cells(), b.cells());
        assert_eq!(back.step_count(), 1);
        assert!(b.model_mut().is_none());
        let state1 = row().to_state();
        assert!(matches!(Sim::from_state(&state1), Some(Sim::D1(_))));
    }

    #[test]
    fn paint_changes_one_cell_without_touching_the_step_counter() {
        let mut s = blinker();
        s.step_n(3);
        let step_before = s.step_count();
        let alive = CellType::from("Alive");
        let dead = CellType::inactive();
        assert_eq!(s.ages()[0], 3, "an untouched cell ages every step");
        s.paint(0, alive).unwrap();
        assert_eq!(s.cells()[0], alive);
        assert_eq!(s.ages()[0], 0, "a real type change resets the cell's age");
        assert_eq!(
            s.step_count(),
            step_before,
            "paint must not move the step counter — a driver seeding one \
             immigrant must never desync it from the rest of the ensemble"
        );
        // Painting the same type again just ages it, like a step would.
        s.paint(0, alive).unwrap();
        assert_eq!(s.ages()[0], 1);
        s.paint(0, dead).unwrap();
        assert_eq!(s.ages()[0], 0);
        assert!(s.paint(999, alive).is_err(), "out-of-bounds is refused");

        // The 1D grid takes the same path.
        let mut r = row();
        let x = CellType::from("X");
        r.paint(0, x).unwrap();
        assert_eq!(r.cells()[0], x);
        assert!(r.paint(999, x).is_err());
    }

    #[test]
    fn resize_forwards_to_the_grid_inside_and_1d_ignores_height() {
        let mut r = row();
        r.resize(5, 99).unwrap();
        assert_eq!(r.dims(), (5, 1));

        let mut s = blinker();
        s.resize(3, 4).unwrap();
        assert_eq!(s.dims(), (3, 4));
    }
}
