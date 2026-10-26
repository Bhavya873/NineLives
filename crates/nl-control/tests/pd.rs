//! PD baseline tests (SPEC §8, §12): PD's torques have restoring signs, and PD saves a
//! dropped car. The allocation PD calls is tested in allocation.rs.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};
use nl_control::{ControlCtx, Controller, Off, Pd};
use nl_sim::car_state::{CarModel, CarState};
use nl_sim::conventions::{NOZZLE_COUNT, PHYSICS_HZ};
use nl_sim::events::SimEvent;
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// Rolled +90° about forward: lying on its right side (nl-sim's test fixtures are not
/// reachable from this crate, so this one constant is repeated).
const ON_ITS_RIGHT_SIDE: [f32; 4] = [
    std::f32::consts::FRAC_1_SQRT_2,
    0.0,
    0.0,
    std::f32::consts::FRAC_1_SQRT_2,
];

fn controllers() -> ControllersConfig {
    ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap()
}

fn new_world() -> World {
    let config = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML).unwrap();
    World::new_on_flat_ground(&config, modes.realistic, DEFAULT_GROUND_FRICTION, 0)
}

fn car_model() -> CarModel {
    new_world().car_model()
}

/// Runs the world for `seconds` with `controller` at 60 Hz; returns every event.
fn run_with(world: &mut World, controller: &mut dyn Controller, seconds: f32) -> Vec<SimEvent> {
    let mut events = Vec::new();
    for _ in 0..(seconds * PHYSICS_HZ as f32).round() as usize {
        if world.is_control_step() {
            let ctx = ControlCtx::for_world(world, InputFrame::default());
            world.set_controller_throttles(controller.act(&ctx));
        }
        world.step(InputFrame::default());
        events.extend(world.drain_events());
    }
    events
}

#[test]
fn a_car_lying_on_its_right_side_gets_a_negative_roll_torque_to_come_back() {
    let pd = Pd::new(&car_model(), &controllers());
    // Right side (+Z) down: gravity points along body +Z.
    let state = CarState {
        gravity_dir_body: [0.0, 0.0, 1.0],
        wheel_contact: [false; 4],
        height_m: 6.0,
        ..CarState::default()
    };

    let wrench = pd.wanted_wrench(&state);

    assert!(wrench[3] < 0.0, "roll torque {wrench:?}");
}

#[test]
fn a_nose_up_car_gets_a_negative_pitch_torque_to_bring_the_nose_down() {
    let pd = Pd::new(&car_model(), &controllers());
    // Pitched 30° nose up (about +Z): gravity points backward and down in the body.
    let state = CarState {
        gravity_dir_body: [-0.5, -0.866, 0.0],
        wheel_contact: [false; 4],
        height_m: 6.0,
        ..CarState::default()
    };

    let wrench = pd.wanted_wrench(&state);

    assert!(wrench[5] < 0.0, "pitch torque {wrench:?}");
    assert_eq!(wrench[3], 0.0, "no roll error, so no roll torque");
}

#[test]
fn an_exactly_upside_down_car_still_gets_a_roll_torque() {
    let pd = Pd::new(&car_model(), &controllers());
    let state = CarState {
        gravity_dir_body: [0.0, 1.0, 0.0],
        wheel_contact: [false; 4],
        height_m: 6.0,
        ..CarState::default()
    };

    let wrench = pd.wanted_wrench(&state);

    assert!(wrench[3] != 0.0, "roll torque {wrench:?}");
    assert_eq!(wrench[4], 0.0);
    assert_eq!(wrench[5], 0.0);
}

#[test]
fn the_thruster_matrix_has_rank_6_so_every_axis_is_controllable() {
    // SPEC §12: B (6×10) has rank 6. Gaussian elimination on B with each row scaled to
    // its strongest entry, counting pivots that are clearly not zero.
    let model = car_model();
    let mut rows: Vec<Vec<f32>> = Vec::new();
    for row in 0..6 {
        let values: Vec<f32> = model
            .nozzle_wrenches
            .iter()
            .map(|wrench| wrench[row])
            .collect();
        let strongest = values
            .iter()
            .fold(0.0_f32, |max, value| max.max(value.abs()));
        rows.push(values.iter().map(|value| value / strongest).collect());
    }

    assert_eq!(matrix_rank(rows), 6);
}

/// Rank by Gaussian elimination with partial pivoting.
fn matrix_rank(mut rows: Vec<Vec<f32>>) -> usize {
    let columns = rows[0].len();
    let mut rank = 0;
    for column in 0..columns {
        let Some(pivot) = (rank..rows.len())
            .max_by(|a, b| rows[*a][column].abs().total_cmp(&rows[*b][column].abs()))
        else {
            break;
        };
        if rows[pivot][column].abs() < 1e-4 {
            continue;
        }
        rows.swap(rank, pivot);
        let pivot_row = rows[rank].clone();
        for (other, row) in rows.iter_mut().enumerate() {
            if other != rank {
                let factor = row[column] / pivot_row[column];
                for (value, pivot_value) in row.iter_mut().zip(&pivot_row).skip(column) {
                    *value -= factor * pivot_value;
                }
            }
        }
        rank += 1;
    }
    rank
}

