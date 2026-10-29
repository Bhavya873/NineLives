//! The curvature rules of docs/contracts/ramp-curvature.md, checked on every piece the
//! generator can make and on the playground, plus the arc-based spacing and the published
//! `v_complete_min_m_s` of a ramp with a gap.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_sim::course::{CourseSpec, PLAYGROUND_JSON, PieceSpec};
use nl_sim::course_build::{
    MAX_KINK_DEG, MIN_CONCAVE_RADIUS_M, ShapeGeometry, build_course_shapes, ramp_lip_height_m,
};
use nl_sim::course_gen::{
    DEFAULT_PIECES_JSON, PieceCatalog, PieceTemplate, flight_distance_m, generate_course,
};

const SEEDS: u64 = 300;
/// The largest lip angle of a piece that throws the car (see ramp-curvature.md).
const MAX_LIP_DEG: f32 = 8.0;

fn catalog() -> PieceCatalog {
    PieceCatalog::from_json_str(DEFAULT_PIECES_JSON).unwrap()
}

#[test]
fn the_catalog_and_the_builder_use_the_same_concave_radius() {
    assert_eq!(catalog().design.min_concave_radius_m, MIN_CONCAVE_RADIUS_M);
    assert!(catalog().design.max_kink_deg >= MAX_KINK_DEG);
}

/// The concave radius of a curved piece: length over the lip angle (corkscrew: over its
/// tangent), in m. None for pieces that are not curved ramps.
fn concave_radius_m(piece: &PieceSpec) -> Option<(f32, f32)> {
    match piece {
        PieceSpec::Ramp {
            length, angle_deg, ..
        }
        | PieceSpec::Kicker {
            length, angle_deg, ..
        } => Some((length / angle_deg.to_radians(), *angle_deg)),
        PieceSpec::Corkscrew {
            length, angle_deg, ..
        } => Some((length / angle_deg.to_radians().tan(), *angle_deg)),
        _ => None,
    }
}

#[test]
fn every_generated_curved_piece_has_the_design_radius_and_a_safe_lip() {
    let catalog = catalog();
    let mut seen = 0;
    for seed in 0..SEEDS {
        for piece in &generate_course(&catalog, seed).pieces {
            let Some((radius_m, angle_deg)) = concave_radius_m(piece) else {
                continue;
            };
            seen += 1;
            assert!(
                radius_m >= catalog.design.min_concave_radius_m - 0.01,
                "seed {seed}: radius {radius_m} m"
            );
            assert!(
                angle_deg <= MAX_LIP_DEG + 1e-3,
                "seed {seed}: lip {angle_deg}°"
            );
        }
    }
    assert!(seen > 300, "only {seen} curved pieces");
}

#[test]
fn the_playgrounds_curved_pieces_have_the_design_radius_and_a_safe_lip() {
    let course = CourseSpec::from_json_str(PLAYGROUND_JSON).unwrap();
    for piece in &course.pieces {
        if let Some((radius_m, angle_deg)) = concave_radius_m(piece) {
            assert!(
                radius_m >= MIN_CONCAVE_RADIUS_M - 0.5,
                "{piece:?}: {radius_m} m"
            );
            assert!(angle_deg <= MAX_LIP_DEG + 1e-3, "{piece:?}");
        }
    }
}

/// The side profile of a piece's solid at the edge `z`: (x, y) of its top points.
fn top_points(piece_json: &str) -> Vec<(f32, f32)> {
    let course = CourseSpec::from_json_str(&format!(
        r#"{{"version": 1, "name": "t", "start": {{"pos": [0, 0.7, 0], "yaw": 0}},
        "bounds": {{"min": [-50, -20, -50], "max": [400, 60, 50]}}, "pieces": [{piece_json}]}}"#
    ))
    .unwrap();
    let mut points = Vec::new();
    for shape in build_course_shapes(&course) {
        if let ShapeGeometry::Mesh { points_m, .. } = &shape.geometry {
            let z_max = points_m.iter().map(|p| p[2]).fold(f32::MIN, f32::max);
            points.extend(
                points_m
                    .iter()
                    .filter(|p| (p[2] - z_max).abs() < 1e-4 && p[1] > -0.1)
                    .map(|p| (p[0], p[1])),
            );
        }
    }
    points.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    points.dedup();
    points
}

