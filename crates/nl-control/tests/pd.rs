//! PD baseline tests (SPEC §8, §12): the allocation's answers stay in 0..1 and produce
//! the wanted push, PD's torques have restoring signs, and PD saves a dropped car.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_control::allocation::Allocator;
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
fn the_allocation_lifts_with_the_four_lift_nozzles_and_stays_in_bounds() {
    let model = car_model();
    let allocator = Allocator::new(&model.nozzle_wrenches, &controllers().allocation);
    let wanted = [0.0, 20000.0, 0.0, 0.0, 0.0, 0.0];
    let mut throttles = [0.0; NOZZLE_COUNT];

    // Warm-started over a few control steps, as in the game.
    for _ in 0..10 {
        allocator.solve(wanted, 1.0, &mut throttles);
    }

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
