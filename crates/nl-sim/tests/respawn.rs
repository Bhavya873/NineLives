//! Respawn tests (SPEC §7.9, with the explode-and-respawn rule from docs/DECISIONS.md):
//! a crashed car stays frozen; with auto-respawn on it comes back at the last checkpoint
//! with a full tank after the delay, and the Reset button does the same at once.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, buttons, new_world, run_and_collect};
use nl_sim::conventions::{NOZZLE_COUNT, PHYSICS_HZ};
use nl_sim::events::SimEvent;
use nl_sim::input::BUTTON_RESET;
use nl_sim::world::World;

/// Below the out-of-bounds height, so the first step is a crash.
const OUT_OF_BOUNDS_M: [f32; 3] = [0.0, -25.0, 0.0];
/// The auto-respawn delay these tests turn on.
const RESPAWN_DELAY_S: f32 = 1.5;

/// A fresh world whose crashed car respawns by itself after `RESPAWN_DELAY_S`.
fn new_game_world() -> World {
    let mut world = new_world();
    world.enable_auto_respawn(RESPAWN_DELAY_S);
    world
}

/// Crashes the car by dropping it out of the world, then forgets those events.
fn crash(world: &mut World) {
    world
        .place_car(OUT_OF_BOUNDS_M, LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
    world.set_air_kg(10.0);
    run_and_collect(world, 1.0 / PHYSICS_HZ as f32);
    assert!(world.is_crashed());
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
    let mut world = new_game_world();
    crash(&mut world);
    let wreck = world.chassis_pose();

    let events = run_and_collect(&mut world, RESPAWN_DELAY_S - 0.1);

    assert_eq!(world.chassis_pose(), wreck);
    assert!(world.is_crashed());
    assert_eq!(count_respawns(&events), 0);
}

#[test]
fn after_the_delay_the_car_is_back_at_the_start_with_a_full_tank() {
    let mut world = new_game_world();
    let start = world.chassis_pose();
    crash(&mut world);

    let events = run_and_collect(&mut world, RESPAWN_DELAY_S);

    assert_eq!(count_respawns(&events), 1);
    assert!(!world.is_crashed());
    assert_eq!(world.air_fraction(), 1.0);
    assert_eq!(world.chassis_pose().position_m, start.position_m);
}

#[test]
fn without_auto_respawn_a_crash_stays_until_reset() {
    let mut world = new_world();
    crash(&mut world);

    let events = run_and_collect(&mut world, 3.0 * RESPAWN_DELAY_S);

    assert!(world.is_crashed());
    assert_eq!(count_respawns(&events), 0);
}

#[test]
fn pressing_reset_respawns_at_once_even_without_a_crash() {
    let mut world = new_world();
    world.set_checkpoint();
    let checkpoint = world.chassis_pose();
    world
        .place_car([30.0, 5.0, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();

    world.step(buttons(BUTTON_RESET));

    assert_eq!(count_respawns(&world.drain_events()), 1);
    let position_m = world.chassis_pose().position_m;
    // One physics step has run since the respawn, so allow a tiny settle.
    assert!((position_m[0] - checkpoint.position_m[0]).abs() < 0.01);
    assert!((position_m[1] - checkpoint.position_m[1]).abs() < 0.01);
}

#[test]
fn holding_reset_respawns_only_once() {
    let mut world = new_world();

    let mut events = Vec::new();
    for _ in 0..PHYSICS_HZ {
        world.step(buttons(BUTTON_RESET));
        events.extend(world.drain_events());
    }

    assert_eq!(count_respawns(&events), 1);
}

#[test]
fn the_clock_keeps_running_through_a_respawn() {
    let mut world = new_game_world();
    crash(&mut world);
    let steps_before = world.step_count();

    run_and_collect(&mut world, RESPAWN_DELAY_S);

    let expected_steps = steps_before + (RESPAWN_DELAY_S * PHYSICS_HZ as f32) as u64;
    assert_eq!(world.step_count(), expected_steps);
}

#[test]
fn a_crash_closes_every_valve_on_the_frozen_wreck() {
    let mut world = new_game_world();
    world
        .place_car(OUT_OF_BOUNDS_M, LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
    // Open every valve on the very step that crashes.
    world.set_controller_throttles([1.0; NOZZLE_COUNT]);

    run_and_collect(&mut world, 1.0 / PHYSICS_HZ as f32);

    assert!(world.is_crashed());
    assert_eq!(world.effective_throttles(), [0.0; NOZZLE_COUNT]);
}

#[test]
fn a_wreck_never_becomes_the_checkpoint() {
    let mut world = new_game_world();
    let start = world.chassis_pose();
    crash(&mut world);

    world.set_checkpoint();
    run_and_collect(&mut world, RESPAWN_DELAY_S);

    assert_eq!(world.chassis_pose().position_m, start.position_m);
}

#[test]
fn restoring_a_snapshot_keeps_the_auto_respawn_setting() {
    let snapshot_without_auto_respawn = new_world().snapshot().unwrap();
    let mut world = new_game_world();

    world.restore(&snapshot_without_auto_respawn).unwrap();
    crash(&mut world);
    let events = run_and_collect(&mut world, RESPAWN_DELAY_S);

    assert_eq!(count_respawns(&events), 1);
}
