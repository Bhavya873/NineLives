//! Traversal in a running world (docs/contracts/traversal.md, survival.md): a car entering
//! a ramp that leads to a gap too slowly raises `TooSlowFallback`, a fast one does not, and
//! the record shows a clean entry.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, realistic, vehicle};
use nl_sim::course::CourseSpec;
use nl_sim::events::SimEvent;
use nl_sim::input::InputFrame;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

fn course() -> CourseSpec {
    CourseSpec::from_json_str(
        r#"{"version": 1, "name": "gap", "start": {"pos": [0, 0.7, 0], "yaw": 0},
        "bounds": {"min": [-50, -20, -50], "max": [400, 60, 50]},
        "pieces": [
          {"type": "ramp", "pos": [60, 0, 0], "length": 11.5, "width": 8, "angle_deg": 6,
           "v_complete_min_m_s": 14},
          {"type": "gap", "pos": [71.5, 0, 0], "length": 6, "pit_depth": 0.5, "entry_height": 0.6}
        ]}"#,
    )
    .unwrap()
}

fn drive(speed_m_s: f32) -> (World, Vec<SimEvent>) {
    let mut world = World::new_on_course(
        &vehicle(),
        realistic(),
        &course(),
        DEFAULT_GROUND_FRICTION,
        0,
    );
    for _ in 0..120 {
        world.step(InputFrame::default());
    }
    let y = world.chassis_pose().position_m[1];
    world
        .place_car([40.0, y, 0.0], LEVEL, [speed_m_s, 0.0, 0.0], [0.0; 3])
        .unwrap();
    let mut events = Vec::new();
    for _ in 0..(6 * 120) {
        let error = speed_m_s - world.forward_speed_m_s();
        let throttle = ((error * 0.5).clamp(-1.0, 1.0) * 127.0).round() as i8;
        world.step(InputFrame {
            throttle,
            ..InputFrame::default()
        });
        events.extend(world.drain_events());
    }
    (world, events)
}

#[test]
fn a_slow_clean_entry_raises_too_slow_fallback_and_drives_out_of_the_pit() {
    let (world, events) = drive(8.0);

    let too_slow = events
        .iter()
        .any(|event| matches!(event, SimEvent::TooSlowFallback { piece: 0, .. }));
    assert!(too_slow, "{events:?}");
    assert!(!world.is_crashed(), "the fallback must be safe: {events:?}");
    let record = world.traversal().finished().first().copied();
    assert!(record.is_some_and(|r| r.approach.clean && r.too_slow));
}

#[test]
fn a_fast_clean_entry_clears_the_gap_without_the_fallback() {
    let (world, events) = drive(25.0);

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, SimEvent::TooSlowFallback { .. })),
        "{events:?}"
    );
    assert!(!world.is_crashed(), "{events:?}");
    assert!(world.chassis_pose().position_m[0] > 78.0);
}

#[test]
fn the_approach_hint_shows_before_a_slow_entry_and_not_for_a_fast_one() {
    for (speed_m_s, expect_hint) in [(8.0, true), (25.0, false)] {
        let mut world = World::new_on_course(
            &vehicle(),
            realistic(),
            &course(),
            DEFAULT_GROUND_FRICTION,
            0,
        );
        for _ in 0..120 {
            world.step(InputFrame::default());
        }
        let y = world.chassis_pose().position_m[1];
        world
            .place_car([40.0, y, 0.0], LEVEL, [speed_m_s, 0.0, 0.0], [0.0; 3])
            .unwrap();
        // 20 m before the ramp, still driving up to it.
        for _ in 0..30 {
            world.step(InputFrame::default());
        }
        assert!(world.traversal().active().is_none());
        let hint = world.approach_hint_m_s();
        assert_eq!(hint > 0.0, expect_hint, "speed {speed_m_s}: hint {hint}");
        if expect_hint {
            assert_eq!(hint, 14.0);
        }
    }
}

#[test]
fn the_hint_never_changes_a_run() {
    // Reading it twice around a step must not move the car: it is read-only.
    let mut a = World::new_on_course(
        &vehicle(),
        realistic(),
        &course(),
        DEFAULT_GROUND_FRICTION,
        0,
    );
    let mut b = World::new_on_course(
        &vehicle(),
        realistic(),
        &course(),
        DEFAULT_GROUND_FRICTION,
        0,
    );
    for _ in 0..240 {
        let _ = a.approach_hint_m_s();
        a.step(InputFrame::default());
        b.step(InputFrame::default());
    }
    assert_eq!(a.chassis_pose().position_m, b.chassis_pose().position_m);
}
