//! Helpers shared by the integration tests in this folder.

// Each test file compiles its own copy of this module and uses only some helpers.
#![allow(dead_code)]

use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
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

/// A fresh flat-ground world with the repo's car in `mode`.
pub fn new_world_in(mode: ModeParams) -> World {
    let config = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    World::new_on_flat_ground(&config, mode, DEFAULT_GROUND_FRICTION)
}

/// A fresh flat-ground world with the repo's car in Realistic mode.
pub fn new_world() -> World {
    new_world_in(realistic())
}
