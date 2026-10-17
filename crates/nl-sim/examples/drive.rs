//! Drives the car for 10 seconds with a scripted input and prints its trajectory.
//!
//! Run with: `cargo run -p nl-sim --example drive`

use std::error::Error;

use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::golden_trace::scripted_input;
use nl_sim::vehicle_config::VehicleConfig;
use nl_sim::world::World;

const VEHICLE_TOML: &str = include_str!("../../../configs/vehicle.toml");
const DRIVE_SECONDS: u32 = 10;
const PRINTS_PER_SECOND: u32 = 4;

fn main() -> Result<(), Box<dyn Error>> {
    let config = VehicleConfig::from_toml_str(VEHICLE_TOML)?;
    let mut world = World::new_on_flat_ground(&config, 1.0);
    let steps_between_prints = PHYSICS_HZ / PRINTS_PER_SECOND;

    println!("  time_s      x_m      y_m      z_m   speed_m_s  wheels");
    for step in 0..DRIVE_SECONDS * PHYSICS_HZ {
        world.step(scripted_input(step));
        if (step + 1) % steps_between_prints == 0 {
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
