//! Thruster allocation tests (SPEC §8): the allocation turns a wanted push and turn
//! (a wrench) into throttles in 0..1 that make it. Thrust is one-sided, so each test
//! checks the allocation finds the nozzles that can, and shuts every valve when none can.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_control::allocation::Allocator;
use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};
use nl_sim::car_state::CarModel;
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// Control steps the warm-started allocation gets, as over a few steps in the game.
const WARM_START_STEPS: usize = 10;

fn controllers() -> ControllersConfig {
    ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap()
}

fn car_model() -> CarModel {
    let config = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML).unwrap();
    World::new_on_flat_ground(&config, modes.realistic, DEFAULT_GROUND_FRICTION, 0).car_model()
}

/// The push the throttles make: B·u, with B's columns the nozzle wrenches.
fn produced_wrench(model: &CarModel, throttles: &[f32; NOZZLE_COUNT]) -> [f32; 6] {
    let mut wrench = [0.0; 6];
    for (nozzle, throttle) in throttles.iter().enumerate() {
        for (total, push) in wrench.iter_mut().zip(model.nozzle_wrenches[nozzle]) {
            *total += push * throttle;
        }
    }
    wrench
}

/// Solves for `wanted` over `WARM_START_STEPS` control steps, each starting from the last.
fn solve_warm(allocator: &Allocator, wanted: [f32; 6], throttles: &mut [f32; NOZZLE_COUNT]) {
    for _ in 0..WARM_START_STEPS {
        allocator.solve(wanted, 1.0, throttles);
    }
}

#[test]
fn the_allocation_lifts_with_the_four_lift_nozzles_and_stays_in_bounds() {
    let model = car_model();
    let allocator = Allocator::new(&model.nozzle_wrenches, &controllers().allocation);
    let wanted = [0.0, 20000.0, 0.0, 0.0, 0.0, 0.0];
    let mut throttles = [0.0; NOZZLE_COUNT];

    solve_warm(&allocator, wanted, &mut throttles);

    assert!(
        throttles
            .iter()
            .all(|throttle| (0.0..=1.0).contains(throttle))
    );
    let lift = produced_wrench(&model, &throttles)[1];
    assert!(
        (lift - 20000.0).abs() < 1000.0,
        "lift {lift} N, throttles {throttles:?}"
    );
    assert!(
        throttles[4..].iter().all(|throttle| *throttle < 0.05),
        "{throttles:?}"
    );
}

#[test]
fn one_control_step_from_all_valves_shut_already_gives_most_of_a_braking_push() {
    // Braking starts from shut valves (PD is off until the car leaves the ground),
    // and the brake window lasts only a few control steps.
    let model = car_model();
    let allocator = Allocator::new(&model.nozzle_wrenches, &controllers().allocation);
    let brake_n = 1.5 * model.mass_kg * 9.81;
    let mut throttles = [0.0; NOZZLE_COUNT];

    allocator.solve([0.0, brake_n, 0.0, 0.0, 0.0, 0.0], 1.0, &mut throttles);

    let lift = produced_wrench(&model, &throttles)[1];
    assert!(lift > 0.9 * brake_n, "{lift} N of {brake_n} N");
}

#[test]
fn a_push_no_nozzle_can_make_leaves_every_valve_shut() {
    let model = car_model();
    let allocator = Allocator::new(&model.nozzle_wrenches, &controllers().allocation);
    // Straight down: thrust is one-sided, nothing pushes the car down.
    let mut throttles = [0.0; NOZZLE_COUNT];

    allocator.solve([0.0, -20000.0, 0.0, 0.0, 0.0, 0.0], 1.0, &mut throttles);

    assert_eq!(throttles, [0.0; NOZZLE_COUNT]);
}

#[test]
fn the_allocation_turns_the_car_both_ways_about_every_axis() {
    // Thrust is one-sided, so every turn must come from a different set of nozzles.
    let model = car_model();
    let allocator = Allocator::new(&model.nozzle_wrenches, &controllers().allocation);
    let wanted_torque_n_m = 4000.0;
    // Wrench layout: force x, y, z, then torque about roll (3), yaw (4) and pitch (5).
    for axis in 3..6 {
        for sign in [1.0_f32, -1.0] {
            let mut wanted = [0.0; 6];
            wanted[axis] = sign * wanted_torque_n_m;
            let mut throttles = [0.0; NOZZLE_COUNT];

            solve_warm(&allocator, wanted, &mut throttles);

            let produced = produced_wrench(&model, &throttles)[axis];
            assert!(
                produced * sign >= 0.5 * wanted_torque_n_m,
                "axis {axis} sign {sign}: produced {produced} N·m, throttles {throttles:?}"
            );
        }
    }
}

#[test]
fn a_pure_yaw_comes_from_side_nozzles_whose_pushes_cancel() {
    let model = car_model();
    let allocator = Allocator::new(&model.nozzle_wrenches, &controllers().allocation);
    let mut throttles = [0.0; NOZZLE_COUNT];

    solve_warm(
        &allocator,
        [0.0, 0.0, 0.0, 0.0, 3000.0, 0.0],
        &mut throttles,
    );

    let wrench = produced_wrench(&model, &throttles);
    let net_force_n =
        (wrench[0] * wrench[0] + wrench[1] * wrench[1] + wrench[2] * wrench[2]).sqrt();
    assert!(wrench[4] > 1500.0, "yaw {} N·m", wrench[4]);
    assert!(
        net_force_n < 1500.0,
        "net force {net_force_n} N, {wrench:?}"
    );
}
