//! The game's open world (SPEC §11.6, `courses/playground.json`): it parses and builds
//! with every zone's pieces (the flip zone's corkscrews included), its spawn is clear so no Stage A drop starts against a
//! piece, the pieces sit well inside the bounds, and an episode on it runs to the end.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{realistic, scenarios, vehicle};
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::course::{CourseSpec, PLAYGROUND_JSON, PieceSpec};
use nl_sim::course_build::{CourseShape, ShapeGeometry, Surface, build_course_shapes};
use nl_sim::episode::{Episode, EpisodeStatus};
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::Stage;
use rapier3d::prelude::Vector;

/// Drop seeds checked against the spawn (the viewer's eval seeds and more).
const DROP_SEEDS: std::ops::Range<u64> = 0..128;
/// Farthest any part of the car reaches from its centre of mass, horizontally, in m.
const CAR_REACH_M: f32 = 3.5;
/// Every piece stays this far inside the bounds, so free driving rarely meets the edge.
const MIN_EDGE_MARGIN_M: f32 = 300.0;

/// Solid pieces keep at least this much clear floor between them, in m (a car row may sit
/// just past a ramp lip, so it is small).
const MIN_PIECE_GAP_M: f32 = 5.0;

fn playground() -> CourseSpec {
    CourseSpec::from_json_str(PLAYGROUND_JSON).unwrap()
}

fn start_drop(seed: u64) -> Episode {
    let plain = EpisodeVariation::none(&vehicle(), realistic());
    Episode::start_on_course(&plain, &scenarios(), Stage::A, &playground(), seed).unwrap()
}

/// A shape's corner points in world coordinates (a box's 8 corners or a hull's points).
fn world_corners(shape: &CourseShape) -> Vec<Vector> {
    let local: Vec<[f32; 3]> = match &shape.geometry {
        ShapeGeometry::Box {
            half_extents_m: [x, y, z],
        } => {
            let mut corners = Vec::new();
            for sx in [-1.0, 1.0] {
                for sy in [-1.0, 1.0] {
                    for sz in [-1.0, 1.0] {
                        corners.push([sx * x, sy * y, sz * z]);
                    }
                }
            }
            corners
        }
        ShapeGeometry::Hull { points_m } | ShapeGeometry::Mesh { points_m, .. } => points_m.clone(),
    };
    let pose = shape.pose.to_rapier();
    local
        .iter()
        .map(|point| pose * Vector::from_array(*point))
        .collect()
}

/// Horizontal distance from `point` to the box around `corners` in X and Z, in m
/// (0 when the point is over the box).
fn horizontal_distance_to(corners: &[Vector], point: [f32; 3]) -> f32 {
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for corner in corners {
        for (axis, value) in [corner.x, corner.z].into_iter().enumerate() {
            min[axis] = min[axis].min(value);
            max[axis] = max[axis].max(value);
        }
    }
    let dx = (min[0] - point[0]).max(point[0] - max[0]).max(0.0);
    let dz = (min[1] - point[2]).max(point[2] - max[1]).max(0.0);
    (dx * dx + dz * dz).sqrt()
}

/// Every solid shape the car could hit, other than the floor.
fn obstacles() -> Vec<CourseShape> {
    build_course_shapes(&playground())
        .into_iter()
        .filter(|shape| shape.solid && shape.surface != Surface::Floor)
        .collect()
}

#[test]
fn the_playground_has_every_zone_but_no_lines_or_gaps() {
    let spec = playground();

    let shapes = build_course_shapes(&spec);

    let count = |surface| shapes.iter().filter(|s| s.surface == surface).count();
    let is_halfpipe = |index: usize| matches!(spec.pieces[index], PieceSpec::Halfpipe { .. });
    let halfpipe_walls = shapes.iter().filter(|s| s.piece.is_some_and(is_halfpipe));
    assert_eq!(halfpipe_walls.count(), 4 * 2, "four halfpipes of two walls");
    assert!(
        count(Surface::Ramp) >= 15 + 4 * 2,
        "ramps, kickers, wall ramps and halfpipe walls"
    );
    assert!(count(Surface::Obstacle) >= 30, "car rows, bumps and walls");
    assert!(count(Surface::LandingPad) >= 5);
    assert!(count(Surface::KickPlate) >= 4);
    let corkscrews = spec.pieces.iter();
    let corkscrews = corkscrews.filter(|piece| matches!(piece, PieceSpec::Corkscrew { .. }));
    assert!(corkscrews.count() >= 3, "a flip zone of corkscrews");
    assert_eq!(count(Surface::Floor), 1, "one floor, no gaps");
    assert_eq!(count(Surface::Checkpoint) + count(Surface::Finish), 0);
}

