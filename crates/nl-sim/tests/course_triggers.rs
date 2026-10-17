//! Course triggers (SPEC §7.11): crossing a checkpoint line moves the respawn point,
//! crossing the finish line ends the run, and a kick plate pushes the car once per pass
//! in the direction its impulse says.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, realistic, run_and_collect, vehicle};
use nl_sim::course::CourseSpec;
use nl_sim::events::SimEvent;
use nl_sim::input::InputFrame;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// A straight course with these pieces, inside a 300 × 80 × 100 m box.
fn course_world(pieces_json: &str) -> World {
    let text = format!(
        r#"{{"version": 1, "name": "Test", "start": {{"pos": [0, 0.7, 0]}},
           "bounds": {{"min": [-20, -20, -50], "max": [280, 60, 50]}},
           "pieces": [{pieces_json}]}}"#
    );
    let spec = CourseSpec::from_json_str(&text).unwrap();
    World::new_on_course(&vehicle(), realistic(), &spec, DEFAULT_GROUND_FRICTION, 0)
}

/// Puts the car upright at `x_m` on the floor, rolling along +X at `speed_m_s`.
fn roll_from(world: &mut World, x_m: f32, speed_m_s: f32) {
    world
        .place_car([x_m, 0.7, 0.0], LEVEL, [speed_m_s, 0.0, 0.0], [0.0; 3])
        .unwrap();
}

/// Steps until a kick plate fires (at most 2 s).
fn step_until_kicked(world: &mut World) {
    for _ in 0..240 {
        world.step(InputFrame::default());
        let events = world.drain_events();
        if events
            .iter()
            .any(|event| matches!(event, SimEvent::Kicked { .. }))
        {
            return;
        }
    }
    panic!("the kick plate never fired");
}

#[test]
fn a_kick_plate_with_upward_impulse_throws_the_car_up() {
    let mut world = course_world(
        r#"{"type": "kick_plate", "pos": [20, 0, 0], "impulse": [0, 6000, 0], "angular": [0, 0, 0]}"#,
    );
    roll_from(&mut world, 17.0, 10.0);

    run_and_collect(&mut world, 0.5);

    assert!(world.linear_velocity_m_s()[1] > 1.0 || world.chassis_pose().position_m[1] > 1.0);
}

#[test]
fn a_kick_plate_with_plus_z_impulse_pushes_the_car_right() {
    let mut world = course_world(
        r#"{"type": "kick_plate", "pos": [20, 0, 0], "impulse": [0, 0, 4000], "angular": [0, 0, 0]}"#,
    );
    roll_from(&mut world, 17.0, 10.0);

    // Tyre grip soon kills the sideways speed, so look one step after the kick.
    step_until_kicked(&mut world);
    world.step(InputFrame::default());

    let z_speed_m_s = world.linear_velocity_m_s()[2];
    assert!(z_speed_m_s > 1.0, "z speed {z_speed_m_s}");
}

#[test]
fn a_kick_plate_kicks_once_per_pass() {
    let mut world = course_world(
        r#"{"type": "kick_plate", "pos": [20, 0, 0], "impulse": [0, 0, 2000], "angular": [0, 0, 0]}"#,
    );
    // Parked on the plate: one kick on arrival, none while it stays.
    roll_from(&mut world, 21.0, 0.0);

    let events = run_and_collect(&mut world, 2.0);

    let kicks = events
        .iter()
        .filter(|event| matches!(event, SimEvent::Kicked { .. }))
        .count();
    assert_eq!(kicks, 1);
}

#[test]
fn crossing_a_checkpoint_moves_the_respawn_point_there() {
    let mut world = course_world(r#"{"type": "checkpoint", "pos": [20, 0, 0]}"#);
    roll_from(&mut world, 15.0, 10.0);

    let events = run_and_collect(&mut world, 1.0);
    world.respawn();

    assert!(
        events.contains(&SimEvent::CheckpointReached { number: 0 }),
        "{events:?}"
    );
    let respawn_x = world.chassis_pose().position_m[0];
    assert!(
        (19.0..22.0).contains(&respawn_x),
        "respawned at x = {respawn_x}"
    );
    assert_eq!(world.linear_velocity_m_s(), [0.0; 3], "respawns at rest");
}

#[test]
fn checkpoints_count_in_order_and_never_go_back() {
    let mut world = course_world(
        r#"{"type": "checkpoint", "pos": [20, 0, 0]}, {"type": "checkpoint", "pos": [40, 0, 0]}"#,
    );
    roll_from(&mut world, 35.0, 10.0);
    run_and_collect(&mut world, 1.0);
    // Driving back over the first line must not move the respawn point back.
    roll_from(&mut world, 25.0, -10.0);

    let events = run_and_collect(&mut world, 1.0);

    assert_eq!(world.checkpoints_reached(), 2);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, SimEvent::CheckpointReached { .. }))
    );
}

