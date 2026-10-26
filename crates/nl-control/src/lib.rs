//! Controllers: whatever produces the 10 nozzle throttles each control step (SPEC §8).
//!
//! Each controller implements `Controller`. The game and the eval runner call `act` at
//! 60 Hz and hand the throttles to `World::set_controller_throttles`: `Off`, the debug
//! `Manual`, the `Pd` baseline with its thruster allocation, `Ai`, the trained policy, and
//! `Hybrid`, where the AI flies and PD drives.

use nl_sim::actuator_model::ActuatorState;
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
mod hybrid;
mod manual;
mod off;
mod pd;
mod residual;

pub use ai::Ai;
pub use controller_kind::{
    BuildControllerError, CONTROLLER_NAMES, CYCLE_NAMES, ControllerChoiceError, ControllerChoices,
    ControllerKind, build_controller,
};
pub use hybrid::{Hybrid, HybridGate, HybridGateSettings, HybridSwitch};
pub use manual::Manual;
pub use off::Off;
pub use pd::Pd;
pub use residual::{
    RESIDUAL_ACTION_COUNT, ResidualPd, ResidualSettings, add_residual, gate_opening,
};

/// What a controller may look at on a control step (SPEC §8).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlCtx {
    /// The driver's input this step; `Manual` reads its nozzle buttons.
    pub input: InputFrame,
    /// The car's physical state (`World::car_state`); `Pd` reads it.
    pub state: CarState,
    /// The observation built from that state (SPEC §9.2); `Ai` reads it.
    pub observation: Observation,
    /// The thrusters' own state, for the actuator-aware allocation; None where the
    /// next command is not what the valves get (latency) or nobody filled it in.
    pub actuators: Option<ActuatorState>,
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
            actuators: Some(world.actuator_state()),
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
            actuators: episode.actuator_state(),
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
            actuators: None,
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
