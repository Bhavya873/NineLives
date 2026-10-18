//! The training viewer's obstacle arena (SPEC §11.6, `courses/arena.json`): it parses and
//! builds, its drop zone is clear so no Stage A drop starts inside or against a ramp or
//! block, and an episode on it runs the same way twice.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{realistic, scenarios, vehicle};
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::course::{ARENA_JSON, CourseSpec};
use nl_sim::course_build::{CourseShape, ShapeGeometry, Surface, build_course_shapes};
use nl_sim::episode::{Episode, EpisodeStatus};
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::Stage;
use rapier3d::prelude::Vector;

/// The viewer's 64 drops use eval seeds 0–63 (SPEC §11.6).
const VIEWER_SEEDS: std::ops::Range<u64> = 0..64;
/// Farthest any part of the car reaches from its centre of mass, horizontally, in m
/// (half of its 4.5 m length plus its overhang, rounded up generously).
const CAR_REACH_M: f32 = 3.5;

fn arena() -> CourseSpec {
    CourseSpec::from_json_str(ARENA_JSON).unwrap()
}

fn start_drop(seed: u64) -> Episode {
    let plain = EpisodeVariation::none(&vehicle(), realistic());
    Episode::start_on_course(&plain, &scenarios(), Stage::A, &arena(), seed).unwrap()
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
        ShapeGeometry::Hull { points_m } => points_m.clone(),
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
    build_course_shapes(&arena())
        .into_iter()
        .filter(|shape| shape.solid && shape.surface != Surface::Floor)
        .collect()
}

#[test]
fn the_arena_parses_and_has_ramps_and_blocks_but_no_lines() {
    let spec = arena();

    let shapes = build_course_shapes(&spec);

    let count = |surface| shapes.iter().filter(|s| s.surface == surface).count();
    assert!(count(Surface::Ramp) >= 4 * 6, "four ramps of six segments");
    assert!(count(Surface::Obstacle) >= 10, "blocks, bumps and walls");
    assert_eq!(count(Surface::Floor), 1, "one floor, no gaps");
    assert_eq!(count(Surface::Checkpoint) + count(Surface::Finish), 0);
}

#[test]
fn no_viewer_drop_starts_inside_or_against_an_obstacle() {
    let obstacles = obstacles();

    for seed in VIEWER_SEEDS {
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
fn no_viewer_drop_meets_an_obstacle_in_its_first_quarter_second() {
    // With nothing in its way, a drop on the arena follows the same path as the same drop
    // on flat ground; an obstacle at or near the start would bend it.
    let plain = EpisodeVariation::none(&vehicle(), realistic());
    for seed in VIEWER_SEEDS {
        let mut on_arena = start_drop(seed);
        let mut on_flat = Episode::start(&plain, &scenarios(), Stage::A, seed).unwrap();

        for _ in 0..30 {
            on_arena.step([0.0; NOZZLE_COUNT]);
            on_flat.step([0.0; NOZZLE_COUNT]);
        }

        let arena_m = on_arena.world().chassis_pose().position_m;
        let flat_m = on_flat.world().chassis_pose().position_m;
        for axis in 0..3 {
            let gap_m = (arena_m[axis] - flat_m[axis]).abs();
            assert!(
                gap_m < 1e-3,
                "seed {seed}: paths differ by {gap_m} m on axis {axis}"
            );
        }
    }
}

#[test]
fn drops_on_the_arena_end_without_errors() {
    for seed in 0..8 {
        let mut episode = start_drop(seed);

        let mut status = EpisodeStatus::Running;
        while status == EpisodeStatus::Running {
            status = episode.step([0.0; NOZZLE_COUNT]);
        }

        assert!(episode.result().duration_s > 0.0);
    }
}

#[test]
fn the_same_seed_gives_the_same_episode_on_the_arena() {
    let mut first = start_drop(7);
    let mut second = start_drop(7);
    // Fire a different nozzle each tenth of a second so the run is not just a fall.
    let throttles_at = |step: usize| {
        let mut throttles = [0.0; NOZZLE_COUNT];
        throttles[(step / 12) % NOZZLE_COUNT] = 1.0;
        throttles
    };

    for step in 0..600 {
        first.step(throttles_at(step));
        second.step(throttles_at(step));
    }

    assert_eq!(first.result(), second.result());
    assert_eq!(first.world().chassis_pose(), second.world().chassis_pose());
    assert_eq!(
        first.world().snapshot().unwrap(),
        second.world().snapshot().unwrap()
    );
}
