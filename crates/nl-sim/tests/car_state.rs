//! Sign tests for what a controller sees (SPEC §4 frames, §9.2): gravity and spin in the
//! body frame, the height ray, and the car model.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, arcade, new_world, new_world_in, run_and_collect};
use nl_sim::car_state::MAX_HEIGHT_M;

/// Rolled +90° about the body's forward axis (+X): the right side (+Z) faces down.
const ROLLED_RIGHT_SIDE_DOWN: [f32; 4] = [
    std::f32::consts::FRAC_1_SQRT_2,
    0.0,
    0.0,
    std::f32::consts::FRAC_1_SQRT_2,
];

fn assert_close(actual: [f32; 3], expected: [f32; 3]) {
    for axis in 0..3 {
        assert!(
            (actual[axis] - expected[axis]).abs() < 1e-4,
            "{actual:?} vs {expected:?}"
        );
    }
}

#[test]
fn a_level_car_feels_gravity_straight_down_its_own_minus_y() {
    let world = new_world();

    let state = world.car_state();

    assert_close(state.gravity_dir_body, [0.0, -1.0, 0.0]);
    assert!((state.up_alignment - 1.0).abs() < 1e-4);
}

#[test]
fn rolling_plus_90_degrees_about_forward_puts_gravity_on_the_right_plus_z() {
    let mut world = new_world();
    world
        .place_car([0.0, 10.0, 0.0], ROLLED_RIGHT_SIDE_DOWN, [0.0; 3], [0.0; 3])
        .unwrap();

    let state = world.car_state();

    assert_close(state.gravity_dir_body, [0.0, 0.0, 1.0]);
    assert!(state.up_alignment.abs() < 1e-4);
}

#[test]
fn spinning_about_world_up_on_a_rolled_car_shows_as_minus_body_z() {
    let mut world = new_world();
    // Rolled right side down, the body's -Z axis points up, so a world-up spin is a
    // spin about body -Z (pitch) in SPEC §4 signs.
    world
        .place_car(
            [0.0, 10.0, 0.0],
            ROLLED_RIGHT_SIDE_DOWN,
            [0.0; 3],
            [0.0, 2.0, 0.0],
        )
        .unwrap();

    let state = world.car_state();

    assert_close(state.angular_velocity_body_rad_s, [0.0, 0.0, -2.0]);
}

#[test]
fn the_height_ray_measures_down_to_the_ground_and_is_capped() {
    let mut world = new_world();
    world
        .place_car([0.0, 5.0, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
    assert!((world.car_state().height_m - 5.0).abs() < 1e-3);

    world
        .place_car([0.0, 80.0, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();

    assert_eq!(world.car_state().height_m, MAX_HEIGHT_M);
}

#[test]
fn a_settled_car_has_all_wheels_down_and_arcade_doubles_thrust() {
    let mut world = new_world_in(arcade());
    run_and_collect(&mut world, 1.5);

    let state = world.car_state();

    assert_eq!(state.wheels_down, 4);
    assert!(
        (state.thrust_scale - 2.0).abs() < 1e-6,
        "{}",
        state.thrust_scale
    );
}

#[test]
fn the_car_model_has_positive_mass_and_inertia_and_lift_pushes_up() {
    let world = new_world();

    let model = world.car_model();

    assert!(model.mass_kg > 1000.0, "{}", model.mass_kg);
    assert!(
        model
            .principal_inertia_kg_m2
            .iter()
            .all(|moment| *moment > 0.0)
    );
    // L_FL (nozzle 0) is a lift nozzle: it pushes straight up the body's +Y.
    let lift = model.nozzle_wrenches[0];
    assert!(
        lift[1] > 0.0 && lift[0] == 0.0 && lift[2] == 0.0,
        "{lift:?}"
    );
}
