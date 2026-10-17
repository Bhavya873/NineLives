//! Helpers shared by the integration tests in this folder.

use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// A fresh flat-ground world with the repo's car.
pub fn new_world() -> World {
    let config = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    World::new_on_flat_ground(&config, DEFAULT_GROUND_FRICTION)
}
