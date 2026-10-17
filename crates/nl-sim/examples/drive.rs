//! Drives the car for 10 seconds with a scripted input and prints its trajectory.
//!
//! Run with: `cargo run -p nl-sim --example drive`

use std::error::Error;

use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::golden_trace::scripted_input;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

const DRIVE_SECONDS: u32 = 10;
/// Print a row every 30 physics steps, which is 4 times a second at 120 Hz.
const STEPS_BETWEEN_PRINTS: u32 = 30;

fn main() -> Result<(), Box<dyn Error>> {
    let config = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML)?;
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)?;
    let seed = 0;
    let mut world =
        World::new_on_flat_ground(&config, modes.realistic, DEFAULT_GROUND_FRICTION, seed);

    println!("  time_s      x_m      y_m      z_m   speed_m_s  wheels");
    for step in 0..DRIVE_SECONDS * PHYSICS_HZ {
        world.step(scripted_input(step));
        if (step + 1) % STEPS_BETWEEN_PRINTS == 0 {
            print_row(&world);
        }
    }
    Ok(())
}

fn print_row(world: &World) {
    let [x_m, y_m, z_m] = world.chassis_pose().position_m;
    let wheels_down = world
        .wheels_touching()
        .iter()
        .filter(|touching| **touching)
        .count();
    println!(
        "{:8.2} {:8.2} {:8.2} {:8.2} {:11.2} {:7}",
        world.time_s(),
        x_m,
        y_m,
        z_m,
        world.forward_speed_m_s(),
        wheels_down
    );
}