/// The largest angle between neighbouring segments of a profile, in degrees, not counting
/// the lip: a corner at the profile's highest point is where the car takes off on purpose.
fn largest_kink_deg(points: &[(f32, f32)]) -> f32 {
    let mut largest: f32 = 0.0;
    let top_m = points.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    let slope = |a: (f32, f32), b: (f32, f32)| (b.1 - a.1).atan2(b.0 - a.0);
    for window in points.windows(3) {
        if (window[1].1 - top_m).abs() < 1e-4 {
            continue;
        }
        if window[1].0 - window[0].0 < 1e-3 || window[2].0 - window[1].0 < 1e-3 {
            continue;
        }
        let turn = (slope(window[1], window[2]) - slope(window[0], window[1])).abs();
        largest = largest.max(turn.to_degrees());
    }
    largest
}

#[test]
fn ramps_kickers_corkscrews_and_wall_ramps_have_no_kink_over_1_5_degrees() {
    let cases = [
        r#"{"type": "ramp", "pos": [10, 0, 0], "length": 15.4, "width": 8, "angle_deg": 8}"#,
        r#"{"type": "ramp", "pos": [10, 0, 0], "length": 60, "width": 8, "angle_deg": 30}"#,
        r#"{"type": "kicker", "pos": [10, 0, 0], "length": 15.4, "width": 4, "angle_deg": 8, "side": "left"}"#,
        r#"{"type": "corkscrew", "pos": [10, 0, 0], "length": 15.5, "width": 4, "angle_deg": 8, "bank_deg": 8, "side": "left"}"#,
        r#"{"type": "wall", "pos": [10, 0, 0], "height": 3, "with_ramp": true}"#,
    ];
    for case in cases {
        let kink = largest_kink_deg(&top_points(case));
        assert!(kink <= 1.5, "{case}: kink {kink}°");
    }
}

#[test]
fn a_clean_car_never_lands_on_the_next_generated_piece() {
    let catalog = catalog();
    let speed = catalog.design.speed_m_s;
    for seed in 0..SEEDS {
        let course = generate_course(&catalog, seed);
        for pair in course.pieces.windows(2) {
            let [from, to] = pair else { continue };
            let (start, length, angle_deg) = match from {
                PieceSpec::Ramp {
                    pos,
                    length,
                    angle_deg,
                    ..
                }
                | PieceSpec::Kicker {
                    pos,
                    length,
                    angle_deg,
                    ..
                } => (pos[0], *length, *angle_deg),
                _ => continue,
            };
            let lip = ramp_lip_height_m(length, angle_deg);
            let lands_at = start + length + flight_distance_m(speed, angle_deg, lip);
            // The next course piece (a gap, checkpoint or the next piece) starts after it.
            let next_x = to.position()[0];
            if matches!(to, PieceSpec::Gap { .. }) {
                continue;
            }
            assert!(
                next_x > lands_at,
                "seed {seed}: lands {lands_at}, next {next_x}"
            );
        }
    }
}

#[test]
fn the_published_ramp_gap_minimum_speed_clears_the_widest_gap_from_the_lowest_lip() {
    let catalog = catalog();
    let template = catalog
        .pieces
        .iter()
        .find_map(|piece| match piece {
            PieceTemplate::RampGap {
                angle_deg,
                gap_m,
                radius_m,
                v_complete_min_m_s,
                ..
            } => Some((*angle_deg, *gap_m, *radius_m, *v_complete_min_m_s)),
            _ => None,
        })
        .unwrap();
    let (angles, gaps, radii, v_min) = template;
    // The worst case: the lowest lip (smallest angle, shortest ramp) and the widest gap.
    let length = radii[0] * angles[0].to_radians();
    let lip = ramp_lip_height_m(length, angles[0]);

    let reach = flight_distance_m(v_min, angles[0], lip);

    assert!(
        reach >= gaps[1] + 0.5,
        "reach {reach} m over a {} m gap",
        gaps[1]
    );
}