#[test]
fn pd_damps_a_spin_with_an_opposing_torque() {
    let pd = Pd::new(&car_model(), &controllers());
    let state = CarState {
        angular_velocity_body_rad_s: [0.0, 3.0, 0.0],
        wheel_contact: [false; 4],
        height_m: 6.0,
        ..CarState::default()
    };

    let wrench = pd.wanted_wrench(&state);

    assert!(wrench[4] < 0.0, "yaw torque {wrench:?}");
}

#[test]
fn pd_brakes_a_fast_low_upright_fall_and_ignores_a_slow_one() {
    let pd = Pd::new(&car_model(), &controllers());
    // Level, so the body's -Y is world down.
    let falling = |speed_m_s: f32| CarState {
        linear_velocity_body_m_s: [0.0, -speed_m_s, 0.0],
        wheel_contact: [false; 4],
        height_m: 2.0,
        ..CarState::default()
    };

    assert!(pd.wanted_wrench(&falling(8.0))[1] > 0.0);
    assert_eq!(pd.wanted_wrench(&falling(2.0))[1], 0.0);
}

/// A level car in the air at `height_m`, falling at `speed_m_s`, in a mode whose thrust
/// is multiplied by `thrust_scale`.
fn falling_level(height_m: f32, speed_m_s: f32, thrust_scale: f32) -> CarState {
    CarState {
        linear_velocity_body_m_s: [0.0, -speed_m_s, 0.0],
        wheel_contact: [false; 4],
        height_m,
        mode_thrust_scale: thrust_scale,
        ..CarState::default()
    }
}

#[test]
fn a_fast_fall_brakes_from_high_enough_to_stop_and_a_slow_one_still_waits() {
    let pd = Pd::new(&car_model(), &controllers());

    // 15 m/s needs about 13 m to stop on Realistic's lift: braking starts well above the
    // old fixed 3 m, but not from any height.
    assert!(pd.wanted_wrench(&falling_level(6.0, 15.0, 1.0))[1] > 0.0);
    assert!(pd.wanted_wrench(&falling_level(12.0, 15.0, 1.0))[1] > 0.0);
    assert_eq!(pd.wanted_wrench(&falling_level(40.0, 15.0, 1.0))[1], 0.0);
    // 5 m/s stops within the 3 m floor, so it waits as before.
    assert_eq!(pd.wanted_wrench(&falling_level(6.0, 5.0, 1.0))[1], 0.0);
}

#[test]
fn arcade_thrust_lets_the_same_fall_brake_later() {
    let pd = Pd::new(&car_model(), &controllers());

    let realistic = pd.wanted_wrench(&falling_level(9.0, 15.0, 1.0));
    let arcade = pd.wanted_wrench(&falling_level(9.0, 15.0, 2.0));

    assert!(realistic[1] > 0.0);
    assert_eq!(arcade[1], 0.0);
}

#[test]
fn sliding_right_in_the_air_gets_a_push_to_the_left_and_on_wheels_none() {
    let pd = Pd::new(&car_model(), &controllers());
    let sliding = |wheel_contact: [bool; 4]| CarState {
        linear_velocity_body_m_s: [0.0, 0.0, 3.0],
        wheel_contact,
        height_m: 3.0,
        ..CarState::default()
    };

    assert!(pd.wanted_wrench(&sliding([false; 4]))[2] < 0.0);
    assert_eq!(pd.wanted_wrench(&sliding([true; 4]))[2], 0.0);
}

#[test]
fn a_car_lying_on_its_right_side_is_pushed_towards_its_left_side_which_faces_up() {
    let pd = Pd::new(&car_model(), &controllers());
    // Right side (+Z) down on the ground: body −Z points up.
    let state = CarState {
        gravity_dir_body: [0.0, 0.0, 1.0],
        wheel_contact: [false; 4],
        height_m: 0.5,
        ..CarState::default()
    };

    let wrench = pd.wanted_wrench(&state);

    assert!(wrench[2] < 0.0, "side push {wrench:?}");
}

/// A level car high in the air, holding still.
fn level_in_the_air(height_m: f32) -> CarState {
    CarState {
        wheel_contact: [false; 4],
        height_m,
        ..CarState::default()
    }
}

fn input(throttle: i8, steer: i8) -> InputFrame {
    InputFrame {
        throttle,
        steer,
        ..InputFrame::default()
    }
}

