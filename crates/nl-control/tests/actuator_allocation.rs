//! The actuator-aware allocation (`[allocation] model = "actuator_aware"`): PD plans its
//! throttles with the actuator predictor, so the valve lag, the Hop/Boost throttles
//! underneath and the interlock are part of the plan. The legacy allocation stays the
//! default and must not change.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_control::allocation::{ActuatorPlan, AllocationModel, Allocator};
use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};
use nl_control::{ControlCtx, Controller, Pd};
use nl_sim::car_state::{CarState, Wrench};
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// Rolled 40° about the forward axis (quaternion x, y, z, w).
const ROLLED_40_DEG: [f32; 4] = [0.342_020_14, 0.0, 0.0, 0.939_692_6];

/// A torque PD might ask for: roll back and a little pitch, in N·m.
const SOME_TORQUE: Wrench = [0.0, 0.0, 0.0, 3000.0, 0.0, -1500.0];

fn controllers(model: AllocationModel) -> ControllersConfig {
    let mut config = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
    config.allocation.model = model;
    config
}

fn new_world() -> World {
    let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML).unwrap();
    World::new_on_flat_ground(&vehicle, modes.realistic, DEFAULT_GROUND_FRICTION, 0)
}

/// A car rolled 40°, 5 m up and still: PD wants to roll it back.
fn rolled_world() -> World {
    let mut world = new_world();
    world
        .place_car([0.0, 5.0, 0.0], ROLLED_40_DEG, [0.0; 3], [0.0; 3])
        .unwrap();
    world
}

fn pd(world: &World, model: AllocationModel) -> Pd {
    Pd::new(&world.car_model(), &controllers(model))
}

fn allocator() -> Allocator {
    let world = new_world();
    Allocator::new(
        &world.car_model().nozzle_wrenches,
        &controllers(AllocationModel::Legacy).allocation,
    )
}

fn legacy_answer(allocator: &Allocator, wrench: Wrench) -> [f32; NOZZLE_COUNT] {
    let mut throttles = [0.0; NOZZLE_COUNT];
    for _ in 0..10 {
        allocator.solve(wrench, 1.0, &mut throttles);
    }
    throttles
}

fn gain() -> f32 {
    new_world().actuator_state().controller_gain()
}

fn torque_error(wanted: &Wrench, got: &Wrench) -> f32 {
    let mut sum = 0.0;
    for row in 3..6 {
        sum += (got[row] - wanted[row]).powi(2);
    }
    sum.sqrt()
}

#[test]
fn the_repo_config_keeps_the_legacy_allocation() {
    let config = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();

    assert_eq!(config.allocation.model, AllocationModel::Legacy);
}

#[test]
fn the_actuator_aware_model_is_picked_by_name_and_a_wrong_name_is_refused() {
    let named = |name: &str| DEFAULT_CONTROLLERS_TOML.replace("model = \"legacy\"", name);

    let aware = ControllersConfig::from_toml_str(&named("model = \"actuator_aware\"")).unwrap();
    let wrong = ControllersConfig::from_toml_str(&named("model = \"fancy\""));

    assert_eq!(aware.allocation.model, AllocationModel::ActuatorAware);
    assert!(wrong.unwrap_err().to_string().contains("fancy"));
}

#[test]
fn legacy_pd_ignores_the_actuator_state() {
    let world = rolled_world();
    let with_actuators = ControlCtx::for_world(&world, InputFrame::default());
    let without_actuators = ControlCtx {
        actuators: None,
        ..with_actuators
    };

    let mut first = pd(&world, AllocationModel::Legacy);
    let mut second = pd(&world, AllocationModel::Legacy);

    assert_eq!(first.act(&with_actuators), second.act(&without_actuators));
    assert!(first.last_allocation().is_none());
}

#[test]
fn with_valves_already_at_the_answer_the_plan_keeps_it() {
    let allocator = allocator();
    let answer = legacy_answer(&allocator, SOME_TORQUE);
    let gain = gain();
    // Valves held at `answer` and no new command: they close to (1 − gain) of it on average.
    let plan = ActuatorPlan {
        valve_openings_with_no_command: answer.map(|opening| opening * (1.0 - gain)),
        mean_feed_forward: [0.0; NOZZLE_COUNT],
        max_feed_forward: [0.0; NOZZLE_COUNT],
        controller_gain: gain,
    };
    let mut throttles = answer;

    allocator.solve_actuator_aware(SOME_TORQUE, 1.0, &plan, &mut throttles);

    for nozzle in 0..NOZZLE_COUNT {
        assert!(
            (throttles[nozzle] - answer[nozzle]).abs() < 1e-3,
            "nozzle {nozzle}: {} vs {}",
            throttles[nozzle],
            answer[nozzle]
        );
    }
}

