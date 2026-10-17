//! Determinism tests (SPEC §7.10): the same inputs must give bit-identical results,
//! and a restored snapshot must continue exactly like the original.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_sim::golden_trace::{GOLDEN_TRACE_STEPS, run_golden_trace, scripted_input};
use nl_sim::vehicle_config::VehicleConfig;
use nl_sim::world::World;

const VEHICLE_TOML: &str = include_str!("../../../configs/vehicle.toml");
const GOLDEN_FLAT_DRIVE_JSON: &str = include_str!("golden/flat_drive.json");

fn new_world() -> World {
    let config = VehicleConfig::from_toml_str(VEHICLE_TOML).unwrap();
    World::new_on_flat_ground(&config, 1.0)
}

/// Steps `world` through the scripted inputs for steps `first..last`.
fn run_script(world: &mut World, first: u32, last: u32) {
    for step in first..last {
        world.step(scripted_input(step));
    }
}

#[test]
fn two_worlds_with_the_same_inputs_stay_bit_identical() {
    let mut first = new_world();
    let mut second = new_world();

    run_script(&mut first, 0, GOLDEN_TRACE_STEPS);
    run_script(&mut second, 0, GOLDEN_TRACE_STEPS);

    assert_eq!(first.snapshot().unwrap(), second.snapshot().unwrap());
}

#[test]
fn a_restored_snapshot_continues_exactly_like_the_original() {
    let mut original = new_world();
    run_script(&mut original, 0, 700);
    let saved = original.snapshot().unwrap();

    run_script(&mut original, 700, 1400);
    let mut restored = new_world();
    restored.restore(&saved).unwrap();
    run_script(&mut restored, 700, 1400);

    assert_eq!(restored.step_count(), 1400);
    assert_eq!(original.snapshot().unwrap(), restored.snapshot().unwrap());
}

#[test]
fn restoring_garbage_bytes_is_an_error_not_a_crash() {
    let mut world = new_world();

    let result = world.restore(&[1, 2, 3]);

    assert!(result.is_err());
}

#[test]
fn the_golden_trace_matches_the_stored_hash() {
    let expected: serde_json::Value = serde_json::from_str(GOLDEN_FLAT_DRIVE_JSON).unwrap();

    let result = run_golden_trace(VEHICLE_TOML).unwrap();

    assert_eq!(result.steps, expected["steps"].as_u64().unwrap() as u32);
    assert_eq!(
        result.hash_hex,
        expected["hash"].as_str().unwrap(),
        "physics output changed. If that was intended, regenerate the golden file \
         (see examples/golden_trace.rs) and explain why in the PR."
    );
}
