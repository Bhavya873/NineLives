//! Event tests in the full world (SPEC §7.9 and §12, with the explode-and-respawn
//! crash rules from docs/DECISIONS.md): landings, saves, crashes and Chaos.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, new_world, run_and_collect};
use nl_sim::events::{CrashCause, SimEvent};
use nl_sim::input::{BUTTON_CHAOS, InputFrame};
use rapier3d::prelude::Vector;

/// Rotation quaternions (x, y, z, w).
const UPSIDE_DOWN: [f32; 4] = [1.0, 0.0, 0.0, 0.0];
/// Rolled 90° about the forward axis: lying on its side.
const ON_ITS_SIDE: [f32; 4] = [
    std::f32::consts::FRAC_1_SQRT_2,
    0.0,
    0.0,
    std::f32::consts::FRAC_1_SQRT_2,
];

fn crash_causes(events: &[SimEvent]) -> Vec<CrashCause> {
    let mut causes = Vec::new();
    for event in events {
        if let SimEvent::Crash { cause } = event {
            causes.push(*cause);
        }
    }
    causes
}

#[test]
fn an_upright_2_m_drop_lands_safely_and_counts_as_a_save() {
    let mut world = new_world();
    // Resting centre-of-mass height is about 0.6 m, so this is a 2 m drop.
    world
        .place_car([0.0, 2.6, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();

    let events = run_and_collect(&mut world, 3.0);

    assert_eq!(crash_causes(&events), vec![]);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, SimEvent::Save { .. })),
        "{events:?}"
    );
}

#[test]
fn landing_upside_down_from_half_a_metre_is_a_crash() {
    let mut world = new_world();
    // The roof top is 0.55 m from the centre of mass, so this leaves a 0.5 m gap.
    world
        .place_car([0.0, 1.05, 0.0], UPSIDE_DOWN, [0.0; 3], [0.0; 3])
        .unwrap();

    let events = run_and_collect(&mut world, 2.0);

    assert_eq!(crash_causes(&events), vec![CrashCause::UpsideDown]);
    assert!(world.is_crashed());
}

#[test]
fn hitting_the_ground_side_first_at_8_m_s_is_a_hard_impact() {
    let mut world = new_world();
    // On its side the body's half-width (1.0 m) faces down; start 0.2 m above contact.
    world
        .place_car([0.0, 1.2, 0.0], ON_ITS_SIDE, [0.0, -8.0, 0.0], [0.0; 3])
        .unwrap();

    let events = run_and_collect(&mut world, 1.0);

    assert_eq!(crash_causes(&events), vec![CrashCause::HardImpact]);
}

#[test]
fn falling_out_of_the_world_is_a_crash() {
    let mut world = new_world();
    world
        .place_car([0.0, -25.0, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();

    let events = run_and_collect(&mut world, 0.1);

    assert_eq!(crash_causes(&events), vec![CrashCause::OutOfBounds]);
}

#[test]
fn placing_the_car_again_clears_a_crash() {
    let mut world = new_world();
    world
        .place_car([0.0, -25.0, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
    run_and_collect(&mut world, 0.1);
    assert!(world.is_crashed());

    world
        .place_car([0.0, 0.7, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();

    assert!(!world.is_crashed());
}

#[test]
fn chaos_knocks_the_car_and_repeats_for_the_same_seed() {
    let knock = InputFrame {
        buttons: BUTTON_CHAOS,
        ..InputFrame::default()
    };
    let mut first = new_world();
    let mut second = new_world();
    run_and_collect(&mut first, 1.0);
    run_and_collect(&mut second, 1.0);

    first.step(knock);
    second.step(knock);

    let speed_m_s = Vector::from_array(first.linear_velocity_m_s()).length();
    assert!(speed_m_s > 0.1, "chaos did not move the car");
    assert_eq!(first.snapshot().unwrap(), second.snapshot().unwrap());
}
