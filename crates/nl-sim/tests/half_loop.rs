//! The half loop (M7 wrap-up, owner request): a car driven fast enough at it rides the
//! curve up through vertical and leaves its top upside down, heading back along −X.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, realistic, vehicle};
use nl_sim::course::CourseSpec;
use nl_sim::course_build::{ShapeGeometry, build_course_shapes};
use nl_sim::input::InputFrame;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// A course holding a half loop of `radius_m` starting at x = 0.
fn course_with_half_loop(radius_m: f32) -> CourseSpec {
    let json = format!(
        r#"{{"version": 1, "name": "test", "start": {{"pos": [0, 0.7, 0], "yaw": 0}},
        "bounds": {{"min": [-200, -20, -200], "max": [200, 60, 200]}},
        "pieces": [{{"type": "half_loop", "pos": [0, 0, 0], "radius": {radius_m}, "width": 8}}]}}"#
    );
    CourseSpec::from_json_str(&json).unwrap()
}

/// Six seconds of physics steps at 120 Hz.
const STEPS: usize = 720;

/// What a full-throttle run at `speed_m_s` from x = −10 into a half loop of `radius_m`
/// did: the highest the centre of mass got (m), the largest tilt (degrees) and the
/// speed along X at the highest point (m/s).
fn drive_into_half_loop(radius_m: f32, speed_m_s: f32) -> (f32, f32, f32) {
    let course = course_with_half_loop(radius_m);
    let mut world =
        World::new_on_course(&vehicle(), realistic(), &course, DEFAULT_GROUND_FRICTION, 0);
    world
        .place_car([-10.0, 0.62, 0.0], LEVEL, [speed_m_s, 0.0, 0.0], [0.0; 3])
        .unwrap();
    let full_throttle = InputFrame {
        throttle: 127,
        ..InputFrame::default()
    };
    let (mut highest_m, mut peak_tilt_rad, mut speed_x_at_top_m_s) = (0.0_f32, 0.0_f32, 0.0);
    for _ in 0..STEPS {
        world.step(full_throttle);
        let height_m = world.chassis_pose().position_m[1];
        if height_m > highest_m {
            highest_m = height_m;
            speed_x_at_top_m_s = world.linear_velocity_m_s()[0];
        }
        peak_tilt_rad = peak_tilt_rad.max(world.tilt_rad());
    }
    (highest_m, peak_tilt_rad.to_degrees(), speed_x_at_top_m_s)
}

#[test]
fn a_fast_car_goes_over_the_top_upside_down_and_heads_back() {
    for radius_m in [5.0_f32, 6.0] {
        for speed_m_s in [17.0_f32, 20.0, 24.0] {
            let (highest_m, peak_tilt_deg, speed_x_at_top_m_s) =
                drive_into_half_loop(radius_m, speed_m_s);

            let label = format!("radius {radius_m} m at {speed_m_s} m/s");
            assert!(
                highest_m > 1.75 * radius_m,
                "{label}: only {highest_m:.1} m up"
            );
            assert!(
                peak_tilt_deg > 160.0,
                "{label}: tilted only {peak_tilt_deg:.0}°"
            );
            assert!(
                speed_x_at_top_m_s < -3.0,
                "{label}: at the top it moved {speed_x_at_top_m_s:.1} m/s along X"
            );
        }
    }
}

#[test]
fn the_road_starts_flush_with_the_floor_and_tops_out_at_twice_the_radius() {
    let course = course_with_half_loop(6.0);

    let shapes = build_course_shapes(&course);

    let mut road_points = Vec::new();
    for shape in &shapes {
        if shape.piece == Some(0)
            && let ShapeGeometry::Mesh { points_m, .. } = &shape.geometry
        {
            road_points.extend(points_m.iter().copied());
        }
    }
    let lowest_y = road_points
        .iter()
        .map(|p| p[1])
        .fold(f32::INFINITY, f32::min);
    let highest_y = road_points.iter().map(|p| p[1]).fold(0.0, f32::max);
    let furthest_x = road_points.iter().map(|p| p[0]).fold(0.0, f32::max);
    assert!(
        lowest_y.abs() < 0.35,
        "the road starts {lowest_y} m off the floor"
    );
    assert!(
        (highest_y - 12.0).abs() < 0.35,
        "the top is at {highest_y} m"
    );
    assert!(
        (furthest_x - 6.0).abs() < 0.35,
        "the wall is at x = {furthest_x} m"
    );
}