#[test]
fn no_drop_starts_inside_or_against_a_piece() {
    let obstacles = obstacles();

    for seed in DROP_SEEDS {
        let episode = start_drop(seed);

        let centre = episode.world().chassis_pose().position_m;
        for shape in &obstacles {
            let distance_m = horizontal_distance_to(&world_corners(shape), centre);
            assert!(
                distance_m > CAR_REACH_M,
                "seed {seed}: the car starts {distance_m:.1} m from piece {:?}",
                shape.piece
            );
        }
    }
}

#[test]
fn every_piece_sits_well_inside_the_bounds() {
    let spec = playground();
    let (min, max) = (spec.bounds.min, spec.bounds.max);

    for shape in obstacles() {
        for corner in world_corners(&shape) {
            for (axis, value) in [(0, corner.x), (2, corner.z)] {
                let margin_m = (value - min[axis]).min(max[axis] - value);
                assert!(
                    margin_m > MIN_EDGE_MARGIN_M,
                    "piece {:?} is {margin_m:.0} m from the edge",
                    shape.piece
                );
            }
        }
    }
}

/// Horizontal box (min x, min z, max x, max z) around every solid shape of each piece.
fn piece_footprints() -> Vec<(usize, [f32; 4])> {
    let mut boxes: Vec<(usize, [f32; 4])> = Vec::new();
    for shape in obstacles() {
        let Some(piece) = shape.piece else { continue };
        let mut footprint = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for corner in world_corners(&shape) {
            footprint[0] = footprint[0].min(corner.x);
            footprint[1] = footprint[1].min(corner.z);
            footprint[2] = footprint[2].max(corner.x);
            footprint[3] = footprint[3].max(corner.z);
        }
        match boxes.iter_mut().find(|(index, _)| *index == piece) {
            Some((_, merged)) => {
                merged[0] = merged[0].min(footprint[0]);
                merged[1] = merged[1].min(footprint[1]);
                merged[2] = merged[2].max(footprint[2]);
                merged[3] = merged[3].max(footprint[3]);
            }
            None => boxes.push((piece, footprint)),
        }
    }
    boxes
}

#[test]
fn the_playground_is_spread_out_so_no_two_pieces_touch() {
    let spec = playground();
    let footprints = piece_footprints();
    let is_car_row = |index: usize| matches!(spec.pieces[index], PieceSpec::CarRow { .. });

    for (i, (first, a)) in footprints.iter().enumerate() {
        for (second, b) in &footprints[i + 1..] {
            // The car park is a block of rows on purpose; everything else keeps clear.
            if is_car_row(*first) && is_car_row(*second) {
                continue;
            }
            let gap_x = (a[0] - b[2]).max(b[0] - a[2]);
            let gap_z = (a[1] - b[3]).max(b[1] - a[3]);
            assert!(
                gap_x.max(gap_z) >= MIN_PIECE_GAP_M,
                "pieces {first} and {second} are {:.1} m apart",
                gap_x.max(gap_z)
            );
        }
    }
}

#[test]
fn drops_on_the_playground_end_without_errors() {
    for seed in 0..8 {
        let mut episode = start_drop(seed);

        let mut status = EpisodeStatus::Running;
        while status == EpisodeStatus::Running {
            status = episode.step([0.0; NOZZLE_COUNT]);
        }

        assert!(episode.result().duration_s > 0.0);
    }
}
