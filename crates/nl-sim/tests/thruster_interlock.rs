//! The thruster interlock (SPEC §7.4): while the car is steady on the ground (upright,
//! low, not falling, barely turning), the controller's throttles are cut, so no
//! controller can idle thrust while parked or push the car around while driving. Hop and
//! Boost feed-forward still work, and a falling or tumbling car is never cut off.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, UPSIDE_DOWN, buttons, new_world};
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::input::{BUTTON_HOP, InputFrame};
use nl_sim::world::World;

fn hold(world: &mut World, throttles: [f32; NOZZLE_COUNT], input: InputFrame, steps: usize) {
    for _ in 0..steps {
        world.set_controller_throttles(throttles);
        world.step(input);
    }
}

#[test]
fn a_parked_car_uses_no_air_whatever_the_controller_asks() {
    let mut world = new_world();
    hold(&mut world, [0.0; NOZZLE_COUNT], InputFrame::default(), 120);
    let air_before_kg = world.air_used_kg();

    hold(&mut world, [1.0; NOZZLE_COUNT], InputFrame::default(), 120);

    assert!(world.thrusters_locked());
    assert_eq!(world.air_used_kg(), air_before_kg);
}

#[test]
fn a_falling_car_still_gets_its_thrust() {
    let mut world = new_world();
    world
        .place_car([0.0, 6.0, 0.0], LEVEL, [0.0, -4.0, 0.0], [0.0; 3])
        .unwrap();

    hold(&mut world, [1.0; NOZZLE_COUNT], InputFrame::default(), 12);

    assert!(!world.thrusters_locked());
    assert!(world.air_used_kg() > 0.0);
}

#[test]
fn an_upside_down_car_on_the_ground_is_never_locked() {
    let mut world = new_world();
    world
        .place_car([0.0, 1.5, 0.0], UPSIDE_DOWN, [0.0; 3], [0.0; 3])
        .unwrap();

    hold(&mut world, [1.0; NOZZLE_COUNT], InputFrame::default(), 30);

    assert!(!world.thrusters_locked());
}

#[test]
fn hop_still_fires_while_the_controller_is_locked_out() {
    let mut world = new_world();
    hold(&mut world, [0.0; NOZZLE_COUNT], InputFrame::default(), 120);

    hold(&mut world, [0.0; NOZZLE_COUNT], buttons(BUTTON_HOP), 1);
    let mut highest_m: f32 = 0.0;
    for _ in 0..120 {
        hold(&mut world, [0.0; NOZZLE_COUNT], InputFrame::default(), 1);
        highest_m = highest_m.max(world.chassis_pose().position_m[1]);
    }

    assert!(
        highest_m > 0.9,
        "the hop should lift the car, highest {highest_m} m"
    );
}
