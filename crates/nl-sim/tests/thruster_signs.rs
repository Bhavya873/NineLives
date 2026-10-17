//! Thruster sign tests (SPEC §7.4, §12), written before the thruster code
//! (CLAUDE.md golden rule 5). Each nozzle's torque at full force must match the
//! table in SPEC §7.4, and firing a nozzle must turn the car the right way.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_sim::conventions::{NOZZLE_COUNT, NOZZLE_NAMES, PHYSICS_HZ};
use nl_sim::thrusters::nozzle_force_and_torque_at_full_throttle;
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// SPEC §7.4 torque table in kN·m as (roll X, yaw Y, pitch Z), in nozzle order.
const EXPECTED_TORQUE_KN_M: [[f32; 3]; NOZZLE_COUNT] = [
    [10.2, 0.0, 19.2],
    [-10.2, 0.0, 19.2],
    [10.2, 0.0, -19.2],
    [-10.2, 0.0, -19.2],
    [2.0, -5.6, 0.0],
    [2.0, 5.6, 0.0],
    [-2.0, 5.6, 0.0],
    [-2.0, -5.6, 0.0],
    [0.0, -3.2, 0.0],
    [0.0, 3.2, 0.0],
];

fn config() -> VehicleConfig {
    VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap()
}

#[test]
fn nozzles_are_listed_in_the_spec_order() {
    let config = config();

    let names: Vec<&str> = config
        .thrusters
        .nozzles
        .iter()
        .map(|nozzle| nozzle.name.as_str())
        .collect();

    assert_eq!(names, NOZZLE_NAMES);
}

#[test]
fn every_nozzle_torque_matches_the_spec_table() {
    let config = config();

    for (index, nozzle) in config.thrusters.nozzles.iter().enumerate() {
        let (_force_n, torque_n_m) = nozzle_force_and_torque_at_full_throttle(nozzle);
        let torque_kn_m = torque_n_m / 1000.0;
        let expected = EXPECTED_TORQUE_KN_M[index];
        for axis in 0..3 {
            assert!(
                (torque_kn_m[axis] - expected[axis]).abs() < 0.01,
                "{} axis {axis}: got {} kN·m, expected {}",
                nozzle.name,
                torque_kn_m[axis],
                expected[axis]
            );
        }
    }
}

#[test]
fn firing_front_left_lift_raises_the_left_front_corner() {
    // In the air, so the wheels do not interfere: 10 m up, level, still.
    let mut world = World::new_on_flat_ground(&config(), DEFAULT_GROUND_FRICTION);
    world.place_car([0.0, 10.0, 0.0], [0.0, 0.0, 0.0, 1.0], [0.0; 3], [0.0; 3]);
    let mut throttles = [0.0; NOZZLE_COUNT];
    throttles[0] = 1.0; // L_FL

    let steps = PHYSICS_HZ as usize / 10; // 0.1 s
    for _ in 0..steps {
        world.set_controller_throttles(throttles);
        world.step(Default::default());
    }

    // SPEC §4: positive roll lifts the left side; positive pitch lifts the nose.
    let [roll_rate, _yaw_rate, pitch_rate] = world.angular_velocity_body_rad_s();
    assert!(roll_rate > 0.0, "roll rate {roll_rate}");
    assert!(pitch_rate > 0.0, "pitch rate {pitch_rate}");
}

#[test]
fn boost_nozzles_push_the_car_forward() {
    let mut world = World::new_on_flat_ground(&config(), DEFAULT_GROUND_FRICTION);
    world.place_car([0.0, 10.0, 0.0], [0.0, 0.0, 0.0, 1.0], [0.0; 3], [0.0; 3]);
    let mut throttles = [0.0; NOZZLE_COUNT];
    throttles[8] = 1.0; // B_L
    throttles[9] = 1.0; // B_R

    for _ in 0..PHYSICS_HZ / 2 {
        world.set_controller_throttles(throttles);
        world.step(Default::default());
    }

    // 16 kN on 1800 kg is about 8.9 m/s² forward; after 0.5 s (minus valve lag) ≈ 4 m/s.
    let forward_speed_m_s = world.forward_speed_m_s();
    assert!(
        forward_speed_m_s > 3.5 && forward_speed_m_s < 4.6,
        "speed {forward_speed_m_s}"
    );
}