#[test]
fn from_closed_valves_the_plan_opens_further_to_make_up_for_the_lag() {
    let allocator = allocator();
    let answer = legacy_answer(&allocator, SOME_TORQUE);
    let plan = ActuatorPlan {
        valve_openings_with_no_command: [0.0; NOZZLE_COUNT],
        mean_feed_forward: [0.0; NOZZLE_COUNT],
        max_feed_forward: [0.0; NOZZLE_COUNT],
        controller_gain: gain(),
    };
    let mut throttles = [0.0; NOZZLE_COUNT];
    for _ in 0..10 {
        allocator.solve_actuator_aware(SOME_TORQUE, 1.0, &plan, &mut throttles);
    }

    let legacy_total: f32 = answer.iter().sum();
    let aware_total: f32 = throttles.iter().sum();
    assert!(
        aware_total > legacy_total,
        "{aware_total} vs {legacy_total}"
    );
    assert!(
        throttles
            .iter()
            .all(|throttle| (0.0..=1.0).contains(throttle))
    );
}

#[test]
fn a_nozzle_under_a_hop_gets_only_the_headroom_above_it() {
    let allocator = allocator();
    let hop_ff = 0.75;
    let mut feed_forward = [0.0; NOZZLE_COUNT];
    feed_forward[..4].fill(hop_ff);
    let plan = ActuatorPlan {
        valve_openings_with_no_command: feed_forward,
        mean_feed_forward: feed_forward,
        max_feed_forward: feed_forward,
        controller_gain: gain(),
    };
    // Far more lift than the Hop gives.
    let wanted: Wrench = [0.0, 60_000.0, 0.0, 0.0, 0.0, 0.0];
    let mut throttles = [0.0; NOZZLE_COUNT];
    for _ in 0..10 {
        allocator.solve_actuator_aware(wanted, 1.0, &plan, &mut throttles);
    }

    for lift_nozzle in 0..4 {
        assert!(throttles[lift_nozzle] <= 1.0 - hop_ff, "{throttles:?}");
    }
}

#[test]
fn while_the_interlock_holds_actuator_aware_pd_fires_nothing_and_says_so() {
    let mut parked = new_world();
    for _ in 0..120 {
        parked.step(InputFrame::default());
    }
    assert!(parked.thrusters_locked());
    // A tumbling car's view, with the parked car's (locked) actuators.
    let ctx = ControlCtx {
        state: CarState {
            gravity_dir_body: [0.3, 0.9, 0.0],
            angular_velocity_body_rad_s: [2.0, 0.5, -1.0],
            height_m: 4.0,
            wheel_contact: [false; 4],
            ..CarState::default()
        },
        actuators: Some(parked.actuator_state()),
        ..ControlCtx::default()
    };
    let mut controller = pd(&parked, AllocationModel::ActuatorAware);

    let throttles = controller.act(&ctx);

    assert_eq!(throttles, [0.0; NOZZLE_COUNT]);
    assert!(controller.last_allocation().unwrap().thrusters_locked);
}

#[test]
fn actuator_aware_pd_comes_closer_to_the_torque_it_asks_for_from_closed_valves() {
    let world = rolled_world();
    let ctx = ControlCtx::for_world(&world, InputFrame::default());
    let actuators = world.actuator_state();
    let mut legacy = pd(&world, AllocationModel::Legacy);
    let mut aware = pd(&world, AllocationModel::ActuatorAware);

    let legacy_push = actuators.predict(legacy.act(&ctx)).mean_wrench;
    aware.act(&ctx);
    let report = aware.last_allocation().unwrap();

    let wanted = report.requested_wrench;
    let aware_error = torque_error(&wanted, &report.predicted_wrench);
    let legacy_error = torque_error(&wanted, &legacy_push);
    assert!(
        aware_error < 0.5 * legacy_error,
        "{aware_error} vs {legacy_error}"
    );
    for row in 0..6 {
        let residual = report.predicted_wrench[row] - report.requested_wrench[row];
        assert_eq!(report.residual_wrench[row], residual);
    }
    assert!(report.air_used_kg > 0.0);
    assert!(!report.thrusters_locked);
}
