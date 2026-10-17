//! Sign tests for the domain-randomization physics (SPEC §9.8), written before the code
//! (CLAUDE.md golden rule 5): wind pushes the car the way it blows, and a centre of mass
//! moved forward makes equal lift pitch the nose down.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, realistic, vehicle};
use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::input::InputFrame;
use nl_sim::vehicle_config::VehicleConfig;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// `config`'s car 10 m up, level and still.
fn airborne(config: &VehicleConfig) -> World {
    let mut world = World::new_on_flat_ground(config, realistic(), DEFAULT_GROUND_FRICTION, 0);
    world
        .place_car([0.0, 10.0, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
    world
}

fn run(world: &mut World, seconds: f32) {
    for _ in 0..(seconds * PHYSICS_HZ as f32).round() as usize {
        world.step(InputFrame::default());
    }
}

#[test]
fn wind_from_behind_pushes_a_still_car_forward() {
    let mut config = vehicle();
    config.air_drag.wind_m_s = [10.0, 0.0, 0.0];
    let mut world = airborne(&config);

    run(&mut world, 1.0);

    // 0.5 · 1.2 · 0.6 · 10² ≈ 36 N on 1800 kg: about 0.02 m/s after 1 s.
    assert!(world.linear_velocity_m_s()[0] > 0.01);
}

#[test]
fn no_wind_leaves_a_still_car_without_sideways_push() {
    let mut world = airborne(&vehicle());

    run(&mut world, 0.2);

    let [x, _, z] = world.linear_velocity_m_s();
    assert!(x.abs() < 1e-4 && z.abs() < 1e-4);
}

#[test]
fn a_centre_of_mass_moved_forward_makes_equal_lift_pitch_the_nose_down() {
    let mut config = vehicle();
    config.chassis.center_of_mass_offset_m = [0.1, 0.0, 0.0];
    let mut world = airborne(&config);
    let mut throttles = [0.0; 10];
    throttles[..4].fill(1.0);

    world.set_controller_throttles(throttles);
    run(&mut world, 0.1);

    // Pitch rate about +Z: positive is nose up (SPEC §4).
    assert!(world.angular_velocity_body_rad_s()[2] < -0.01);
}
