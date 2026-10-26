//! The actuator predictor (`nl_sim::actuator_model`): what the thrusters will do over
//! the next control interval, worked out from a copy of the world's valves, tank and
//! Hop/Boost state with the same functions the world steps. Each test predicts, then
//! steps the real world the same way and checks the two agree.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, buttons, new_world, vehicle};
use nl_sim::actuator_model::IntervalPrediction;
use nl_sim::conventions::{
    GRAVITY_M_S2, NOZZLE_COUNT, PHYSICS_DT_S, PHYSICS_STEPS_PER_CONTROL_STEP,
};
use nl_sim::input::{BUTTON_BOOST, BUTTON_HOP, InputFrame};
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::thrusters::valve_step_fraction;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// Front-left lift nozzle (nozzle order, SPEC §4).
const L_FL: usize = 0;
const LIFT: [usize; 4] = [0, 1, 2, 3];
const BOOST: [usize; 2] = [8, 9];

/// A car hanging still, level, 6 m up: no wheel touches and the interlock is off.
fn hanging_world() -> World {
    let mut world = new_world();
    hang(&mut world);
    world
}

fn hang(world: &mut World) {
    world
        .place_car([0.0, 6.0, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
}

/// Runs one control interval of the real world: the controller's throttles are held
/// for both physics steps, and the driver holds `input`.
fn step_interval(world: &mut World, throttles: [f32; NOZZLE_COUNT], input: InputFrame) {
    for _ in 0..PHYSICS_STEPS_PER_CONTROL_STEP {
        world.set_controller_throttles(throttles);
        world.step(input);
    }
}

/// A car that has stood on its wheels for a second, so the wheels have found the ground.
fn parked_world() -> World {
    let mut world = new_world();
    for _ in 0..120 {
        world.step(InputFrame::default());
    }
    world
}

fn predict(world: &World, throttles: [f32; NOZZLE_COUNT]) -> IntervalPrediction {
    world.actuator_state().predict(throttles)
}

fn some_throttles() -> [f32; NOZZLE_COUNT] {
    [0.9, 0.1, 0.4, 0.0, 0.7, 0.2, 0.0, 1.0, 0.3, 0.6]
}

#[test]
fn the_prediction_matches_the_valves_and_air_after_one_real_interval() {
    let mut world = hanging_world();
    let throttles = some_throttles();

    let prediction = predict(&world, throttles);
    step_interval(&mut world, throttles, InputFrame::default());

    // The same functions in the same order, so the same bits.
    assert_eq!(prediction.final_valve_openings, world.effective_throttles());
    assert_eq!(prediction.air_left_kg, world.air_kg());
    assert!(!prediction.thrusters_locked);
    assert!(prediction.air_used_kg > 0.0);
}

#[test]
fn the_predicted_push_matches_how_the_car_accelerates_about_its_true_centre_of_mass() {
    // A centre of mass 0.1 m forward of the body origin (domain randomization's limit):
    // the front-left lift nozzle then pitches the nose up less than about the origin.
    let mut config = vehicle();
    config.chassis.center_of_mass_offset_m = [0.1, 0.0, 0.0];
    let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
        .unwrap()
        .realistic;
    let mut world = World::new_on_flat_ground(&config, mode, DEFAULT_GROUND_FRICTION, 0);
    hang(&mut world);
    let mut throttles = [0.0; NOZZLE_COUNT];
    throttles[L_FL] = 1.0;
    let model = world.car_model();
    let speed_before = world.car_state().linear_velocity_body_m_s;

    let prediction = predict(&world, throttles);
    step_interval(&mut world, throttles, InputFrame::default());

    let interval_s = PHYSICS_DT_S * PHYSICS_STEPS_PER_CONTROL_STEP as f32;
    let speed_after = world.car_state().linear_velocity_body_m_s;
    let lift_accel_m_s2 = (speed_after[1] - speed_before[1]) / interval_s + GRAVITY_M_S2;
    let predicted_lift_accel_m_s2 = prediction.mean_wrench[1] / model.mass_kg;
    assert!(
        (lift_accel_m_s2 / predicted_lift_accel_m_s2 - 1.0).abs() < 0.02,
        "lift {lift_accel_m_s2} m/s², predicted {predicted_lift_accel_m_s2}"
    );
    // Started still, so the spin after the interval is the torque's doing: pitch is +Z.
    // The configured inertia, not `car_model`'s: that one lists Rapier's principal
    // moments, which swap yaw and pitch (docs/control-upgrade-audit.md, finding 8).
    let pitch_inertia_kg_m2 = config.chassis.inertia_kg_m2[2];
    let pitch_rate_rad_s = world.car_state().angular_velocity_body_rad_s[2];
    let predicted_pitch_rate_rad_s = prediction.mean_wrench[5] / pitch_inertia_kg_m2 * interval_s;
    assert!(
        (pitch_rate_rad_s / predicted_pitch_rate_rad_s - 1.0).abs() < 0.03,
        "pitch rate {pitch_rate_rad_s} rad/s, predicted {predicted_pitch_rate_rad_s}"
    );
    // About the body origin (the allocation's model) the pitch torque would be bigger.
    let origin_pitch_torque_n_m = model.nozzle_wrenches[L_FL][5];
    assert!(prediction.mean_wrench[5] < origin_pitch_torque_n_m);
}

#[test]
fn with_no_command_open_valves_close_at_the_valve_rate() {
    let mut world = hanging_world();
    step_interval(&mut world, [1.0; NOZZLE_COUNT], InputFrame::default());
    let open = world.effective_throttles();

    let prediction = predict(&world, [0.0; NOZZLE_COUNT]);
    step_interval(&mut world, [0.0; NOZZLE_COUNT], InputFrame::default());

    let time_constant_s = vehicle().thrusters.valve_time_constant_s;
    let step_fraction = valve_step_fraction(PHYSICS_DT_S, time_constant_s);
    let left = (1.0 - step_fraction) * (1.0 - step_fraction);
    assert!((prediction.final_valve_openings[L_FL] - open[L_FL] * left).abs() < 1e-6);
    assert_eq!(prediction.final_valve_openings, world.effective_throttles());
    // Still pushing up while they close.
    assert!(prediction.mean_wrench[1] > 0.0);
}

#[test]
fn a_fading_tank_scales_the_predicted_push_by_its_gate() {
    let mut full = hanging_world();
    let mut fading = hanging_world();
    let mode = fading.mode();
    let fade_start_kg = mode.empty_fade_fraction * mode.tank_capacity_kg;
    fading.set_air_kg(0.5 * fade_start_kg);
    let throttles = some_throttles();

    let full_prediction = predict(&full, throttles);
    let fading_prediction = predict(&fading, throttles);
    step_interval(&mut full, throttles, InputFrame::default());
    step_interval(&mut fading, throttles, InputFrame::default());

    let ratio = fading_prediction.mean_wrench[1] / full_prediction.mean_wrench[1];
    assert!((0.45..0.5).contains(&ratio), "push ratio {ratio}");
    assert_eq!(fading_prediction.air_left_kg, fading.air_kg());
    assert!(fading_prediction.final_tank_gate < 0.5);
}

#[test]
fn an_empty_tank_predicts_no_push_and_no_air() {
    let mut world = hanging_world();
    world.set_air_kg(0.0);

    let prediction = predict(&world, [1.0; NOZZLE_COUNT]);

    assert_eq!(prediction.mean_wrench, [0.0; 6]);
    assert_eq!(prediction.air_used_kg, 0.0);
    assert_eq!(prediction.final_tank_gate, 0.0);
}

#[test]
fn on_the_ground_the_interlock_cuts_the_controller_but_not_boost() {
    let mut world = parked_world();
    let hold_boost = buttons(BUTTON_BOOST);
    step_interval(&mut world, [0.0; NOZZLE_COUNT], hold_boost);
    assert!(world.thrusters_locked());

    let with_command = predict(&world, [1.0; NOZZLE_COUNT]);
    let without_command = predict(&world, [0.0; NOZZLE_COUNT]);
    step_interval(&mut world, [1.0; NOZZLE_COUNT], hold_boost);

    assert!(with_command.thrusters_locked);
    // The controller's full command changes nothing: the interlock is not bypassed.
    assert_eq!(with_command, without_command);
    assert_eq!(
        with_command.final_valve_openings,
        world.effective_throttles()
    );
    // Boost still pushes forward (+X), and only through the boost nozzles.
    assert!(with_command.feed_forward_wrench[0] > 0.0);
    for nozzle in 0..NOZZLE_COUNT {
        let boosting = BOOST.contains(&nozzle);
        assert_eq!(with_command.mean_feed_forward[nozzle] > 0.0, boosting);
    }
}

#[test]
fn during_a_hop_the_controller_only_has_the_headroom_above_the_hop() {
    let mut world = parked_world();
    let hold_hop = buttons(BUTTON_HOP);
    world.step(hold_hop);
    // Climb until the interlock lets go (about 0.3 s into the 0.4 s Hop); holding the
    // button starts no second Hop.
    for _ in 0..45 {
        if !world.thrusters_locked() {
            break;
        }
        world.step(hold_hop);
    }
    assert!(!world.thrusters_locked());
    assert!(world.hop_boost().hop_active());
    let mut throttles = [0.0; NOZZLE_COUNT];
    for nozzle in LIFT {
        throttles[nozzle] = 1.0;
    }
    let hop_ff = world.mode().hop_ff;

    let prediction = predict(&world, throttles);
    step_interval(&mut world, throttles, hold_hop);

    assert_eq!(prediction.final_valve_openings, world.effective_throttles());
    for nozzle in 0..NOZZLE_COUNT {
        let lifting = LIFT.contains(&nozzle);
        assert_eq!(prediction.saturated[nozzle], lifting, "nozzle {nozzle}");
        let expected_ff = if lifting { hop_ff } else { 0.0 };
        assert_eq!(prediction.max_feed_forward[nozzle], expected_ff);
    }
    // The Hop's own push is counted once, in the feed-forward wrench.
    assert!(prediction.feed_forward_wrench[1] > 0.0);
    assert!(prediction.mean_wrench[1] > prediction.feed_forward_wrench[1]);
}

#[test]
fn predicting_changes_nothing_in_the_world() {
    let world = hanging_world();
    let before = world.snapshot().unwrap();

    let state = world.actuator_state();
    for _ in 0..3 {
        let _ = state.predict(some_throttles());
    }

    assert_eq!(world.snapshot().unwrap(), before);
}

#[test]
fn the_same_state_and_command_always_predict_the_same_bits() {
    let world = hanging_world();
    let state = world.actuator_state();

    let first = state.predict(some_throttles());
    let second = state.predict(some_throttles());

    assert_eq!(first, second);
}
