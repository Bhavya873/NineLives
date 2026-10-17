//! Controllers: whatever produces the 10 nozzle throttles each control step (SPEC §8).
//!
//! Each controller implements `Controller`. The game and the eval runner call `act` at
//! 60 Hz and hand the throttles to `World::set_controller_throttles`. So far: `Off` and
//! the debug `Manual`; M4 adds `Pd` and the thruster allocation, and the observation.

use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::input::InputFrame;

mod controller_kind;
mod manual;
mod off;

pub use controller_kind::{CONTROLLER_NAMES, ControllerKind, UnknownController};
pub use manual::Manual;
pub use off::Off;

/// What a controller may look at on a control step, besides the observation (which
/// arrives with `ObsSpec` in M4–M5).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ControlCtx {
    /// The driver's input this step; `Manual` reads its nozzle buttons.
    pub input: InputFrame,
}

/// Turns what the controller sees into 10 throttles in [0, 1], in nozzle order.
pub trait Controller {
    /// Forgets any state, e.g. at the start of an episode. `seed` is for controllers
    /// that use randomness; none do yet.
    fn reset(&mut self, seed: u64);

    /// The throttles for this control step.
    fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT];
}
