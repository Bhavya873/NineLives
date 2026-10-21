//! Course files and course worlds (SPEC §7.11): the JSON checks name what to fix, the
//! pieces turn into shapes in the right place and direction, and a car on a course
//! drives, lands and crashes out of bounds like on flat ground.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, realistic, run_and_collect, vehicle};
use nl_sim::course::{CourseError, CourseSpec, GAUNTLET_JSON};
use nl_sim::course_build::{CourseShape, ShapeGeometry, Surface, build_course_shapes};
use nl_sim::events::{CrashCause, SimEvent};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};
use rapier3d::prelude::Vector;

/// A course with one piece, given as JSON, inside a 200 × 80 × 100 m box.
fn course_with(piece_json: &str) -> String {
    format!(
        r#"{{"version": 1, "name": "Test", "start": {{"pos": [0, 0.7, 0]}},
           "bounds": {{"min": [-20, -20, -50], "max": [180, 60, 50]}},
           "pieces": [{piece_json}]}}"#
    )
}

fn parse(text: &str) -> Result<CourseSpec, CourseError> {
    CourseSpec::from_json_str(text)
}

/// The shapes a one-piece course adds on top of its floor.
fn piece_shapes(piece_json: &str) -> Vec<CourseShape> {
    let spec = parse(&course_with(piece_json)).unwrap();
    build_course_shapes(&spec)
        .into_iter()
        .filter(|shape| shape.piece.is_some())
        .collect()
}

/// A hull's or mesh's corner points in world coordinates.
fn world_points(shape: &CourseShape) -> Vec<[f32; 3]> {
    let points_m = match &shape.geometry {
        ShapeGeometry::Hull { points_m } | ShapeGeometry::Mesh { points_m, .. } => points_m,
        ShapeGeometry::Box { .. } => panic!("expected a hull or mesh, got a box"),
    };
    let pose = shape.pose.to_rapier();
    let mut points = Vec::new();
    for point in points_m {
        points.push((pose * Vector::from_array(*point)).to_array());
    }
    points
}

/// Every hull or mesh corner of these shapes, in world coordinates.
fn all_world_points(shapes: &[CourseShape]) -> Vec<[f32; 3]> {
    let mut points = Vec::new();
    for shape in shapes {
        points.extend(world_points(shape));
    }
    points
}

/// The point with the largest y.
fn highest_point(points: &[[f32; 3]]) -> [f32; 3] {
    let mut highest = points[0];
    for point in points {
        if point[1] > highest[1] {
            highest = *point;
        }
    }
    highest
}

/// How many shapes have this surface.
fn count_surface(shapes: &[CourseShape], surface: Surface) -> usize {
    shapes
        .iter()
        .filter(|shape| shape.surface == surface)
        .count()
}

fn course_world(text: &str) -> World {
    let spec = parse(text).unwrap();
    World::new_on_course(&vehicle(), realistic(), &spec, DEFAULT_GROUND_FRICTION, 0)
}

#[test]
fn the_gauntlet_course_parses_and_has_every_section() {
    let spec = parse(GAUNTLET_JSON).unwrap();

    let shapes = build_course_shapes(&spec);

    assert_eq!(spec.name, "Gauntlet");
    for wanted in [
        Surface::Ramp,
        Surface::Obstacle,
        Surface::LandingPad,
        Surface::KickPlate,
    ] {
        assert!(
            count_surface(&shapes, wanted) > 0,
            "no {wanted:?} in the Gauntlet"
        );
    }
    assert_eq!(count_surface(&shapes, Surface::Finish), 1);
    assert_eq!(count_surface(&shapes, Surface::Checkpoint), 8);
}