#[test]
fn crossing_the_finish_reports_it_once() {
    let mut world = course_world(r#"{"type": "finish", "pos": [20, 0, 0]}"#);
    roll_from(&mut world, 15.0, 10.0);

    let events = run_and_collect(&mut world, 2.0);

    let finishes = events
        .iter()
        .filter(|event| matches!(event, SimEvent::Finished { .. }))
        .count();
    assert_eq!(finishes, 1);
    assert!(world.is_finished());
}

#[test]
fn jumping_over_a_line_still_counts() {
    let mut world = course_world(r#"{"type": "checkpoint", "pos": [20, 0, 0]}"#);
    world
        .place_car([15.0, 6.0, 0.0], LEVEL, [12.0, 0.0, 0.0], [0.0; 3])
        .unwrap();

    let events = run_and_collect(&mut world, 1.0);

    assert!(
        events.contains(&SimEvent::CheckpointReached { number: 0 }),
        "{events:?}"
    );
}

#[test]
fn a_flat_ground_world_has_no_checkpoints() {
    let world = common::new_world();

    assert_eq!(world.checkpoint_count(), 0);
    assert!(!world.is_finished());
}

#[test]
fn a_car_too_fast_to_land_inside_a_line_still_crosses_it() {
    // At 90 m/s the car moves 0.75 m per step, more than the 0.5 m line.
    let mut world = course_world(r#"{"type": "finish", "pos": [40, 0, 0]}"#);
    roll_from(&mut world, 20.0, 90.0);

    run_and_collect(&mut world, 0.5);

    assert!(world.is_finished());
}

#[test]
fn a_kick_that_throws_the_car_up_does_not_fire_again_when_it_lands_back() {
    let mut world = course_world(
        r#"{"type": "kick_plate", "pos": [20, 0, 0], "impulse": [0, 9000, 0], "angular": [0, 0, 0]}"#,
    );
    roll_from(&mut world, 21.0, 0.0);

    let events = run_and_collect(&mut world, 3.0);

    let kicks = events
        .iter()
        .filter(|event| matches!(event, SimEvent::Kicked { .. }))
        .count();
    assert_eq!(kicks, 1, "{events:?}");
}

#[test]
fn placing_the_car_back_onto_a_kick_plate_kicks_it_again() {
    let mut world = course_world(
        r#"{"type": "kick_plate", "pos": [20, 0, 0], "impulse": [0, 0, 2000], "angular": [0, 0, 0]}"#,
    );
    roll_from(&mut world, 21.0, 0.0);
    run_and_collect(&mut world, 1.0);
    roll_from(&mut world, 21.0, 0.0);

    let events = run_and_collect(&mut world, 1.0);

    assert!(
        events
            .iter()
            .any(|event| matches!(event, SimEvent::Kicked { .. }))
    );
}

#[test]
fn a_kick_plate_angular_impulse_about_plus_x_spins_the_car_about_plus_x() {
    let mut world = course_world(
        r#"{"type": "kick_plate", "pos": [20, 0, 0], "impulse": [0, 0, 0], "angular": [2500, 0, 0]}"#,
    );
    roll_from(&mut world, 17.0, 10.0);

    step_until_kicked(&mut world);
    world.step(InputFrame::default());

    let roll_rate_rad_s = world.angular_velocity_rad_s()[0];
    assert!(roll_rate_rad_s > 0.5, "roll rate {roll_rate_rad_s}");
}

#[test]
fn a_checkpoint_turned_90_degrees_respawns_the_car_facing_minus_z() {
    // Yaw +90° turns the line's run direction from +X to −Z (left turn, SPEC §4).
    let mut world = course_world(r#"{"type": "checkpoint", "pos": [40, 0, -20], "yaw": 90}"#);
    let facing_minus_z = [
        0.0,
        std::f32::consts::FRAC_1_SQRT_2,
        0.0,
        std::f32::consts::FRAC_1_SQRT_2,
    ];
    world
        .place_car(
            [40.0, 0.7, -15.0],
            facing_minus_z,
            [0.0, 0.0, -10.0],
            [0.0; 3],
        )
        .unwrap();

    let events = run_and_collect(&mut world, 1.0);
    world.respawn();

    assert!(
        events.contains(&SimEvent::CheckpointReached { number: 0 }),
        "{events:?}"
    );
    // The respawned car faces the line's heading, −Z: its rotation is +90° about +Y.
    let [qx, qy, qz, qw] = world.chassis_pose().rotation_xyzw;
    let expected = std::f32::consts::FRAC_1_SQRT_2;
    assert!(
        qx.abs() < 1e-3 && qz.abs() < 1e-3,
        "rotation {:?}",
        [qx, qy, qz, qw]
    );
    assert!((qy - expected).abs() < 1e-3 && (qw - expected).abs() < 1e-3);
}
