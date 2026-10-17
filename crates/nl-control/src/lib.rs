//! Controllers: whatever produces the 10 nozzle throttles each control step (SPEC §8).
//!
//! Each controller implements `Controller`. The game and the eval runner call `act` at
//! 60 Hz and hand the throttles to `World::set_controller_throttles`: `Off`, the debug
//! `Manual`, and the `Pd` baseline with its thruster allocation. The AI joins in M5–M6.

use nl_sim::car_state::CarState;
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::input::InputFrame;

pub mod allocation;
pub mod config;
mod controller_kind;
pub mod eval;
mod manual;
mod off;
mod pd;

pub use controller_kind::{CONTROLLER_NAMES, ControllerKind, UnknownController};
pub use manual::Manual;
pub use off::Off;
pub use pd::Pd;

/// What a controller may look at on a control step. The M5 observation (`ObsSpec`) is
/// built from the same `state`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ControlCtx {
    /// The driver's input this step; `Manual` reads its nozzle buttons.
    pub input: InputFrame,
    /// The car's physical state (`World::car_state`); `Pd` reads it.
    pub state: CarState,
}

/// Turns what the controller sees into 10 throttles in [0, 1], in nozzle order.
pub trait Controller {
    /// Forgets any state, e.g. at the start of an episode. `seed` is for controllers
    /// that use randomness; none do yet.
    fn reset(&mut self, seed: u64);

    /// The throttles for this control step.
    fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT];
}