#[test]
fn an_error_names_the_piece_and_the_field() {
    let text = course_with(r#"{"type": "ramp", "pos": [40, 0, 0], "length": 12, "width": 6}"#);

    let message = parse(&text).unwrap_err().to_string();

    assert!(message.contains("piece 0 'ramp'"), "{message}");
    assert!(message.contains("angle_deg"), "{message}");
}

#[test]
fn a_piece_outside_the_bounds_is_refused() {
    let text = course_with(r#"{"type": "checkpoint", "pos": [500, 0, 0]}"#);

    let message = parse(&text).unwrap_err().to_string();

    assert!(message.contains("outside the course bounds"), "{message}");
}

#[test]
fn bounds_beyond_five_kilometres_are_refused() {
    let text = course_with(r#"{"type": "checkpoint", "pos": [40, 0, 0]}"#).replace("180", "1e9");

    let message = parse(&text).unwrap_err().to_string();

    assert!(message.contains("within"), "{message}");
}

#[test]
fn a_newer_version_is_refused_for_its_version() {
    let text = course_with(r#"{"type": "checkpoint", "pos": [40, 0, 0]}"#)
        .replace(r#""version": 1"#, r#""version": 2, "theme": "night""#);

    let message = parse(&text).unwrap_err().to_string();

    assert!(message.contains("`version` must be 1, not 2"), "{message}");
}

#[test]
fn a_nearly_flat_ramp_is_refused() {
    let text = course_with(
        r#"{"type": "ramp", "pos": [40, 0, 0], "length": 10, "width": 4, "angle_deg": 0.000001}"#,
    );

    assert!(parse(&text).is_err());
}

#[test]
fn an_unknown_piece_type_is_refused() {
    let text = course_with(r#"{"type": "loop", "pos": [40, 0, 0]}"#);

    assert!(parse(&text).is_err());
}

#[test]
fn a_gap_cuts_the_floor_in_two() {
    let spec = parse(&course_with(
        r#"{"type": "gap", "pos": [50, 0, 0], "length": 10}"#,
    ))
    .unwrap();

    let floors: Vec<CourseShape> = build_course_shapes(&spec)
        .into_iter()
        .filter(|shape| shape.surface == Surface::Floor)
        .collect();

    assert_eq!(floors.len(), 2);
    for floor in &floors {
        let ShapeGeometry::Box { half_extents_m } = floor.geometry else {
            panic!("floor should be a box");
        };
        let start_x = floor.pose.position_m[0] - half_extents_m[0];
        let end_x = floor.pose.position_m[0] + half_extents_m[0];
        assert!(
            end_x <= 50.0 + 1e-3 || start_x >= 60.0 - 1e-3,
            "floor {start_x}..{end_x} covers the gap"
        );
    }
}

#[test]
fn a_ramp_curves_up_along_its_heading_to_its_lip_angle() {
    let shapes = piece_shapes(
        r#"{"type": "ramp", "pos": [40, 0, 0], "length": 12, "width": 4, "angle_deg": 45}"#,
    );

    let points = all_world_points(&shapes);
    let last_segment = world_points(&shapes[shapes.len() - 1]);

    let top = highest_point(&points);
    assert!(
        (top[0] - 52.0).abs() < 1e-3,
        "the high end should be at the far end, x = {}",
        top[0]
    );
    // Curved, so lower than a straight 45° wedge (12 m) but well off the ground.
    assert!(top[1] > 4.0 && top[1] < 12.0, "top height {}", top[1]);
    // The lip (the last 2 m segment) leaves at the full 45°: it rises as much as it runs.
    let lip_start = highest_point(
        &last_segment
            .iter()
            .copied()
            .filter(|p| p[0] < 51.0)
            .collect::<Vec<_>>(),
    );
    let lip_rise_m = top[1] - lip_start[1];
    assert!(
        (lip_rise_m - 2.0).abs() < 1e-3,
        "lip rises {lip_rise_m} m over 2 m"
    );
}

#[test]
fn a_fast_car_drives_up_a_steep_ramp_without_crashing_at_its_foot() {
    // A straight 22° wedge crashed the car at its foot above about 25 m/s.
    let mut world = course_world(&course_with(
        r#"{"type": "ramp", "pos": [30, 0, 0], "length": 12, "width": 8, "angle_deg": 22}"#,
    ));
    world
        .place_car([20.0, 0.7, 0.0], LEVEL, [45.0, 0.0, 0.0], [0.0; 3])
        .unwrap();

    let events = run_and_collect(&mut world, 0.6);

    assert!(!world.is_crashed(), "events: {events:?}");
    let x_m = world.chassis_pose().position_m[0];
    assert!(x_m > 40.0, "car only reached x = {x_m}");
}

#[test]
fn a_ramp_turned_90_degrees_runs_towards_minus_z() {
    // Positive yaw turns left, like the car, and left is −Z (SPEC §4).
    let shapes = piece_shapes(
        r#"{"type": "ramp", "pos": [40, 0, 0], "yaw": 90, "length": 10, "width": 4, "angle_deg": 30}"#,
    );

    let points = all_world_points(&shapes);

    let top = highest_point(&points);
    assert!(
        (top[2] + 10.0).abs() < 1e-3,
        "the high end should be at z = −10, got {}",
        top[2]
    );
    // The top edge spans the 4 m width, which now lies along X.
    assert!((top[0] - 40.0).abs() <= 2.0 + 1e-3, "x = {}", top[0]);
}

#[test]
fn a_kicker_rolls_a_fast_car_without_crashing_it() {
    // A kicker whose inner side was a wall under the car jammed or crashed the chassis.
    let mut world = course_world(&course_with(
        r#"{"type": "kicker", "pos": [30, 0, 0], "length": 8, "width": 2, "angle_deg": 15, "side": "left"}"#,
    ));
    world
        .place_car([20.0, 0.7, 0.0], LEVEL, [30.0, 0.0, 0.0], [0.0; 3])
        .unwrap();

    let mut peak_tilt_rad: f32 = 0.0;
    for _ in 0..60 {
        run_and_collect(&mut world, 1.0 / 60.0);
        peak_tilt_rad = peak_tilt_rad.max(world.tilt_rad());
    }

    assert!(!world.is_crashed());
    assert!(
        peak_tilt_rad.to_degrees() > 20.0,
        "peak tilt {}°",
        peak_tilt_rad.to_degrees()
    );
}

#[test]
fn a_left_kicker_sits_on_the_left_minus_z_side() {
    let shapes = piece_shapes(
        r#"{"type": "kicker", "pos": [40, 0, 0], "length": 6, "width": 3, "angle_deg": 20, "side": "left"}"#,
    );

    for point in all_world_points(&shapes) {
        assert!(
            point[2] <= 1e-3,
            "kicker point {point:?} is right of the centre line"
        );
    }
}

#[test]
fn a_right_corkscrew_is_raised_on_the_right_plus_z_edge_and_highest_at_its_lip() {
    let shapes = piece_shapes(
        r#"{"type": "corkscrew", "pos": [40, 0, 0], "length": 15, "width": 4, "angle_deg": 15, "bank_deg": 45, "side": "right"}"#,
    );

    let top = highest_point(&all_world_points(&shapes));
    // The raised edge is the right (+Z) one, 2 m from the centre line, at the lip
    // (x = 55): about 15 · tan 15° / 2 ≈ 2 m plus 4 · tan 45° = 4 m above the floor.
    assert!((top[2] - 2.0).abs() < 1e-3, "z = {}", top[2]);
    assert!((top[0] - 55.0).abs() < 1e-3, "x = {}", top[0]);
    assert!((top[1] - 6.0).abs() < 0.1, "y = {}", top[1]);
}

#[test]
fn a_corkscrew_banked_past_eighty_degrees_is_refused_by_name() {
    let text = course_with(
        r#"{"type": "corkscrew", "pos": [40, 0, 0], "length": 15, "width": 4, "angle_deg": 15, "bank_deg": 85, "side": "left"}"#,
    );

    let message = parse(&text).unwrap_err().to_string();

    assert!(message.contains("piece 0 'corkscrew'"), "{message}");
    assert!(message.contains("bank_deg"), "{message}");
}

#[test]
fn a_car_row_has_one_box_per_car_along_the_course() {
    let shapes =
        piece_shapes(r#"{"type": "car_row", "pos": [40, 0, 0], "count": 5, "height": 1.4}"#);

    assert_eq!(shapes.len(), 5);
    let last_end_x = shapes[4].pose.position_m[0] + 4.5 / 2.0;
    assert!((last_end_x - (40.0 + 5.0 * 4.5 + 4.0 * 0.6)).abs() < 1e-3);
    assert!(
        shapes
            .iter()
            .all(|shape| (shape.pose.position_m[1] - 0.7).abs() < 1e-3)
    );
}

#[test]
fn markings_are_not_solid_and_obstacles_are() {
    let pad = piece_shapes(r#"{"type": "landing_pad", "pos": [40, 0, 0]}"#);
    let wall =
        piece_shapes(r#"{"type": "wall", "pos": [40, 0, 0], "height": 3, "with_ramp": true}"#);

    assert!(!pad[0].solid);
    // A wall with a ramp is one wall box plus the ramp's smooth mesh.
    assert_eq!(count_surface(&wall, Surface::Obstacle), 1);
    assert_eq!(count_surface(&wall, Surface::Ramp), 1);
    assert!(wall.iter().all(|shape| shape.solid));
}

#[test]
fn a_car_on_the_gauntlet_settles_on_its_wheels_at_the_start() {
    let mut world = course_world(GAUNTLET_JSON);

    let events = run_and_collect(&mut world, 1.0);

    assert!(!world.is_crashed(), "events: {events:?}");
    assert_eq!(world.wheels_touching_count(), 4);
    let position = world.chassis_pose().position_m;
    assert!(
        position[0].abs() < 0.1 && position[2].abs() < 0.1,
        "car moved to {position:?}"
    );
}

#[test]
fn a_car_driving_into_a_gap_falls_through_the_floor() {
    let mut world = course_world(&course_with(
        r#"{"type": "gap", "pos": [-5, 0, 0], "length": 10}"#,
    ));

    run_and_collect(&mut world, 1.0);

    assert!(
        world.chassis_pose().position_m[1] < -1.0,
        "the car should fall through the gap"
    );
}

#[test]
fn leaving_the_course_box_is_an_out_of_bounds_crash() {
    let mut world = course_world(&course_with(r#"{"type": "checkpoint", "pos": [40, 0, 0]}"#));
    world
        .place_car([0.0, 0.7, 49.0], LEVEL, [0.0, 0.0, 10.0], [0.0; 3])
        .unwrap();

    let events = run_and_collect(&mut world, 0.5);

    assert!(
        events.contains(&SimEvent::Crash {
            cause: CrashCause::OutOfBounds
        }),
        "events: {events:?}"
    );
}

#[test]
fn the_same_course_and_seed_give_the_same_run() {
    let mut first = course_world(GAUNTLET_JSON);
    let mut second = course_world(GAUNTLET_JSON);

    run_and_collect(&mut first, 2.0);
    run_and_collect(&mut second, 2.0);

    assert_eq!(first.chassis_pose(), second.chassis_pose());
}
