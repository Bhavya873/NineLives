//! Respawn tests (SPEC §7.9, with the explode-and-respawn rule from docs/DECISIONS.md):
//! a crashed car stays frozen for the respawn delay, then comes back at the last
//! checkpoint with a full tank; the Reset button does the same at once.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::new_world;
use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::events::SimEvent;
use nl_sim::input::{BUTTON_RESET, InputFrame};
use nl_sim::world::World;

const LEVEL: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
/// Below the out-of-bounds height, so the first step is a crash.
const OUT_OF_BOUNDS_M: [f32; 3] = [0.0, -25.0, 0.0];
/// From configs/vehicle.toml `[events] respawn_delay_s`.
const RESPAWN_DELAY_S: f32 = 1.5;

/// Steps `seconds` with no input and returns every event.
fn run_and_collect(world: &mut World, seconds: f32) -> Vec<SimEvent> {
    let mut events = Vec::new();
    for _ in 0..(seconds * PHYSICS_HZ as f32).round() as usize {
        world.step(InputFrame::default());
        events.extend(world.drain_events());
    }
    events
}

/// Crashes the car by dropping it out of the world, then forgets those events.
fn crash(world: &mut World) {
    world
        .place_car(OUT_OF_BOUNDS_M, LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
    world.set_air_kg(10.0);
    world.step(InputFrame::default());
    assert!(world.is_crashed());
    world.drain_events();
}

fn count_respawns(events: &[SimEvent]) -> usize {
    let mut respawns = 0;
    for event in events {
        if *event == SimEvent::Respawn {
            respawns += 1;
        }
    }
    respawns
}

#[test]
fn a_crashed_car_stays_frozen_until_the_respawn_delay_has_passed() {
    let mut world = new_world();
    crash(&mut world);
    let wreck = world.chassis_pose();

    let events = run_and_collect(&mut world, RESPAWN_DELAY_S - 0.1);

    assert_eq!(world.chassis_pose(), wreck);
    assert!(world.is_crashed());
    assert_eq!(count_respawns(&events), 0);
}

#[test]
fn after_the_delay_the_car_is_back_at_the_start_with_a_full_tank() {
    let mut world = new_world();
    let start = world.chassis_pose();
    crash(&mut world);

    let events = run_and_collect(&mut world, RESPAWN_DELAY_S);

    assert_eq!(count_respawns(&events), 1);
    assert!(!world.is_crashed());
    assert_eq!(world.air_fraction(), 1.0);
    assert_eq!(world.chassis_pose().position_m, start.position_m);
    assert_eq!(world.crashes(), 1);
}

#[test]
fn pressing_reset_respawns_at_once_even_without_a_crash() {
    let mut world = new_world();
    world.set_checkpoint();
    let checkpoint = world.chassis_pose();
    world
        .place_car([30.0, 5.0, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
    let reset = InputFrame {
        buttons: BUTTON_RESET,
        ..InputFrame::default()
    };

    world.step(reset);

    assert_eq!(count_respawns(&world.drain_events()), 1);
    let position_m = world.chassis_pose().position_m;
    // One physics step has run since the respawn, so allow a tiny settle.
    assert!((position_m[0] - checkpoint.position_m[0]).abs() < 0.01);
    assert!((position_m[1] - checkpoint.position_m[1]).abs() < 0.01);
    assert_eq!(world.crashes(), 0);
}

#[test]
fn holding_reset_respawns_only_once() {
    let mut world = new_world();
    let reset = InputFrame {
        buttons: BUTTON_RESET,
        ..InputFrame::default()
    };

    let mut events = Vec::new();
    for _ in 0..PHYSICS_HZ {
        world.step(reset);
        events.extend(world.drain_events());
    }

    assert_eq!(count_respawns(&events), 1);
}

#[test]
fn the_clock_keeps_running_through_a_respawn() {
    let mut world = new_world();
    crash(&mut world);
    let steps_before = world.step_count();

    run_and_collect(&mut world, RESPAWN_DELAY_S);

    let expected_steps = steps_before + (RESPAWN_DELAY_S * PHYSICS_HZ as f32) as u64;
    assert_eq!(world.step_count(), expected_steps);
}
