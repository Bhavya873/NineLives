//! Climbing obstacles (SPEC §7.11): a car driven at a ramp, a wall jump or a halfpipe
//! wall rides up it. Ramps built from separate convex segments once stopped the car dead
//! on their seams (a 22° ramp at 8 m/s), and the flat-bottomed body box dug its nose
//! into halfpipe curves, so the owner saw obstacles "stop the car from going up".

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, realistic, vehicle};
use nl_sim::course::CourseSpec;
use nl_sim::input::InputFrame;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// Approach speeds tried for every obstacle, in m/s.
const SPEEDS_M_S: [f32; 4] = [8.0, 12.0, 16.0, 20.0];
/// Four seconds of physics steps at 120 Hz.
const STEPS: usize = 480;
/// Steps over which a sudden stop is measured (0.1 s).
const WINDOW_STEPS: usize = 12;
/// Losing more speed than this in 0.1 s means the car hit something, in m/s.
const MAX_SPEED_LOSS_M_S: f32 = 5.0;

/// A course holding just `piece`.
fn course_with(piece: &str) -> CourseSpec {
    let json = format!(
        r#"{{"version": 1, "name": "test", "start": {{"pos": [0, 0.7, 0], "yaw": 0}},
        "bounds": {{"min": [-200, -20, -200], "max": [200, 60, 200]}}, "pieces": [{piece}]}}"#
    );
    CourseSpec::from_json_str(&json).unwrap()
}

/// What a full-throttle run at `speed_m_s` from `start_x` along +X did: the largest
/// speed loss over any 0.1 s, and the highest the centre of mass got, in m.
fn drive_at(piece: &str, start_x: f32, speed_m_s: f32) -> (f32, f32) {
    let course = course_with(piece);
    let mut world =
        World::new_on_course(&vehicle(), realistic(), &course, DEFAULT_GROUND_FRICTION, 0);
    world
        .place_car([start_x, 0.62, 0.0], LEVEL, [speed_m_s, 0.0, 0.0], [0.0; 3])
        .unwrap();
    let full_throttle = InputFrame {
        throttle: 127,
        ..InputFrame::default()
    };
    let mut speeds_m_s = Vec::new();
    let (mut worst_loss_m_s, mut highest_m) = (0.0_f32, 0.0_f32);
    for _ in 0..STEPS {
        world.step(full_throttle);
        let [vx, vy, vz] = world.linear_velocity_m_s();
        speeds_m_s.push((vx * vx + vy * vy + vz * vz).sqrt());
        if speeds_m_s.len() > WINDOW_STEPS {
            let before = speeds_m_s[speeds_m_s.len() - 1 - WINDOW_STEPS];
            worst_loss_m_s = worst_loss_m_s.max(before - speeds_m_s[speeds_m_s.len() - 1]);
        }
        highest_m = highest_m.max(world.chassis_pose().position_m[1]);
    }
    (worst_loss_m_s, highest_m)
}

#[test]
fn ramps_and_wall_jumps_never_stop_the_car_dead() {
    let pieces = [
        r#"{"type": "ramp", "pos": [30, 0, 0], "length": 12, "width": 8, "angle_deg": 15}"#,
        r#"{"type": "ramp", "pos": [30, 0, 0], "length": 15, "width": 8, "angle_deg": 22}"#,
        r#"{"type": "ramp", "pos": [30, 0, 0], "length": 15, "width": 8, "angle_deg": 28}"#,
        r#"{"type": "wall", "pos": [40, 0, 0], "height": 3, "with_ramp": true}"#,
    ];
    for piece in pieces {
        for speed_m_s in SPEEDS_M_S {
            let (worst_loss_m_s, highest_m) = drive_at(piece, -10.0, speed_m_s);

            assert!(
                worst_loss_m_s < MAX_SPEED_LOSS_M_S,
                "{piece} at {speed_m_s} m/s: lost {worst_loss_m_s:.1} m/s in 0.1 s"
            );
            assert!(
                highest_m > 3.0,
                "{piece} at {speed_m_s} m/s: only {highest_m:.1} m up"
            );
        }
    }
}

#[test]
fn a_car_driven_across_a_halfpipe_rides_up_past_the_top_of_the_wall() {
    for radius_m in [5.0_f32, 8.0] {
        let piece = format!(
            r#"{{"type": "halfpipe", "pos": [0, 0, -20], "yaw": -90, "radius": {radius_m}, "length": 40}}"#
        );
        for speed_m_s in SPEEDS_M_S {
            // Start on the 4 m floor, heading across the channel at the +X wall.
            let (_, highest_m) = drive_at(&piece, -0.8, speed_m_s);

            assert!(
                highest_m > radius_m,
                "radius {radius_m} m at {speed_m_s} m/s: only {highest_m:.1} m up"
            );
        }
    }
}
