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

/// Rolled +90° about forward: lying on its right side.
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
    for _ in 0..(seconds * PHYSICS_HZ as f32) as usize {
        if world.is_control_step() {
            let ctx = ControlCtx {
                input: InputFrame::default(),
                state: world.car_state(),
            };
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
        wheels_down: 0,
        up_alignment: 0.0,
        height_m: 6.0,
        ..CarState::default()
    };

    let wrench = pd.wanted_wrench(&state);

    assert!(wrench[3] < 0.0, "roll torque {wrench:?}");
}

#[test]
fn pd_damps_a_spin_with_an_opposing_torque() {
    let pd = Pd::new(&car_model(), &controllers());
    let state = CarState {
        angular_velocity_body_rad_s: [0.0, 3.0, 0.0],
        wheels_down: 0,
        height_m: 6.0,
        ..CarState::default()
    };

    let wrench = pd.wanted_wrench(&state);

    assert!(wrench[4] < 0.0, "yaw torque {wrench:?}");
}

#[test]
fn pd_brakes_a_fast_low_upright_fall_and_ignores_a_slow_one() {
    let pd = Pd::new(&car_model(), &controllers());
    let falling = |speed_m_s: f32| CarState {
        linear_velocity_world_m_s: [0.0, -speed_m_s, 0.0],
        wheels_down: 0,
        height_m: 2.0,
        ..CarState::default()
    };

    assert!(pd.wanted_wrench(&falling(8.0))[1] > 0.0);
    assert_eq!(pd.wanted_wrench(&falling(2.0))[1], 0.0);
}

#[test]
fn pd_leaves_the_thrusters_off_while_the_car_drives() {
    let mut pd = Pd::new(&car_model(), &controllers());

    let throttles = pd.act(&ControlCtx::default());

    assert_eq!(throttles, [0.0; NOZZLE_COUNT]);
}

#[test]
fn pd_saves_a_car_dropped_on_its_side_from_8_m_and_off_does_not() {
    let mut saved_by = Vec::new();
    let mut controllers_under_test: [(&str, Box<dyn Controller>); 2] = [
        ("pd", Box::new(Pd::new(&car_model(), &controllers()))),
        ("off", Box::new(Off)),
    ];
    for (name, controller) in controllers_under_test.iter_mut() {
        let mut world = new_world();
        world
            .place_car([0.0, 8.0, 0.0], ON_ITS_RIGHT_SIDE, [0.0; 3], [0.0; 3])
            .unwrap();

        let events = run_with(&mut world, controller.as_mut(), 5.0);

        let saved = events
            .iter()
            .any(|event| matches!(event, SimEvent::Save { .. }));
        let crashed = events
            .iter()
            .any(|event| matches!(event, SimEvent::Crash { .. }));
        if saved && !crashed {
            saved_by.push(*name);
        }
    }

    assert_eq!(saved_by, vec!["pd"]);
}
