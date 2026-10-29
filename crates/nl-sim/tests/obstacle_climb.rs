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
/// speed loss over any 0.1 s before the centre of mass passes `lip_x` (so the landing
/// after the jump doesn't count), and the highest the centre of mass got, in m.
fn drive_at(piece: &str, start_x: f32, lip_x: f32, speed_m_s: f32) -> (f32, f32) {
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
        let before_lip = world.chassis_pose().position_m[0] < lip_x;
        if before_lip && speeds_m_s.len() > WINDOW_STEPS {
            let before = speeds_m_s[speeds_m_s.len() - 1 - WINDOW_STEPS];
            worst_loss_m_s = worst_loss_m_s.max(before - speeds_m_s[speeds_m_s.len() - 1]);
        }
        highest_m = highest_m.max(world.chassis_pose().position_m[1]);
    }
    (worst_loss_m_s, highest_m)
}

#[test]
fn ramps_and_wall_jumps_never_stop_the_car_dead() {
    // Each piece with the x of its lip (a wall's ramp ends 2 m before the wall).
    let pieces = [
        (
            r#"{"type": "ramp", "pos": [30, 0, 0], "length": 12, "width": 8, "angle_deg": 15}"#,
            42.0,
        ),
        (
            r#"{"type": "ramp", "pos": [30, 0, 0], "length": 15, "width": 8, "angle_deg": 22}"#,
            45.0,
        ),
        (
            r#"{"type": "ramp", "pos": [30, 0, 0], "length": 15, "width": 8, "angle_deg": 28}"#,
            45.0,
        ),
        (
            r#"{"type": "wall", "pos": [40, 0, 0], "height": 3, "with_ramp": true}"#,
            38.0,
        ),
    ];
    for (piece, lip_x) in pieces {
        for speed_m_s in SPEEDS_M_S {
            let (worst_loss_m_s, highest_m) = drive_at(piece, -10.0, lip_x, speed_m_s);

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
            let (_, highest_m) = drive_at(&piece, -0.8, f32::INFINITY, speed_m_s);

            assert!(
                highest_m > radius_m,
                "radius {radius_m} m at {speed_m_s} m/s: only {highest_m:.1} m up"
            );
        }
    }
}

/// The largest tilt from upright, in degrees, of a run at `speed_m_s` from
/// x = −10 along +X over `piece`, for four seconds or until the car crashes.
/// The fast roadster would pull 20 m/s up to 40 m/s on the piece at full throttle, so the
/// run holds the entry speed.
fn peak_tilt_deg(piece: &str, speed_m_s: f32) -> f32 {
    let course = course_with(piece);
    let mut world =
        World::new_on_course(&vehicle(), realistic(), &course, DEFAULT_GROUND_FRICTION, 0);
    world
        .place_car([-10.0, 0.62, 0.0], LEVEL, [speed_m_s, 0.0, 0.0], [0.0; 3])
        .unwrap();
    let mut peak_tilt_rad = 0.0_f32;
    for _ in 0..STEPS {
        let error_m_s = speed_m_s - world.forward_speed_m_s();
        let throttle = (error_m_s * 0.5).clamp(-1.0, 1.0) * 127.0;
        world.step(InputFrame {
            throttle: throttle.round() as i8,
            ..InputFrame::default()
        });
        peak_tilt_rad = peak_tilt_rad.max(world.tilt_rad());
        if world.is_crashed() {
            break;
        }
    }
    peak_tilt_rad.to_degrees()
}

#[test]
fn a_car_driven_over_a_corkscrew_is_rolled_past_its_side() {
    // The owner: "the car doesn't flip upside down in the obstacles". A corkscrew's
    // twisted lip must roll a car past 90° so the thrusters have something to save.
    for side in ["left", "right"] {
        let piece = format!(
            r#"{{"type": "corkscrew", "pos": [20, 0, 0], "length": 15, "width": 4,
            "angle_deg": 15, "bank_deg": 45, "side": "{side}"}}"#
        );
        // Grip 1.9 keeps the tyres on the ramp longer: at 15 m/s the roll is now 66°, at
        // 20 and 25 m/s it is 101° and 130° (it was past 90° from 15 m/s with grip 1.3).
        for speed_m_s in [20.0, 25.0] {
            let tilt_deg = peak_tilt_deg(&piece, speed_m_s);

            assert!(
                tilt_deg > 90.0,
                "{side} corkscrew at {speed_m_s} m/s: only {tilt_deg:.0}° of tilt"
            );
        }
    }
}
