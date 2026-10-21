//! Controllers: whatever produces the 10 nozzle throttles each control step (SPEC §8).
//!
//! Each controller implements `Controller`. The game and the eval runner call `act` at
//! 60 Hz and hand the throttles to `World::set_controller_throttles`: `Off`, the debug
//! `Manual`, the `Pd` baseline with its thruster allocation, and `Ai`, the trained policy.

use nl_sim::car_state::CarState;
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::episode::Episode;
use nl_sim::input::InputFrame;
use nl_sim::obs::{OBS_DIM, Observation, observe};
use nl_sim::world::World;

mod ai;
pub mod allocation;
pub mod config;
mod controller_kind;
pub mod eval;
mod manual;
mod off;
mod pd;

pub use ai::Ai;
pub use controller_kind::{
    BuildControllerError, CONTROLLER_NAMES, CYCLE_NAMES, ControllerChoiceError, ControllerChoices,
    ControllerKind, build_controller,
};
pub use manual::Manual;
pub use off::Off;
pub use pd::Pd;

/// What a controller may look at on a control step (SPEC §8).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlCtx {
    /// The driver's input this step; `Manual` reads its nozzle buttons.
    pub input: InputFrame,
    /// The car's physical state (`World::car_state`); `Pd` reads it.
    pub state: CarState,
    /// The observation built from that state (SPEC §9.2); `Ai` reads it.
    pub observation: Observation,
}

impl ControlCtx {
    /// What a controller sees of `world` now, with the driver's `input`: the game's
    /// context (no latency, no sensor noise). Episodes use `for_episode` instead.
    pub fn for_world(world: &World, input: InputFrame) -> ControlCtx {
        let state = world.car_state();
        ControlCtx {
            input,
            state,
            observation: observe(world, &state),
        }
    }
}

impl ControlCtx {
    /// What a controller sees of a running episode: the state and observation with the
    /// episode's sensor noise, and `prev_action` as last commanded (SPEC §9.8). Eval and
    /// training use this, so PD and the AI see the same noisy car.
    pub fn for_episode(episode: &mut Episode, input: InputFrame) -> ControlCtx {
        let (state, observation) = episode.controller_view();
        ControlCtx {
            input,
            state,
            observation,
        }
    }
}

impl Default for ControlCtx {
    /// No input, a car at rest on its wheels and an all-zero observation (for tests).
    fn default() -> ControlCtx {
        ControlCtx {
            input: InputFrame::default(),
            state: CarState::default(),
            observation: [0.0; OBS_DIM],
        }
    }
}

/// Turns what the controller sees into 10 throttles in [0, 1], in nozzle order.
pub trait Controller {
    /// Forgets any state, e.g. at the start of an episode. `seed` is for controllers
    /// that use randomness; none do yet.
    fn reset(&mut self, seed: u64);

    /// The throttles for this control step.
    fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT];
}
