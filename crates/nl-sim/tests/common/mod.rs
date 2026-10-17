//! Helpers shared by the integration tests in this folder.

// Each test file compiles its own copy of this module and uses only some helpers.
#![allow(dead_code)]

use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::events::SimEvent;
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
use nl_sim::reward::{DEFAULT_REWARD_TOML, RewardConfig};
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// The repo's Realistic mode.
pub fn realistic() -> ModeParams {
    ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
        .unwrap()
        .realistic
}

/// The repo's Arcade mode.
pub fn arcade() -> ModeParams {
    ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
        .unwrap()
        .arcade
}

/// The repo's car.
pub fn vehicle() -> VehicleConfig {
    VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap()
}

/// The repo's scenarios.
pub fn scenarios() -> ScenariosConfig {
    ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML).unwrap()
}

/// The repo's reward weights.
pub fn reward() -> RewardConfig {
    RewardConfig::from_toml_str(DEFAULT_REWARD_TOML).unwrap()
}

/// A fresh flat-ground world with the repo's car in `mode`.
pub fn new_world_in(mode: ModeParams) -> World {
    let seed = 0;
    World::new_on_flat_ground(&vehicle(), mode, DEFAULT_GROUND_FRICTION, seed)
}

/// A fresh flat-ground world with the repo's car in Realistic mode.
pub fn new_world() -> World {
    new_world_in(realistic())
}

/// No rotation: the car upright, facing world +X (quaternion x, y, z, w).
pub const LEVEL: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// Rolled 180° about the forward axis (+X): upside down, roof towards the ground.
pub const UPSIDE_DOWN: [f32; 4] = [1.0, 0.0, 0.0, 0.0];

/// Rolled +90° about the forward axis (+X): lying on its right side (+Z down).
pub const RIGHT_SIDE_DOWN: [f32; 4] = [
    std::f32::consts::FRAC_1_SQRT_2,
    0.0,
    0.0,
    std::f32::consts::FRAC_1_SQRT_2,
];

/// An input frame with only these buttons pressed.
pub fn buttons(buttons: u16) -> InputFrame {
    InputFrame {
        buttons,
        ..InputFrame::default()
    }
}

/// Steps `seconds` (rounded to whole steps) with no input and returns every event.
pub fn run_and_collect(world: &mut World, seconds: f32) -> Vec<SimEvent> {
    let mut events = Vec::new();
    for _ in 0..(seconds * PHYSICS_HZ as f32).round() as usize {
        world.step(InputFrame::default());
        events.extend(world.drain_events());
    }
    events
}
