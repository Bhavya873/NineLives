//! Thruster signs in a running sim (SPEC §7.4, §12; golden rule 5). Each nozzle fires alone
//! on a car hanging level, on its side, upside down and nose-down; the car must be pushed
//! along the nozzle's world direction and turn with the sign of its body torque, which
//! thruster_signs.rs pins to the SPEC §7.4 table. This checks the body→world rotation.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, RIGHT_SIDE_DOWN, UPSIDE_DOWN, arcade, new_world, realistic, vehicle};
use nl_sim::conventions::{GRAVITY_M_S2, NOZZLE_COUNT, PHYSICS_HZ};
use nl_sim::thrusters::{nozzle_force_and_torque_at_full_throttle, valve_step_fraction};
use nl_sim::world::World;
use rapier3d::prelude::{Rotation, Vector};

/// How long each nozzle fires: long enough for the valve to open, short enough that the
/// car barely turns, so the push direction is still the starting one.
const FIRE_STEPS: usize = PHYSICS_HZ as usize / 20;
/// Height the car hangs at, far above the ground.
const START_HEIGHT_M: f32 = 30.0;
/// A torque smaller than this (N·m) in the SPEC table counts as zero for that axis.
const NONZERO_TORQUE_N_M: f32 = 500.0;
/// The push must point within about 18° of the nozzle's world direction.
const MIN_DIRECTION_MATCH: f32 = 0.95;

/// One attitude to test, as a unit quaternion [x, y, z, w] and a name for messages.
struct Attitude {
    name: &'static str,
    rotation: [f32; 4],
}

fn attitudes() -> [Attitude; 4] {
    let half = std::f32::consts::FRAC_1_SQRT_2;
    [
        Attitude {
            name: "level",
            rotation: LEVEL,
        },
        Attitude {
            name: "on its right side",
            rotation: RIGHT_SIDE_DOWN,
        },
        Attitude {
            name: "upside down",
            rotation: UPSIDE_DOWN,
        },
        Attitude {
            name: "nose-down 90°",
            rotation: [0.0, 0.0, -half, half],
        },
    ]
}

/// Turns body-frame vector `v` into the world frame with unit quaternion `q` [x, y, z, w].
fn to_world(q: [f32; 4], v: [f32; 3]) -> Vector {
    let [x, y, z, w] = q;
    Rotation::from_xyzw(x, y, z, w) * Vector::from(v)
}

/// A still car hanging at `START_HEIGHT_M` in `rotation`.
fn hanging_car(rotation: [f32; 4]) -> World {
    let mut world = new_world();
    world
        .place_car([0.0, START_HEIGHT_M, 0.0], rotation, [0.0; 3], [0.0; 3])
        .unwrap();
    world
}

/// Fires only `nozzle` at full throttle for `FIRE_STEPS` physics steps.
fn fire_alone(world: &mut World, nozzle: usize) {
    let mut throttles = [0.0; NOZZLE_COUNT];
    throttles[nozzle] = 1.0;
    for _ in 0..FIRE_STEPS {
        world.set_controller_throttles(throttles);
        world.step(Default::default());
    }
}

/// The velocity the thrust alone added: the measured velocity minus what gravity added.
fn velocity_from_thrust_m_s(world: &World) -> [f32; 3] {
    let elapsed_s = FIRE_STEPS as f32 / PHYSICS_HZ as f32;
    let [vx, vy, vz] = world.linear_velocity_m_s();
    [vx, vy + GRAVITY_M_S2 * elapsed_s, vz]
}

#[test]
fn every_nozzle_pushes_along_its_direction_in_every_attitude() {
    let config = vehicle();
    for attitude in attitudes() {
        for (index, nozzle) in config.thrusters.nozzles.iter().enumerate() {
            let mut world = hanging_car(attitude.rotation);

            fire_alone(&mut world, index);

            let pushed = Vector::from(velocity_from_thrust_m_s(&world));
            let expected = to_world(attitude.rotation, nozzle.direction);
            let match_cos = pushed.normalize().dot(expected.normalize());
            assert!(
                match_cos > MIN_DIRECTION_MATCH,
                "{} {}: pushed {pushed:?}, expected along {expected:?} (cos {match_cos})",
                attitude.name,
                nozzle.name,
            );
        }
    }
}

#[test]
fn every_nozzle_turns_the_car_the_way_the_torque_table_says_in_every_attitude() {
    let config = vehicle();
    for attitude in attitudes() {
        for (index, nozzle) in config.thrusters.nozzles.iter().enumerate() {
            let mut world = hanging_car(attitude.rotation);
            let (_force_n, torque_n_m) = nozzle_force_and_torque_at_full_throttle(nozzle);

            fire_alone(&mut world, index);

            // Body axes: X roll, Y yaw, Z pitch (SPEC §4); the inertia is diagonal, so
            // each body rate starts with the sign of its torque.
            let rate_body = world.angular_velocity_body_rad_s();
            for axis in 0..3 {
                if torque_n_m[axis].abs() < NONZERO_TORQUE_N_M {
                    continue;
                }
                assert!(
                    rate_body[axis] * torque_n_m[axis] > 0.0,
                    "{} {}: body axis {axis} turns at {} rad/s, torque {} N·m",
                    attitude.name,
                    nozzle.name,
                    rate_body[axis],
                    torque_n_m[axis],
                );
            }
        }
    }
}

#[test]
fn arcade_mode_pushes_by_its_thrust_scale() {
    let boost_left = 8; // B_L: horizontal, so gravity does not mix in
    let mut realistic_world = hanging_car(LEVEL);
    realistic_world.set_mode(realistic());
    let mut arcade_world = hanging_car(LEVEL);
    arcade_world.set_mode(arcade());

    fire_alone(&mut realistic_world, boost_left);
    fire_alone(&mut arcade_world, boost_left);

    let ratio = Vector::from(velocity_from_thrust_m_s(&arcade_world)).length()
        / Vector::from(velocity_from_thrust_m_s(&realistic_world)).length();
    let expected = arcade().thrust_scale / realistic().thrust_scale;
    assert!(
        (ratio - expected).abs() < 0.02 * expected,
        "arcade/realistic push ratio {ratio}, expected {expected}"
    );
}

#[test]
fn a_valve_opens_by_one_lag_step_in_the_running_world() {
    let config = vehicle();
    let mut world = hanging_car(LEVEL);
    let mut throttles = [0.0; NOZZLE_COUNT];
    throttles[0] = 1.0;

    world.set_controller_throttles(throttles);
    world.step(Default::default());

    let dt_s = 1.0 / PHYSICS_HZ as f32;
    let expected = valve_step_fraction(dt_s, config.thrusters.valve_time_constant_s);
    let opened = world.effective_throttles()[0];
    assert!(
        (opened - expected).abs() < 1e-4,
        "valve opened to {opened}, expected {expected}"
    );
}