#[test]
fn steering_left_in_the_air_asks_for_a_left_yaw_and_forward_tips_the_nose_down() {
    let pd = Pd::new(&car_model(), &controllers());
    let state = level_in_the_air(10.0);

    let turning = pd.wanted_wrench_for(&state, &input(0, 127));
    let diving = pd.wanted_wrench_for(&state, &input(127, 0));

    // Positive yaw turns the nose left; a negative torque about +Z tips the nose down.
    assert!(turning[4] > 0.0, "yaw torque {turning:?}");
    assert!(diving[5] < 0.0, "pitch torque {diving:?}");
}

#[test]
fn near_the_ground_air_control_gives_way_to_lining_up_for_landing() {
    let pd = Pd::new(&car_model(), &controllers());
    let state = level_in_the_air(1.5);

    let wrench = pd.wanted_wrench_for(&state, &input(127, 127));

    assert_eq!(wrench[4], 0.0);
    assert_eq!(wrench[5], 0.0);
}

#[test]
fn a_car_spinning_fast_past_upside_down_keeps_rolling_the_same_way() {
    let pd = Pd::new(&car_model(), &controllers());
    // Rolled about 135°: the short way back is a negative roll, but the car is rolling
    // positive, away from it, fast enough that carrying on is the shorter trip.
    let tilt = 135_f32.to_radians();
    let state = CarState {
        gravity_dir_body: [0.0, -tilt.cos(), tilt.sin()],
        angular_velocity_body_rad_s: [8.0, 0.0, 0.0],
        wheel_contact: [false; 4],
        height_m: 10.0,
        ..CarState::default()
    };

    let wrench = pd.wanted_wrench(&state);

    assert!(wrench[3] > 0.0, "roll torque {wrench:?}");
}

#[test]
fn pd_never_asks_for_more_than_the_angular_acceleration_cap() {
    let config = controllers();
    let model = car_model();
    let pd = Pd::new(&model, &config);
    let state = CarState {
        gravity_dir_body: [0.7, 0.0, 0.7],
        angular_velocity_body_rad_s: [6.0, -4.0, 5.0],
        wheel_contact: [false; 4],
        height_m: 10.0,
        ..CarState::default()
    };

    let wrench = pd.wanted_wrench(&state);

    let mut accel_squared = 0.0;
    for axis in 0..3 {
        let accel = wrench[3 + axis] / model.principal_inertia_kg_m2[axis];
        accel_squared += accel * accel;
    }
    let cap = config.pd.max_angular_accel_rad_s2;
    assert!(
        accel_squared.sqrt() <= cap * 1.001,
        "{} > {cap}",
        accel_squared.sqrt()
    );
}

#[test]
fn pd_leaves_the_thrusters_off_while_the_car_drives() {
    let mut pd = Pd::new(&car_model(), &controllers());

    let throttles = pd.act(&ControlCtx::default());

    assert_eq!(throttles, [0.0; NOZZLE_COUNT]);
}

/// Drops the car on its right side from 8 m under `controller`; true if it is saved
/// without crashing within 5 s.
fn survives_side_drop_from_8_m(controller: &mut dyn Controller) -> bool {
    let mut world = new_world();
    world
        .place_car([0.0, 8.0, 0.0], ON_ITS_RIGHT_SIDE, [0.0; 3], [0.0; 3])
        .unwrap();

    let events = run_with(&mut world, controller, 5.0);

    let saved = events
        .iter()
        .any(|event| matches!(event, SimEvent::Save { .. }));
    let crashed = events
        .iter()
        .any(|event| matches!(event, SimEvent::Crash { .. }));
    saved && !crashed
}

#[test]
fn pd_saves_a_car_dropped_on_its_side_from_8_m_and_off_does_not() {
    let mut pd = Pd::new(&car_model(), &controllers());

    assert!(survives_side_drop_from_8_m(&mut pd), "PD should save it");
    assert!(!survives_side_drop_from_8_m(&mut Off), "Off should not");
}

#[test]
fn pd_driving_over_bumps_uses_almost_no_air() {
    // The owner saw thrusters fire whenever a wheel left the ground on bumps.
    let course = nl_sim::course::CourseSpec::from_json_str(include_str!(
        "../../../courses/bumps-strip.json"
    ))
    .unwrap();
    let config = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML).unwrap();
    let mut world = World::new_on_course(
        &config,
        modes.realistic,
        &course,
        DEFAULT_GROUND_FRICTION,
        0,
    );
    let mut pd = Pd::new(&world.car_model(), &controllers());
    let drive = InputFrame {
        throttle: 127,
        ..InputFrame::default()
    };

    let used_kg = nl_control::eval::air_used_driving(&mut world, &mut pd, drive, 8.0);

    let x_m = world.chassis_pose().position_m[0];
    assert!(x_m > 80.0, "only reached x = {x_m} m ({used_kg} kg used)");
    assert!(used_kg < 8.0, "used {used_kg} kg");
}
