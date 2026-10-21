//! Passenger g-load (SPEC §9.4): about 1 g sitting on the wheels, about 0 in free fall,
//! and well above 1 g while the lift nozzles brake a fall.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, new_world};
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::input::InputFrame;
use nl_sim::world::World;

fn run(world: &mut World, throttles: [f32; NOZZLE_COUNT], steps: usize) {
    for _ in 0..steps {
        world.set_controller_throttles(throttles);
        world.step(InputFrame::default());
    }
}

#[test]
fn a_parked_car_feels_one_g() {
    let mut world = new_world();

    run(&mut world, [0.0; NOZZLE_COUNT], 240);

    assert!((world.g_load() - 1.0).abs() < 0.05, "{} g", world.g_load());
}

#[test]
fn a_falling_car_feels_almost_nothing_and_braking_feels_heavy() {
    let mut world = new_world();
    world
        .place_car([0.0, 40.0, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();

    run(&mut world, [0.0; NOZZLE_COUNT], 60);
    let falling_g = world.g_load();
    let mut lift = [0.0; NOZZLE_COUNT];
    lift[..4].fill(1.0);
    run(&mut world, lift, 60);
    let braking_g = world.g_load();

    assert!(falling_g < 0.1, "falling {falling_g} g");
    assert!(braking_g > 1.5, "braking {braking_g} g");
}
